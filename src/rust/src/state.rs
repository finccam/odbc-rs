//! R-thread-only ownership. Arc is used for the crate's owned statement API,
//! not to imply concurrent access to R or to the same statement.
use crate::{
    error::{DriverError, Result},
    fetch::Fetcher,
    types::{Column, Options, Values},
};
use odbc_api::{
    handles::StatementConnection, parameter::InputParameter, Connection, ConnectionOptions,
    ConnectionTransitions, Prepared,
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
pub type OwnedPrepared = Prepared<StatementConnection<Arc<Connection<'static>>>>;
pub type Parameters = Vec<Box<dyn InputParameter>>;

pub struct PreparedOwner {
    pub statement: OwnedPrepared,
    // Bound values outlive the cursor and remain available until the next bind.
    pub parameters: Parameters,
}

#[self_referencing]
pub struct ActiveResult {
    owner: PreparedOwner,
    #[borrows(mut owner)]
    #[not_covariant]
    fetcher: Option<Fetcher<'this>>,
}

enum Resource {
    Prepared(PreparedOwner),
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
    pub parameter_count: usize,
    resource: Resource,
    pub columns: Vec<Column>,
    pub delivered: u64,
    pub affected: Option<usize>,
    pub native_attempt: Option<NativeAttempt>,
    pub consumption_error: Option<DriverError>,
}

#[derive(Clone, Debug)]
// Prepared::execute also binds parameters and probes result-column count. These
// are private native-call diagnostics, not the future execution observer events.
pub struct NativeAttempt {
    pub id: u64,
    pub outcome: &'static str,
    pub duration_seconds: f64,
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

    pub fn prepare(self: &Rc<Self>, sql: String) -> Result<Rc<RefCell<ResultState>>> {
        self.check("prepare")?;
        let connection = self
            .native
            .try_borrow()
            .map_err(|_| invalid("prepare", "Connection is already borrowed"))?
            .as_ref()
            .cloned()
            .ok_or_else(|| invalid("prepare", "Connection is closed"))?;
        let mut statement = connection.into_prepared(&sql).map_err(|e| {
            let error = DriverError::odbc("prepare", e);
            self.observe_error(&error);
            error
        })?;
        let parameter_count = statement.num_params().map_err(|e| {
            let error = DriverError::odbc("prepare", e);
            self.observe_error(&error);
            error
        })? as usize;
        let result = Rc::new(RefCell::new(ResultState {
            id: identity(),
            connection_id: self.id,
            pid: self.pid,
            connection: Rc::downgrade(self),
            lease: Some(self.clone()),
            sql,
            parameter_count,
            resource: Resource::Prepared(PreparedOwner {
                statement,
                parameters: vec![],
            }),
            columns: vec![],
            delivered: 0,
            affected: None,
            native_attempt: None,
            consumption_error: None,
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

    fn recover_prepared(&mut self) -> Result<PreparedOwner> {
        match std::mem::replace(&mut self.resource, Resource::Failed) {
            Resource::Prepared(owner) => Ok(owner),
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

    /// Scalar execution primitive. Vector/batch scheduling belongs to the DBI
    /// operation layer, not an implicit per-row loop here.
    pub fn execute(&mut self, parameters: Parameters) -> Result<()> {
        let connection = self.connection()?;
        connection.check("execute")?;
        if parameters.len() != self.parameter_count {
            return Err(DriverError::new(
                "bind",
                "parameter",
                format!(
                    "Expected {} parameters; received {}",
                    self.parameter_count,
                    parameters.len()
                ),
            ));
        }
        let mut owner = self.recover_prepared()?;
        owner.parameters = parameters;
        self.delivered = 0;
        self.affected = None;
        self.columns.clear();
        self.consumption_error = None;
        self.native_attempt = None;
        let options = connection.options.clone();
        let attempt_id = identity();
        let attempt = RefCell::new(None);
        let active = ActiveResultTryBuilder {
            owner,
            fetcher_builder: |owner: &mut PreparedOwner| {
                let start = Instant::now();
                let result = owner.statement.execute(owner.parameters.as_slice());
                let duration_seconds = start.elapsed().as_secs_f64();
                let outcome = match &result {
                    Ok(_) => "success",
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
                });
                result
                    .map_err(|e| DriverError::odbc("execute", e))?
                    .map(|cursor| Fetcher::new(cursor, options))
                    .transpose()
            },
        }
        .try_build();
        self.native_attempt = attempt.into_inner();
        match active {
            Ok(active) => {
                self.columns = active
                    .with_fetcher(|f| f.as_ref().map_or_else(Vec::new, |f| f.columns.clone()));
                if active.with_fetcher(|f| f.is_none()) {
                    let mut owner = active.into_heads().owner;
                    let affected = owner
                        .statement
                        .row_count()
                        .map_err(|e| DriverError::odbc("rows_affected", e));
                    self.resource = Resource::Prepared(owner);
                    match affected {
                        Ok(n) => self.affected = n,
                        Err(error) => {
                            connection.observe_error(&error);
                            self.consumption_error = Some(error.clone());
                            return Err(error);
                        }
                    }
                } else {
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
        let connection = self.connection()?;
        connection.check("fetch")?;
        let result = match &mut self.resource {
            Resource::Active(active) => active.with_fetcher_mut(|f| {
                f.as_mut()
                    .ok_or_else(|| invalid("fetch", "No cursor"))?
                    .fetch(limit)
            }),
            Resource::Prepared(_) if self.native_attempt.is_some() => Ok(vec![]),
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
        match &self.resource {
            Resource::Active(active) => {
                active.with_fetcher(|f| f.as_ref().is_none_or(Fetcher::complete))
            }
            Resource::Prepared(_) => self.native_attempt.is_some(),
            _ => false,
        }
    }

    pub fn clear(&mut self) -> Result<()> {
        if self.pid != std::process::id() {
            return Err(invalid("clear", "Result belongs to another process"));
        }
        let resource = std::mem::replace(&mut self.resource, Resource::Cleared);
        let result = match resource {
            Resource::Active(mut active) => {
                let close = active.with_fetcher_mut(|f| f.take().map_or(Ok(()), Fetcher::close));
                let cleanup = release(active, "clear");
                close.and(cleanup)
            }
            resource => release(resource, "clear"),
        };
        if let Err(error) = &result {
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
