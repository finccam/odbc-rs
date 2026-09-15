//! R-thread-only ownership. Arc is used for the crate's owned statement API,
//! not to imply concurrent access to R or to the same statement.
use crate::{
    error::{DriverError, Result},
    fetch::Fetcher,
    statement::{NativeStatement, Parameters, StatementOwner},
    types::{Column, Options, Values},
};
use odbc_api::{
    handles::{AsStatementRef, Statement},
    Connection, ConnectionOptions, ConnectionTransitions, Cursor,
};
use ouroboros::self_referencing;
use std::{
    cell::{Cell, RefCell},
    panic::{catch_unwind, AssertUnwindSafe},
    rc::{Rc, Weak},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn identity() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
#[self_referencing]
pub struct ActiveResult {
    owner: StatementOwner,
    #[borrows(mut owner)]
    #[not_covariant]
    fetcher: Option<Fetcher<'this>>,
}

enum Resource {
    Ready(StatementOwner),
    Active(ActiveResult),
    Failed,
    Cleared,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConnectionStatus {
    Open,
    Closing,
    Closed,
    Uncertain,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ResultKind {
    Query,
    Statement,
}

pub struct ConnectionInfo {
    pub dbms_name: Option<String>,
    pub database: Option<String>,
}

pub struct TableEntry {
    pub catalog: Option<String>,
    pub schema: Option<String>,
    pub name: String,
    pub table_type: String,
}

pub struct ConnectionState {
    pub id: u64,
    pub pid: u32,
    pub status: Cell<ConnectionStatus>,
    native: RefCell<Option<Arc<Connection<'static>>>>,
    results: RefCell<Vec<Weak<RefCell<ResultState>>>>,
    pub options: Options,
}

pub struct ResultState {
    pub id: u64,
    pub connection_id: u64,
    pid: u32,
    connection: Weak<ConnectionState>,
    // Cleared results retain metadata but do not keep abandoned sessions open.
    lease: Option<Rc<ConnectionState>>,
    pub sql: String,
    // Direct execution has no prepared metadata: the driver validates its
    // marker count during execution, without a second prepare or SQL parser.
    pub parameter_count: Option<usize>,
    pub immediate: bool,
    pub kind: ResultKind,
    resource: Resource,
    pub columns: Vec<Column>,
    pub delivered: u64,
    pub affected: Option<usize>,
    pub native_attempt: Option<NativeAttempt>,
    pub consumption_error: Option<DriverError>,
    pub bound: bool,
    pub batch_size: usize,
}

#[derive(Clone, Debug)]
// Prepared::execute also binds parameters and probes result-column count. These
// are private native-call diagnostics, not the future execution observer events.
pub struct NativeAttempt {
    pub id: u64,
    pub outcome: &'static str,
    pub duration_seconds: f64,
    pub batch_size: usize,
}

fn invalid(operation: &'static str, message: &str) -> DriverError {
    DriverError::new(operation, "state", message)
}

impl ConnectionState {
    pub fn connect(connection_string: &str, options: Options, timeout: u32) -> Result<Rc<Self>> {
        let env = odbc_api::environment().map_err(|e| DriverError::odbc("connect", e))?;
        let connection = env
            .connect_with_connection_string(
                connection_string,
                ConnectionOptions {
                    login_timeout_sec: Some(timeout),
                    ..Default::default()
                },
            )
            .map_err(|e| DriverError::odbc("connect", e))?;
        Ok(Rc::new(Self {
            id: identity(),
            pid: std::process::id(),
            status: Cell::new(ConnectionStatus::Open),
            native: RefCell::new(Some(Arc::new(connection))),
            results: RefCell::new(vec![]),
            options,
        }))
    }

    pub fn check(&self, operation: &'static str) -> Result<()> {
        if self.pid != std::process::id() {
            return Err(invalid(
                operation,
                "Connection belongs to another process; reconnect in this process",
            ));
        }
        if self.status.get() != ConnectionStatus::Open {
            let mut error = invalid(
                operation,
                "Connection is closed or uncertain; dispose of it",
            );
            error.uncertain = self.status.get() == ConnectionStatus::Uncertain;
            return Err(error);
        }
        Ok(())
    }

    pub fn prepare(
        self: &Rc<Self>,
        sql: String,
        kind: ResultKind,
        immediate: bool,
    ) -> Result<Rc<RefCell<ResultState>>> {
        self.check("prepare")?;
        let connection = self
            .native
            .try_borrow()
            .map_err(|_| invalid("prepare", "Connection is already borrowed"))?
            .as_ref()
            .cloned()
            .ok_or_else(|| invalid("prepare", "Connection is closed"))?;
        let on_error = |e| {
            let error = DriverError::odbc("prepare", e);
            self.observe_error(&error);
            error
        };
        let (statement, parameter_count) = if immediate {
            (
                NativeStatement::Direct(connection.into_preallocated().map_err(on_error)?),
                None,
            )
        } else {
            let mut statement = connection.into_prepared(&sql).map_err(on_error)?;
            let count = statement.num_params().map_err(on_error)? as usize;
            (NativeStatement::Prepared(statement), Some(count))
        };
        let result = Rc::new(RefCell::new(ResultState {
            id: identity(),
            connection_id: self.id,
            pid: self.pid,
            connection: Rc::downgrade(self),
            lease: Some(self.clone()),
            sql,
            parameter_count,
            immediate,
            kind,
            resource: Resource::Ready(StatementOwner {
                statement,
                parameters: Parameters::default(),
            }),
            columns: vec![],
            delivered: 0,
            affected: None,
            native_attempt: None,
            consumption_error: None,
            bound: false,
            batch_size: 0,
        }));
        let mut registry = self.results.borrow_mut();
        registry.retain(|r| r.strong_count() > 0);
        registry.push(Rc::downgrade(&result));
        Ok(result)
    }

    pub fn observe_error(&self, error: &DriverError) {
        if error.uncertain {
            self.status.set(ConnectionStatus::Uncertain);
        }
    }

    pub fn local(&self) -> bool {
        self.pid == std::process::id()
    }

    pub fn has_native(&self) -> bool {
        self.native.borrow().is_some()
    }

    pub fn active_results(&self) -> usize {
        self.results
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|r| r.try_borrow().map_or(true, |r| r.has_resource()))
            .count()
    }

    /// SQL_ATTR_CONNECTION_DEAD is a driver check, not a SELECT health probe.
    /// Some drivers do not implement it; local validity remains useful there.
    pub fn is_valid(&self) -> bool {
        if self.check("validity").is_err() {
            return false;
        }
        let native = self.native.borrow();
        let Some(native) = native.as_ref() else {
            return false;
        };
        match native.is_dead() {
            Ok(false) => true,
            Err(error) if unsupported(&error) => true,
            _ => {
                self.status.set(ConnectionStatus::Uncertain);
                false
            }
        }
    }

    pub fn info(&self) -> Result<ConnectionInfo> {
        self.check("connection_info")?;
        let native = self.native.borrow();
        let native = native
            .as_ref()
            .ok_or_else(|| invalid("connection_info", "Connection has been released"))?;
        let read = |value: std::result::Result<String, odbc_api::Error>| match value {
            Ok(value) => Ok(Some(value)),
            Err(error) if unsupported(&error) => Ok(None),
            Err(error) => {
                let error = DriverError::odbc("connection_info", error);
                self.observe_error(&error);
                Err(error)
            }
        };
        let dbms_name = read(native.database_management_system_name())?;
        let database = read(native.current_catalog())?;
        Ok(ConnectionInfo {
            dbms_name,
            database,
        })
    }

    pub fn tables(&self, catalog: &str, schema: &str, table: &str) -> Result<Vec<TableEntry>> {
        self.check("tables")?;
        let native = self.native.borrow();
        let native = native
            .as_ref()
            .ok_or_else(|| invalid("tables", "Connection is closed"))?;
        let catalog = if catalog.is_empty() {
            native
                .current_catalog()
                .map_err(|e| DriverError::odbc("tables", e))?
        } else {
            catalog.to_owned()
        };
        let mut statement = native
            .preallocate()
            .map_err(|e| DriverError::odbc("tables", e))?;
        let mut cursor = statement
            .tables_cursor(&catalog, schema, table, "TABLE,VIEW")
            .map_err(|e| DriverError::odbc("tables", e))?;
        let mut entries = vec![];
        while let Some(mut row) = cursor
            .next_row()
            .map_err(|e| DriverError::odbc("tables", e))?
        {
            let mut text = |index| -> Result<Option<String>> {
                let mut units = vec![];
                if !row
                    .get_wide_text(index, &mut units)
                    .map_err(|e| DriverError::odbc("tables", e))?
                {
                    return Ok(None);
                }
                Ok(Some(String::from_utf16(&units).map_err(|_| {
                    invalid("tables", "Invalid metadata encoding")
                })?))
            };
            entries.push(TableEntry {
                catalog: text(1)?,
                schema: text(2)?,
                name: text(3)?.ok_or_else(|| invalid("tables", "Missing table name"))?,
                table_type: text(4)?.unwrap_or_default(),
            });
        }
        Ok(entries)
    }

    pub fn close(&self) -> Result<()> {
        if self.pid != std::process::id() {
            return Err(invalid(
                "disconnect",
                "Connection belongs to another process",
            ));
        }
        if self.status.get() == ConnectionStatus::Closed {
            return Ok(());
        }
        let results: Vec<_> = self
            .results
            .borrow()
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        // Acquire every borrow before changing state or releasing any resources.
        let mut guards = results
            .iter()
            .map(|r| {
                r.try_borrow_mut()
                    .map_err(|_| invalid("disconnect", "A result is in use"))
            })
            .collect::<Result<Vec<_>>>()?;
        self.status.set(ConnectionStatus::Closing);
        let mut first_error = None;
        for result in &mut guards {
            if let Err(error) = result.clear() {
                first_error.get_or_insert(error);
            }
        }
        let native = self.native.borrow_mut().take();
        if let Err(error) = release(native, "disconnect") {
            first_error.get_or_insert(error);
        }
        self.status.set(if first_error.is_some() {
            ConnectionStatus::Uncertain
        } else {
            ConnectionStatus::Closed
        });
        first_error.map_or(Ok(()), Err)
    }
}

impl ResultState {
    pub fn connection(&self) -> Result<Rc<ConnectionState>> {
        self.connection
            .upgrade()
            .ok_or_else(|| invalid("native", "Connection has been released"))
    }

    pub fn valid(&self) -> bool {
        self.connection().is_ok_and(|c| c.check("validity").is_ok())
            && !matches!(self.resource, Resource::Cleared | Resource::Failed)
    }

    pub fn local(&self) -> bool {
        self.pid == std::process::id()
    }

    pub fn has_resource(&self) -> bool {
        matches!(self.resource, Resource::Ready(_) | Resource::Active(_))
    }

    pub fn check(&self, operation: &'static str) -> Result<()> {
        if !self.local() {
            return Err(invalid(operation, "Result belongs to another process"));
        }
        if !self.has_resource() {
            return Err(invalid(operation, "Result has been cleared or has failed"));
        }
        self.connection()?.check(operation)
    }

    pub fn status(&self) -> &'static str {
        if !self.local() {
            return "invalid";
        }
        match &self.resource {
            Resource::Cleared => "cleared",
            Resource::Failed => "failed",
            _ if !self.valid() => "invalid",
            _ if self.kind == ResultKind::Statement && self.bound => "completed",
            Resource::Ready(_) if !self.bound => {
                if self.immediate {
                    "allocated"
                } else {
                    "prepared"
                }
            }
            Resource::Ready(_) => "completed",
            Resource::Active(_) if self.complete() => "exhausted",
            Resource::Active(_) => "fetchable",
        }
    }

    pub fn column_info(&mut self) -> Result<Vec<Column>> {
        self.check("column_info")?;
        if !self.bound {
            if self.immediate {
                return Err(invalid(
                    "column_info",
                    "Direct statement must be executed before requesting columns",
                ));
            }
            if let Resource::Ready(owner) = &mut self.resource {
                let options = self
                    .lease
                    .as_ref()
                    .ok_or_else(|| invalid("column_info", "Connection has been released"))?
                    .options
                    .clone();
                self.columns =
                    crate::types::describe(&mut owner.statement, &options).map_err(|error| {
                        if let Some(connection) = self.connection.upgrade() {
                            connection.observe_error(&error);
                        }
                        error
                    })?;
            }
        }
        Ok(self.columns.clone())
    }

    fn recover_statement(&mut self) -> Result<StatementOwner> {
        match std::mem::replace(&mut self.resource, Resource::Failed) {
            Resource::Ready(owner) => Ok(owner),
            Resource::Active(mut active) => {
                if let Err(error) =
                    active.with_fetcher_mut(|fetcher| fetcher.take().map_or(Ok(()), Fetcher::close))
                {
                    if let Ok(conn) = self.connection() {
                        conn.status.set(ConnectionStatus::Uncertain);
                    }
                    let _ = release(active, "rebind_cleanup");
                    return Err(error);
                }
                Ok(active.into_heads().owner)
            }
            Resource::Cleared => {
                self.resource = Resource::Cleared;
                Err(invalid("bind", "Result has been cleared"))
            }
            Resource::Failed => Err(invalid(
                "bind",
                "Result has failed; clear it and prepare a new result",
            )),
        }
    }

    /// One native parameter-array execution, never a scalar fallback loop.
    pub fn execute(&mut self, parameters: Parameters) -> Result<()> {
        self.check("execute")?;
        let connection = self.connection()?;
        if let Some(expected) = self
            .parameter_count
            .filter(|expected| *expected != parameters.len())
        {
            return Err(DriverError::new(
                "bind",
                "parameter",
                format!(
                    "Expected {} parameters; received {}",
                    expected,
                    parameters.len()
                ),
            ));
        }
        let rows = parameters.rows;
        if rows == 0 && self.kind == ResultKind::Query && self.immediate {
            return Err(invalid(
                "bind",
                "Empty query batches require prepared execution for typed metadata",
            ));
        }
        let mut owner = self.recover_statement()?;
        if let Err(error) = owner.statement.reset_bindings() {
            connection.status.set(ConnectionStatus::Uncertain);
            let mut error = DriverError::odbc("bind_reset", error);
            error.uncertain = true;
            let _ = release(owner, "bind_reset_cleanup");
            return Err(error);
        }
        owner.parameters = parameters;
        self.delivered = 0;
        self.affected = None;
        self.columns.clear();
        self.consumption_error = None;
        self.native_attempt = None;
        self.bound = false;
        self.batch_size = rows;
        let options = connection.options.clone();
        if rows == 0 {
            self.columns = if self.kind == ResultKind::Query {
                crate::types::describe(&mut owner.statement, &options)?
            } else {
                vec![]
            };
            self.affected = Some(0);
            self.bound = true;
            self.resource = Resource::Ready(owner);
            return Ok(());
        }
        let sql = &self.sql;
        let kind = self.kind;
        let affected = Cell::new(None);
        let drained = Cell::new(false);
        let attempt_id = identity();
        let attempt = RefCell::new(None);
        let active: Result<ActiveResult> = ActiveResultTryBuilder {
            owner,
            fetcher_builder: |owner: &mut StatementOwner| {
                let start = Instant::now();
                let result = owner.statement.execute(sql, &mut owner.parameters);
                let duration_seconds = start.elapsed().as_secs_f64();
                let outcome = match &result {
                    Ok(_) => {
                        if owner.parameters.failure().is_some() {
                            "failure"
                        } else {
                            "success"
                        }
                    }
                    Err(error) => match error {
                        odbc_api::Error::Diagnostics { record, .. } => {
                            match record.state.as_str() {
                                "HYT00" | "HYT01" => "timeout",
                                "HY008" => "cancelled",
                                _ => "failure",
                            }
                        }
                        _ => "failure",
                    },
                };
                *attempt.borrow_mut() = Some(NativeAttempt {
                    id: attempt_id,
                    outcome,
                    duration_seconds,
                    batch_size: rows,
                });
                let cursor = result.map_err(|e| {
                    let mut error = DriverError::odbc("execute", e);
                    owner.parameters.annotate_failure(&mut error);
                    error
                })?;
                if let Some(error) = owner.parameters.failure() {
                    drop(cursor);
                    return Err(error);
                }
                if let Some(mut cursor) = cursor {
                    if kind == ResultKind::Statement && rows > 1 {
                        let mut stmt = cursor.into_stmt();
                        affected.set(drain_affected(&mut stmt).map_err(|mut e| {
                            owner.parameters.annotate_failure(&mut e);
                            e
                        })?);
                        drained.set(true);
                        return Ok(None);
                    }
                    if kind == ResultKind::Statement {
                        let mut statement = cursor.as_stmt_ref();
                        let count = statement
                            .row_count()
                            .into_result_without_logging(&statement)
                            .map_err(|e| DriverError::odbc("rows_affected", e))?;
                        affected.set(usize::try_from(count).ok());
                    }
                    Ok(Some(Fetcher::new(cursor, options, rows > 1)?))
                } else {
                    Ok(None)
                }
            },
        }
        .try_build();
        self.native_attempt = attempt.into_inner();
        match active {
            Ok(active) => {
                self.bound = true;
                self.columns = active
                    .with_fetcher(|f| f.as_ref().map_or_else(Vec::new, |f| f.columns.clone()));
                if active.with_fetcher(|f| f.is_none()) {
                    let mut owner = active.into_heads().owner;
                    let affected = if kind == ResultKind::Statement && rows > 1 {
                        if drained.get() {
                            Ok(affected.get())
                        } else {
                            drain_affected(&mut owner.statement.as_stmt_ref()).map_err(|mut e| {
                                owner.parameters.annotate_failure(&mut e);
                                e
                            })
                        }
                    } else if kind == ResultKind::Statement {
                        owner
                            .statement
                            .row_count()
                            .map_err(|e| DriverError::odbc("rows_affected", e))
                    } else {
                        Ok(None)
                    };
                    let batch_error = owner.parameters.failure();
                    self.resource = Resource::Ready(owner);
                    if let Some(error) = batch_error {
                        if let Some(attempt) = &mut self.native_attempt {
                            attempt.outcome = "failure";
                        }
                        let resource = std::mem::replace(&mut self.resource, Resource::Failed);
                        let _ = release(resource, "batch_cleanup");
                        return Err(error);
                    }
                    match affected {
                        Ok(n) => self.affected = n,
                        Err(error) => {
                            connection.observe_error(&error);
                            self.consumption_error = Some(error.clone());
                            return Err(error);
                        }
                    }
                } else {
                    self.affected = affected.get();
                    self.resource = Resource::Active(active);
                }
                Ok(())
            }
            Err(error) => {
                if self
                    .native_attempt
                    .as_ref()
                    .is_some_and(|e| e.outcome == "success")
                {
                    self.consumption_error = Some(error.clone());
                }
                connection.observe_error(&error);
                Err(error)
            }
        }
    }

    pub fn fetch(&mut self, limit: Option<usize>) -> Result<Vec<Values>> {
        self.check("fetch")?;
        let connection = self.connection()?;
        connection.check("fetch")?;
        if self.kind == ResultKind::Statement && self.bound {
            return Ok(vec![]);
        }
        let result = match &mut self.resource {
            Resource::Active(active) => active.with_fetcher_mut(|f| {
                f.as_mut()
                    .ok_or_else(|| invalid("fetch", "No cursor"))?
                    .fetch(limit)
            }),
            Resource::Ready(_) if self.bound => {
                Ok(self.columns.iter().map(|c| Values::empty(c.kind)).collect())
            }
            _ => {
                return Err(invalid(
                    "fetch",
                    "Result is not fetchable; execute it first or create a new result",
                ))
            }
        };
        if let Err(error) = &result {
            connection.observe_error(error);
            self.consumption_error = Some(error.clone());
            // Preserve original error if native cleanup also fails.
            let resource = std::mem::replace(&mut self.resource, Resource::Failed);
            if release(resource, "fetch_cleanup").is_err() {
                connection.status.set(ConnectionStatus::Uncertain);
            }
        }
        result
    }

    pub fn complete(&self) -> bool {
        if self.kind == ResultKind::Statement && self.has_resource() && self.bound {
            return true;
        }
        match &self.resource {
            Resource::Active(active) => {
                active.with_fetcher(|f| f.as_ref().is_none_or(Fetcher::complete))
            }
            Resource::Ready(_) => self.bound,
            _ => false,
        }
    }

    /// Discover an initially empty result without counting prefetched rows as
    /// delivered. Later calls normally only inspect the retained buffer/EOF state.
    pub fn has_completed(&mut self) -> Result<bool> {
        self.check("completion")?;
        if self.kind == ResultKind::Statement {
            return Ok(self.bound);
        }
        let outcome = match &mut self.resource {
            Resource::Active(active) => {
                active.with_fetcher_mut(|f| f.as_mut().map_or(Ok(true), Fetcher::has_completed))
            }
            _ => Ok(self.complete()),
        };
        if let Err(error) = &outcome {
            if let Ok(connection) = self.connection() {
                connection.observe_error(error);
            }
            self.consumption_error = Some(error.clone());
            let resource = std::mem::replace(&mut self.resource, Resource::Failed);
            if release(resource, "completion_cleanup").is_err() {
                if let Ok(connection) = self.connection() {
                    connection.status.set(ConnectionStatus::Uncertain);
                }
            }
        }
        outcome
    }

    pub fn clear(&mut self) -> Result<()> {
        if self.pid != std::process::id() {
            return Err(invalid("clear", "Result belongs to another process"));
        }
        let resource = std::mem::replace(&mut self.resource, Resource::Cleared);
        let mut result = match resource {
            Resource::Active(mut active) => {
                let close = active.with_fetcher_mut(|f| f.take().map_or(Ok(()), Fetcher::close));
                let cleanup = release(active, "clear");
                close.and(cleanup)
            }
            resource => release(resource, "clear"),
        };
        if let Err(error) = &mut result {
            error.uncertain = true;
            if let Ok(connection) = self.connection() {
                connection.status.set(ConnectionStatus::Uncertain);
                connection.observe_error(error);
            }
        }
        // Drop outside field destruction, so any native finalizer panic is caught.
        let cleanup = release(self.lease.take(), "clear_connection_reference");
        result.and(cleanup)
    }
}

fn unsupported(error: &odbc_api::Error) -> bool {
    matches!(error, odbc_api::Error::Diagnostics { record, .. }
        if matches!(record.state.as_str(), "HYC00" | "HY092" | "HY096" | "IM001" | "S1C00" | "S1092"))
}

fn drain_affected(stmt: &mut impl Statement) -> Result<Option<usize>> {
    let mut total = Some(0usize);
    loop {
        let n = stmt
            .row_count()
            .into_result_without_logging(stmt)
            .map_err(|e| DriverError::odbc("rows_affected", e))?;
        total = total.and_then(|total| usize::try_from(n).ok().and_then(|n| total.checked_add(n)));
        // SAFETY: statement is executed and all parameter/status buffers remain
        // alive in StatementOwner. SQLMoreResults discards statement-returned rows.
        if !unsafe { stmt.more_results() }
            .into_result_bool(stmt)
            .map_err(|e| DriverError::odbc("batch_results", e))?
        {
            return Ok(total);
        }
    }
}

pub fn release<T>(value: T, operation: &'static str) -> Result<()> {
    catch_unwind(AssertUnwindSafe(|| drop(value))).map_err(|_| {
        let mut error = DriverError::new(
            operation,
            "cleanup",
            "Native cleanup panicked; connection usability is uncertain",
        );
        error.uncertain = true;
        error
    })
}

impl Drop for ResultState {
    fn drop(&mut self) {
        let resource = std::mem::replace(&mut self.resource, Resource::Cleared);
        if self.pid != std::process::id() {
            std::mem::forget(resource);
        } else if release(resource, "finalize_result").is_err() {
            if let Some(connection) = self.connection.upgrade() {
                connection.status.set(ConnectionStatus::Uncertain);
            }
        }
    }
}
impl Drop for ConnectionState {
    fn drop(&mut self) {
        let native = self.native.get_mut().take();
        if self.pid != std::process::id() {
            std::mem::forget(native);
        } else {
            let _ = release(native, "finalize_connection");
        }
    }
}
