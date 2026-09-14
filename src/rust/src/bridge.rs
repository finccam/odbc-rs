//! Private extendr boundary. Native failures return envelopes; R raises conditions
//! only after Rust has released operation borrows and temporary resources.
use crate::{
    error::{DriverError, Result as NativeResult},
    parameters::ParameterBatch,
    state::{ConnectionState, ConnectionStatus, ResultState},
    types::{BigInt, Column, Kind, Options, Values},
};
use extendr_api::prelude::*;
use std::{
    cell::RefCell,
    panic::{catch_unwind, AssertUnwindSafe},
    rc::Rc,
};

pub struct ConnectionHandle(pub Rc<ConnectionState>);
pub struct ResultHandle(pub Rc<RefCell<ResultState>>);

struct PoisonOnUnwind(Rc<ConnectionState>);
impl Drop for PoisonOnUnwind {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.status.set(ConnectionStatus::Uncertain);
        }
    }
}

fn boundary(f: impl FnOnce() -> NativeResult<Robj>) -> List {
    let result = catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| {
        let mut error = DriverError::new(
            "native",
            "panic",
            "Rust operation panicked; dispose of the affected connection",
        );
        error.uncertain = true;
        Err(error)
    });
    match result {
        Ok(value) => list!(ok = true, value = value),
        Err(error) => list!(
            ok = false,
            error = list!(
                message = error.to_string(),
                operation = error.operation,
                kind = error.kind,
                sqlstate = error.sqlstate,
                native_code = error.native_code,
                uncertain = error.uncertain
            )
        ),
    }
}

fn connection(ptr: Robj) -> NativeResult<Rc<ConnectionState>> {
    let ptr: ExternalPtr<ConnectionHandle> = ptr.try_into().map_err(|_| invalid_pointer())?;
    Ok(ptr.try_addr().map_err(|_| invalid_pointer())?.0.clone())
}
fn result(ptr: Robj) -> NativeResult<Rc<RefCell<ResultState>>> {
    let ptr: ExternalPtr<ResultHandle> = ptr.try_into().map_err(|_| invalid_pointer())?;
    Ok(ptr.try_addr().map_err(|_| invalid_pointer())?.0.clone())
}
fn invalid_pointer() -> DriverError {
    DriverError::new("native", "state", "Invalid or stale native pointer")
}
fn busy() -> DriverError {
    DriverError::new("native", "state", "Result is already in use")
}
fn r_error(error: extendr_api::Error) -> DriverError {
    DriverError::new("conversion", "conversion", error.to_string())
}

fn option(input: &List, name: &str) -> Option<Robj> {
    input
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value)
}
fn string_option(input: &List, name: &str, default: &str) -> NativeResult<String> {
    option(input, name).map_or_else(
        || Ok(default.into()),
        |value| {
            value
                .as_str()
                .filter(|s| !s.contains('\0'))
                .map(str::to_owned)
                .ok_or_else(|| {
                    DriverError::new(
                        "options",
                        "parameter",
                        format!("{name} must be one nonmissing string"),
                    )
                })
        },
    )
}

/// Private connection primitive; does not construct a public DBI object.
#[extendr]
fn native_connect(connection_string: String, config: List) -> List {
    boundary(|| {
        if connection_string.contains('\0') {
            return Err(DriverError::new(
                "connect",
                "parameter",
                "Connection string contains NUL",
            ));
        }
        let mut options = Options::default();
        options.bigint = match string_option(&config, "bigint", "integer64")?.as_str() {
            "integer64" => BigInt::Integer64,
            "integer" => BigInt::Integer,
            "numeric" => BigInt::Numeric,
            "character" => BigInt::Character,
            _ => {
                return Err(DriverError::new(
                    "connect",
                    "parameter",
                    "Invalid bigint mapping",
                ))
            }
        };
        options.timezone = string_option(&config, "timezone", "UTC")?
            .parse()
            .map_err(|_| DriverError::new("connect", "parameter", "Unknown server timezone"))?;
        options.timezone_out = string_option(&config, "timezone_out", "UTC")?;
        options
            .timezone_out
            .parse::<chrono_tz::Tz>()
            .map_err(|_| DriverError::new("connect", "parameter", "Unknown output timezone"))?;
        // Reject rather than silently ignore options not implemented in this layer.
        for (name, _) in config.iter() {
            if !["bigint", "timezone", "timezone_out", "timeout"].contains(&name) {
                return Err(DriverError::new(
                    "connect",
                    "parameter",
                    format!("Unsupported native option: {name}"),
                ));
            }
        }
        let timeout = option(&config, "timeout")
            .map(|v| v.as_real().or_else(|| v.as_integer().map(f64::from)))
            .unwrap_or(Some(10.0))
            .ok_or_else(|| DriverError::new("connect", "parameter", "Invalid login timeout"))?;
        if !timeout.is_finite()
            || timeout < 0.0
            || timeout.fract() != 0.0
            || timeout > u32::MAX as f64
        {
            return Err(DriverError::new(
                "connect",
                "parameter",
                "Login timeout must be nonnegative whole seconds",
            ));
        }
        let conn = ConnectionState::connect(&connection_string, options, timeout as u32)?;
        Ok(ExternalPtr::new(ConnectionHandle(conn)).into())
    })
}

