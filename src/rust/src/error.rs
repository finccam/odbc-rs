//! Native diagnostics stay separate from any future observer payload.
use std::fmt;

pub type Result<T> = std::result::Result<T, DriverError>;

#[derive(Debug, Clone)]
pub struct DriverError {
    pub operation: &'static str,
    pub kind: &'static str,
    pub message: String,
    pub sqlstate: Option<String>,
    pub native_code: Option<i32>,
    pub uncertain: bool,
    pub batch_size: Option<usize>,
    pub batch_processed: Option<usize>,
    pub batch_succeeded: Option<usize>,
    pub batch_outcome_uncertain: bool,
}

impl DriverError {
    pub fn new(operation: &'static str, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            operation,
            kind,
            message: message.into(),
            sqlstate: None,
            native_code: None,
            uncertain: false,
            batch_size: None,
            batch_processed: None,
            batch_succeeded: None,
            batch_outcome_uncertain: false,
        }
    }

    pub fn odbc(operation: &'static str, error: odbc_api::Error) -> Self {
        use odbc_api::Error::*;
        let record = match &error {
            Diagnostics { record, .. } | InvalidRowArraySize { record, .. } => Some(record),
            UnsupportedOdbcApiVersion(record)
            | UnableToRepresentNull(record)
            | OracleOdbcDriverDoesNotSupport64Bit(record) => Some(record),
            _ => None,
        };
        let sqlstate = record.map(|r| r.state.as_str().to_owned());
        let uncertain = sqlstate.as_deref().is_some_and(|s| {
            s.starts_with("08") || matches!(s, "40003" | "HY117" | "HYT00" | "HYT01" | "HY008")
        });
        Self {
            operation,
            kind: match sqlstate.as_deref() {
                Some("HYT00" | "HYT01") => "timeout",
                Some("HY008") => "cancelled",
                _ => "database",
            },
            message: error.to_string(),
            sqlstate,
            native_code: record.map(|r| r.native_error),
            uncertain,
            batch_size: None,
            batch_processed: None,
            batch_succeeded: None,
            batch_outcome_uncertain: false,
        }
    }
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.operation, self.message)
    }
}
impl std::error::Error for DriverError {}
