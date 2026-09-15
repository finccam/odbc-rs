//! Owned column-wise ODBC parameters. Validation/conversion precedes execution.
use crate::{
    error::{DriverError, Result},
    types::{self, Options},
};
use extendr_api::prelude::*;
use odbc_api::{
    buffers::{BinColumn, TextColumn},
    handles::{CData, HasDataType, SqlResult, Statement},
    parameter::WithDataType,
    sys, Bit, DataType, ParameterCollectionRef, Pod,
};
use std::{ffi::c_void, num::NonZeroUsize};

trait ArrayColumn: CData + HasDataType + Send {}
impl<T: CData + HasDataType + Send> ArrayColumn for T {}

pub struct Parameters {
    columns: Vec<Box<dyn ArrayColumn>>,
    pub rows: usize,
    statuses: Vec<u16>,
    processed: Box<usize>,
}

impl Default for Parameters {
    fn default() -> Self {
        Self {
            columns: vec![],
            rows: 1,
            statuses: vec![u16::MAX],
            processed: Box::new(usize::MAX),
        }
    }
}

impl Parameters {
    pub fn len(&self) -> usize {
        self.columns.len()
    }
    pub fn from_r(input: &Robj, expected: Option<usize>, options: &Options) -> Result<Self> {
        let list = input
            .as_list()
            .ok_or_else(|| parameter("Parameters must be a list or data frame"))?;
        if expected.is_some_and(|n| n != list.len()) {
            return Err(parameter(
                "Parameter count does not match the prepared statement",
            ));
        }
        if list.len() > u16::MAX as usize {
            return Err(parameter("Too many parameter columns"));
        }
        let rows = list.values().next().map_or(1, |x| x.len());
        if list.values().any(|x| x.len() != rows) {
            return Err(parameter("Parameter columns must have equal lengths"));
        }
        let columns = list
            .values()
            .map(|column| convert(&column, options))
            .collect::<Result<_>>()?;
        Ok(Self {
            columns,
            rows,
            statuses: vec![u16::MAX; rows],
            processed: Box::new(usize::MAX),
        })
    }

    pub fn annotate_failure(&self, error: &mut DriverError) {
        if self.rows <= 1 {
            return;
        }
        error.batch_size = Some(self.rows);
        error.batch_processed = (*self.processed <= self.rows).then_some(*self.processed);
        if self.statuses.iter().any(|s| matches!(s, 0 | 1 | 5 | 6 | 7)) {
            error.batch_succeeded =
                Some(self.statuses.iter().filter(|s| matches!(s, 0 | 6)).count());
        }
        error.batch_outcome_uncertain = true;
    }

    pub fn failure(&self) -> Option<DriverError> {
        if self.rows <= 1 || !self.statuses.iter().any(|s| matches!(s, 1 | 5)) {
            return None;
        }
        let mut error = DriverError::new(
            "execute",
            "batch",
            "ODBC reported one or more failed parameter rows; some rows may have succeeded",
        );
        self.annotate_failure(&mut error);
        Some(error)
    }
}

pub fn reset_bindings(stmt: &mut impl Statement) -> std::result::Result<(), odbc_api::Error> {
    set_attr(
        stmt,
        sys::StatementAttribute::ParamStatusPtr,
        std::ptr::null_mut(),
    )?;
    set_attr(
        stmt,
        sys::StatementAttribute::ParamsProcessedPtr,
        std::ptr::null_mut(),
    )?;
    stmt.reset_parameters().into_result_without_logging(stmt)
}

fn set_attr(
    stmt: &mut impl Statement,
    attr: sys::StatementAttribute,
    ptr: *mut c_void,
) -> std::result::Result<(), odbc_api::Error> {
    // Valid statement; pointer attributes are either null or owned output arrays
    // retained in StatementOwner until reset_bindings or statement destruction.
    match unsafe { sys::SQLSetStmtAttr(stmt.as_sys(), attr, ptr, 0) } {
        sys::SqlReturn::SUCCESS | sys::SqlReturn::SUCCESS_WITH_INFO => Ok(()),
        _ => SqlResult::Error {
            function: "SQLSetStmtAttr",
        }
        .into_result_without_logging(stmt),
    }
}