#[extendr]
fn native_prepare(ptr: Robj, sql: String) -> List {
    boundary(|| {
        let conn = connection(ptr)?;
        let _guard = PoisonOnUnwind(conn.clone());
        if sql.contains('\0') {
            return Err(DriverError::new("prepare", "parameter", "SQL contains NUL"));
        }
        Ok(ExternalPtr::new(ResultHandle(conn.prepare(sql)?)).into())
    })
}

#[extendr]
fn native_bind_scalar(ptr: Robj, parameters: Robj) -> List {
    boundary(|| {
        let result = result(ptr)?;
        let (conn, count) = {
            let result = result.try_borrow().map_err(|_| busy())?;
            (result.connection()?, result.parameter_count)
        };
        let _guard = PoisonOnUnwind(conn.clone());
        conn.check("bind")?;
        let mut batch = ParameterBatch::from_r(&parameters, count, &conn.options)?;
        if batch.rows.len() != 1 {
            return Err(DriverError::new("bind", "parameter", "This private scalar primitive requires exactly one parameter row; batch execution is a separate operation"));
        }
        debug_assert_eq!(batch.column_count, count);
        result
            .try_borrow_mut()
            .map_err(|_| busy())?
            .execute(batch.rows.pop().unwrap())?;
        Ok(().into())
    })
}

#[extendr]
fn native_fetch(ptr: Robj, n: f64) -> List {
    boundary(|| {
        let limit = if n == -1.0 || n == f64::INFINITY {
            None
        } else if n.is_finite() && n >= 0.0 && n.fract() == 0.0 && n <= i32::MAX as f64 {
            Some(n as usize)
        } else {
            return Err(DriverError::new(
                "fetch",
                "parameter",
                "n must be -1, Inf, or a nonnegative whole number within R's data-frame row limit",
            ));
        };
        let result = result(ptr)?;
        let conn = result.try_borrow().map_err(|_| busy())?.connection()?;
        let _guard = PoisonOnUnwind(conn.clone());
        let (values, columns) = {
            let mut state = result.try_borrow_mut().map_err(|_| busy())?;
            let values = state.fetch(limit)?;
            (values, state.columns.clone())
        };
        let rows = values.first().map_or(0, Values::len);
        // R allocation happens after releasing the native operation borrow.
        match dataframe(values, &columns, &conn.options) {
            Ok(frame) => {
                let mut state = result.try_borrow_mut().map_err(|_| busy())?;
                state.delivered = state.delivered.checked_add(rows as u64).ok_or_else(|| {
                    DriverError::new("fetch", "conversion", "Delivered-row count overflow")
                })?;
                Ok(frame)
            }
            Err(error) => {
                let mut state = result.try_borrow_mut().map_err(|_| busy())?;
                state.consumption_error = Some(error.clone());
                let _ = state.clear();
                Err(error)
            }
        }
    })
}

#[extendr]
fn native_clear(ptr: Robj) -> List {
    boundary(|| {
        let result = result(ptr)?;
        let conn = result.try_borrow().map_err(|_| busy())?.connection().ok();
        let _guard = conn.map(PoisonOnUnwind);
        result.try_borrow_mut().map_err(|_| busy())?.clear()?;
        Ok(().into())
    })
}

#[extendr]
fn native_disconnect(ptr: Robj) -> List {
    boundary(|| {
        let conn = connection(ptr)?;
        let _guard = PoisonOnUnwind(conn.clone());
        conn.close()?;
        Ok(().into())
    })
}

#[extendr]
fn native_connection_info(ptr: Robj) -> List {
    boundary(|| {
        let conn = connection(ptr)?;
        Ok(list!(
            id = conn.id.to_string(),
            valid = conn.check("validity").is_ok(),
            status = format!("{:?}", conn.status.get())
        )
        .into())
    })
}

