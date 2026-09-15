//! Prepared and direct statements share the same borrowed cursor and buffer path.
use odbc_api::{
    handles::{AsStatementRef, Statement, StatementConnection, StatementRef},
    Connection, CursorImpl, Preallocated, Prepared, ResultSetMetadata,
};
use std::sync::Arc;

type OwnedHandle = StatementConnection<Arc<Connection<'static>>>;
pub use crate::parameters::Parameters;

pub enum NativeStatement {
    Prepared(Prepared<OwnedHandle>),
    Direct(Preallocated<OwnedHandle>),
}

impl NativeStatement {
    pub fn execute(
        &mut self,
        sql: &str,
        parameters: &mut Parameters,
    ) -> Result<Option<CursorImpl<StatementRef<'_>>>, odbc_api::Error> {
        match self {
            Self::Prepared(statement) => statement.execute(parameters),
            Self::Direct(statement) => statement.execute(sql, parameters),
        }
    }

    pub fn row_count(&mut self) -> Result<Option<usize>, odbc_api::Error> {
        let mut statement = self.as_stmt_ref();
        let count = statement
            .row_count()
            .into_result_without_logging(&statement)?;
        Ok(usize::try_from(count).ok())
    }

    pub fn reset_bindings(&mut self) -> Result<(), odbc_api::Error> {
        crate::parameters::reset_bindings(&mut self.as_stmt_ref())
    }
}

impl AsStatementRef for NativeStatement {
    fn as_stmt_ref(&mut self) -> StatementRef<'_> {
        match self {
            Self::Prepared(statement) => statement.as_stmt_ref(),
            Self::Direct(statement) => statement.as_stmt_ref(),
        }
    }
}

impl ResultSetMetadata for NativeStatement {}

pub struct StatementOwner {
    // Statement is dropped before its bound values.
    pub statement: NativeStatement,
    pub parameters: Parameters,
}