// SAFETY: every column has exactly rows initialized values and indicators. Text
// and binary buffers are sized from all input values and never truncated. The
// enclosing StatementOwner keeps inputs/output-status storage alive until its
// cursor is closed and bindings reset; R memory is never bound to ODBC.
unsafe impl ParameterCollectionRef for &mut Parameters {
    fn parameter_set_size(&self) -> usize {
        self.rows
    }
    unsafe fn bind_parameters_to(
        &mut self,
        stmt: &mut impl Statement,
    ) -> std::result::Result<(), odbc_api::Error> {
        set_attr(
            stmt,
            sys::StatementAttribute::ParamBindType,
            std::ptr::null_mut(),
        )?;
        set_attr(
            stmt,
            sys::StatementAttribute::ParamStatusPtr,
            self.statuses.as_mut_ptr().cast(),
        )?;
        set_attr(
            stmt,
            sys::StatementAttribute::ParamsProcessedPtr,
            (&mut *self.processed as *mut usize).cast(),
        )?;
        for (i, column) in self.columns.iter().enumerate() {
            unsafe { stmt.bind_input_parameter((i + 1) as u16, column.as_ref()) }
                .into_result_without_logging(stmt)?;
        }
        Ok(())
    }
}

struct Fixed<T> {
    values: Vec<T>,
    indicators: Vec<isize>,
}
// SAFETY: Pod defines the ODBC layout/stride; both vectors have equal length and
// remain stable while bound. SQL_PARAM_INPUT prevents writes to their contents.
unsafe impl<T: Pod> CData for Fixed<T> {
    fn cdata_type(&self) -> sys::CDataType {
        T::C_DATA_TYPE
    }
    fn value_ptr(&self) -> *const c_void {
        self.values.as_ptr().cast()
    }
    fn indicator_ptr(&self) -> *const isize {
        self.indicators.as_ptr()
    }
    fn buffer_length(&self) -> isize {
        std::mem::size_of::<T>() as isize
    }
}

fn fixed<T: Pod>(values: Vec<Option<T>>, data_type: DataType) -> Box<dyn ArrayColumn> {
    let indicators = values
        .iter()
        .map(|x| if x.is_some() { 0 } else { sys::NULL_DATA })
        .collect();
    Box::new(WithDataType {
        value: Fixed {
            values: values.into_iter().map(Option::unwrap_or_default).collect(),
            indicators,
        },
        data_type,
    })
}

fn text(values: Vec<Option<String>>, data_type: Option<DataType>) -> Result<Box<dyn ArrayColumn>> {
    let values: Vec<Option<Vec<u16>>> = values
        .into_iter()
        .map(|s| s.map(|s| s.encode_utf16().collect()))
        .collect();
    let width = values
        .iter()
        .flatten()
        .map(Vec::len)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut buffer = TextColumn::<u16>::try_new(values.len(), width)
        .map_err(|_| parameter("Parameter text allocation is too large"))?;
    for (i, value) in values.iter().enumerate() {
        buffer.set_value(i, value.as_deref());
    }
    let data_type =
        data_type.unwrap_or_else(|| odbc_api::BindParamDesc::wide_text(width).data_type);
    Ok(Box::new(WithDataType {
        value: buffer,
        data_type,
    }))
}

fn numbers(column: &Robj) -> Result<Vec<Option<f64>>> {
    if let Some(v) = column.as_real_slice() {
        return Ok(v
            .iter()
            .map(|&x| if x.is_na() { None } else { Some(x) })
            .collect());
    }
    if let Some(v) = column.as_integer_slice() {
        return Ok(v
            .iter()
            .map(|&x| if x.is_na() { None } else { Some(x as f64) })
            .collect());
    }
    Err(parameter("Expected numeric parameter storage"))
}