#[extendr]
fn native_result_info(ptr: Robj) -> List {
    boundary(|| {
        let result = result(ptr)?;
        let state = result.try_borrow().map_err(|_| busy())?;
        let attempt: Robj = state
            .native_attempt
            .as_ref()
            .map(|e| {
                list!(
                    id = e.id.to_string(),
                    outcome = e.outcome,
                    duration_seconds = e.duration_seconds
                )
                .into()
            })
            .unwrap_or_else(|| ().into());
        // This is private R-facing metadata, not an observer payload. SQL and raw
        // diagnostics must never be forwarded wholesale to instrumentation.
        Ok(list!(
            id = state.id.to_string(),
            connection_id = state.connection_id.to_string(),
            valid = state.valid(),
            completed = state.complete(),
            rows_delivered = state.delivered as f64,
            rows_affected = state.affected.map(|n| n as f64),
            statement = state.sql.clone(),
            native_attempt = attempt,
            consumption_failed = state.consumption_error.is_some(),
            columns = column_info(&state.columns)
        )
        .into())
    })
}

fn column_info(columns: &[Column]) -> List {
    List::from_values(columns.iter().map(|c| {
        list!(
            name = c.name.clone(),
            r_type = format!("{:?}", c.kind),
            native_type = format!("{:?}", c.native_type),
            nullable = c.nullable
        )
    }))
}

fn dataframe(values: Vec<Values>, columns: &[Column], options: &Options) -> NativeResult<Robj> {
    let rows = values.first().map_or(0, Values::len);
    let row_count: i32 = rows.try_into().map_err(|_| {
        DriverError::new(
            "fetch",
            "conversion",
            "Data frame exceeds supported row count",
        )
    })?;
    let mut output = List::new(values.len());
    for (index, (values, column)) in values.into_iter().zip(columns).enumerate() {
        let mut value: Robj = match values {
            Values::Logical(v) => {
                Logicals::from_values(v.into_iter().map(|v| v.map_or_else(Rbool::na, Rbool::from)))
                    .into()
            }
            Values::Integer(v) => {
                Integers::from_values(v.into_iter().map(|v| v.map_or_else(Rint::na, Rint::from)))
                    .into()
            }
            Values::Integer64(v) => {
                let bits = Doubles::from_values(
                    v.into_iter()
                        .map(|v| f64::from_bits(v.unwrap_or(i64::MIN) as u64)),
                );
                let mut bits: Robj = bits.into();
                bits.set_class(["integer64"]).map_err(r_error)?;
                bits
            }
            Values::Double(v) => Doubles::from_values(
                v.into_iter()
                    .map(|v| v.map_or_else(Rfloat::na, Rfloat::from)),
            )
            .into(),
            Values::Text(v) => Strings::from_values(v.into_iter().map(Rstr::from)).into(),
            Values::Binary(v) => {
                let mut blobs: Robj = List::from_values(v.into_iter().map(|v| {
                    v.map(|bytes| Robj::from(Raw::from_bytes(&bytes)))
                        .unwrap_or_else(|| ().into())
                }))
                .into();
                blobs
                    .set_class(["blob", "vctrs_list_of", "vctrs_vctr", "list"])
                    .map_err(r_error)?;
                blobs.set_attrib("ptype", Raw::new(0)).map_err(r_error)?;
                blobs
            }
        };
        match column.kind {
            Kind::Date => {
                value.set_class(["Date"]).map_err(r_error)?;
            }
            Kind::Timestamp | Kind::TimestampOffset => {
                value.set_class(["POSIXct", "POSIXt"]).map_err(r_error)?;
                value
                    .set_attrib("tzone", options.timezone_out.as_str())
                    .map_err(r_error)?;
            }
            Kind::Time => {
                value.set_class(["hms", "difftime"]).map_err(r_error)?;
                value.set_attrib("units", "secs").map_err(r_error)?;
            }
            _ => {}
        }
        output.set_elt(index, value).map_err(r_error)?;
    }
    let mut output: Robj = output.into();
    output
        .set_attrib(
            "names",
            columns.iter().map(|c| c.name.as_str()).collect::<Strings>(),
        )
        .map_err(r_error)?;
    output.set_class(["data.frame"]).map_err(r_error)?;
    let row_names = if rows == 0 {
        Integers::new(0)
    } else {
        Integers::from_values([Rint::na(), Rint::from(-row_count)])
    };
    output.set_attrib("row.names", row_names).map_err(r_error)?;
    Ok(output)
}

extendr_module! {
    mod bridge;
    fn native_connect;
    fn native_prepare;
    fn native_bind_scalar;
    fn native_fetch;
    fn native_clear;
    fn native_disconnect;
    fn native_connection_info;
    fn native_result_info;
}