fn convert(column: &Robj, options: &Options) -> Result<Box<dyn ArrayColumn>> {
    if column.inherits("integer64") {
        let v = column
            .as_real_slice()
            .ok_or_else(|| parameter("Invalid integer64 storage"))?;
        return Ok(fixed(
            v.iter()
                .map(|x| {
                    let n = x.to_bits() as i64;
                    (n != i64::MIN).then_some(n)
                })
                .collect(),
            DataType::BigInt,
        ));
    }
    if column.inherits("Date") {
        return Ok(fixed(
            numbers(column)?
                .into_iter()
                .map(|x| x.map(types::days_to_date).transpose())
                .collect::<Result<_>>()?,
            DataType::Date,
        ));
    }
    if column.inherits("POSIXct") {
        let values = numbers(column)?
            .into_iter()
            .map(|x| {
                x.map(|x| {
                    let mut ts = types::seconds_to_timestamp(x, options.timezone)?;
                    ts.fraction -= ts.fraction % 100;
                    Ok(ts)
                })
                .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        return Ok(fixed(values, DataType::Timestamp { precision: 7 }));
    }
    if column.inherits("difftime") {
        let units = column
            .get_attrib("units")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| parameter("difftime requires units"))?;
        let scale = match units.as_str() {
            "secs" => 1.0,
            "mins" => 60.0,
            "hours" => 3600.0,
            "days" => 86400.0,
            "weeks" => 604800.0,
            _ => return Err(parameter("Invalid time units")),
        };
        let values = numbers(column)?
            .into_iter()
            .map(|x| {
                x.map(|x| {
                    let x = x * scale;
                    if !x.is_finite() || !(0.0..86400.0).contains(&x) {
                        return Err(parameter("ODBC time must be within one day"));
                    }
                    let seconds = x.floor() as u32;
                    Ok(format!(
                        "{:02}:{:02}:{:02}.{:07}",
                        seconds / 3600,
                        seconds / 60 % 60,
                        seconds % 60,
                        ((x.fract() * 1e7).floor()) as u32
                    ))
                })
                .transpose()
            })
            .collect::<Result<_>>()?;
        return text(values, Some(DataType::Time { precision: 7 }));
    }
    match column.rtype() {
        Rtype::Logicals => Ok(fixed(
            column
                .as_logical_slice()
                .unwrap()
                .iter()
                .map(|x| {
                    if x.is_na() {
                        None
                    } else {
                        Some(Bit::from_bool(x.is_true()))
                    }
                })
                .collect(),
            DataType::Bit,
        )),
        Rtype::Integers => Ok(fixed(
            column
                .as_integer_slice()
                .unwrap()
                .iter()
                .map(|&x| if x.is_na() { None } else { Some(x) })
                .collect(),
            DataType::Integer,
        )),
        Rtype::Doubles => Ok(fixed(numbers(column)?, DataType::Double)),
        // R may expose a null STRING_PTR for character(0). Avoid extendr's
        // slice-backed iterator on that empty storage (Rust requires non-null
        // slice pointers even at length zero).
        Rtype::Strings if column.len() == 0 => text(vec![], None),
        Rtype::Strings => text(
            Strings::try_from(column)
                .map_err(|_| parameter("Invalid character vector"))?
                .iter()
                .map(|x| if x.is_na() { None } else { Some(x.to_string()) })
                .collect(),
            None,
        ),
        Rtype::List => {
            let values = column
                .as_list()
                .unwrap()
                .values()
                .map(|x| {
                    if x.is_null() {
                        Ok(None)
                    } else {
                        x.as_raw_slice()
                            .map(|x| Some(x.to_vec()))
                            .ok_or_else(|| parameter("Binary columns require raw vectors or NULL"))
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            let width = values
                .iter()
                .flatten()
                .map(Vec::len)
                .max()
                .unwrap_or(1)
                .max(1);
            let mut buffer = BinColumn::try_new(values.len(), width)
                .map_err(|_| parameter("Parameter binary allocation is too large"))?;
            for (i, value) in values.iter().enumerate() {
                buffer.set_value(i, value.as_deref());
            }
            let data_type = if width <= 8000 {
                DataType::Varbinary {
                    length: NonZeroUsize::new(width),
                }
            } else {
                DataType::LongVarbinary {
                    length: NonZeroUsize::new(width),
                }
            };
            Ok(Box::new(WithDataType {
                value: buffer,
                data_type,
            }))
        }
        _ => Err(parameter("Unsupported parameter column type")),
    }
}

fn parameter(message: impl Into<String>) -> DriverError {
    DriverError::new("bind", "parameter", message)
}
