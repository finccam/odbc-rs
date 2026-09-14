//! Copy and validate all input before touching an existing cursor or executing SQL.
use crate::{
    error::{DriverError, Result as NativeResult},
    state::Parameters,
    types::{self, Options},
};
use extendr_api::prelude::*;
use odbc_api::{
    parameter::{InputParameter, VarBinaryBox, VarWCharBox, WithDataType},
    Bit, DataType, Nullable as SqlNullable,
};

pub struct ParameterBatch {
    pub rows: Vec<Parameters>,
    pub column_count: usize,
}

impl ParameterBatch {
    pub fn from_r(input: &Robj, expected: usize, options: &Options) -> NativeResult<Self> {
        let input = input
            .as_list()
            .ok_or_else(|| parameter("Parameters must be a list or data frame"))?;
        if input.len() != expected {
            return Err(parameter(format!(
                "Expected {expected} parameter columns; received {}",
                input.len()
            )));
        }
        let n = input.values().next().map_or(1, |x| x.len());
        if input.values().any(|x| x.len() != n) {
            return Err(parameter("All parameter columns must have the same length"));
        }
        let mut rows: Vec<Parameters> = (0..n).map(|_| Vec::with_capacity(expected)).collect();
        for column in input.values() {
            let converted = convert_column(&column, options)?;
            for (row, value) in rows.iter_mut().zip(converted) {
                row.push(value);
            }
        }
        Ok(Self {
            rows,
            column_count: expected,
        })
    }
}

fn parameter(message: impl Into<String>) -> DriverError {
    DriverError::new("bind", "parameter", message)
}

fn boxed<T: InputParameter + 'static>(value: T) -> Box<dyn InputParameter> {
    Box::new(value)
}

fn numeric_values(column: &Robj) -> NativeResult<Vec<Option<f64>>> {
    if let Some(values) = column.as_real_slice() {
        Ok(values
            .iter()
            .map(|v| if v.is_na() { None } else { Some(*v) })
            .collect())
    } else if let Some(values) = column.as_integer_slice() {
        Ok(values
            .iter()
            .map(|v| if v.is_na() { None } else { Some(*v as f64) })
            .collect())
    } else {
        Err(parameter("Expected a numeric vector"))
    }
}

fn convert_column(column: &Robj, options: &Options) -> NativeResult<Parameters> {
    if column.inherits("integer64") {
        let data = column
            .as_real_slice()
            .ok_or_else(|| parameter("integer64 must use double storage"))?;
        return Ok(data
            .iter()
            .map(|v| {
                // integer64 uses the bits of a REALSXP slot, not its floating value.
                let v = v.to_bits() as i64;
                boxed(if v == i64::MIN {
                    SqlNullable::<i64>::null()
                } else {
                    SqlNullable::new(v)
                })
            })
            .collect());
    }
    if column.inherits("Date") {
        return numeric_values(column)?
            .into_iter()
            .map(|v| {
                let v = v.map(types::days_to_date).transpose()?;
                Ok(boxed(v.map_or_else(SqlNullable::null, SqlNullable::new)))
            })
            .collect();
    }
    if column.inherits("POSIXct") {
        return numeric_values(column)?
            .into_iter()
            .map(|v| {
                let v = v
                    .map(|v| types::seconds_to_timestamp(v, options.timezone))
                    .transpose()?;
                Ok(boxed(WithDataType {
                    value: v.map_or_else(SqlNullable::null, SqlNullable::new),
                    data_type: DataType::Timestamp { precision: 9 },
                }))
            })
            .collect();
    }
    if column.inherits("difftime") {
        let units = column
            .get_attrib("units")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(|| parameter("difftime requires units"))?;
        let multiplier = match units.as_str() {
            "secs" => 1.0,
            "mins" => 60.0,
            "hours" => 3600.0,
            "days" => 86400.0,
            "weeks" => 604800.0,
            _ => return Err(parameter("Unsupported difftime units")),
        };
        return numeric_values(column)?
            .into_iter()
            .map(|v| {
                let text = match v {
                    None => VarWCharBox::null(),
                    Some(v) => {
                        let v = v * multiplier;
                        if !v.is_finite() || !(0.0..86400.0).contains(&v) {
                            return Err(parameter("ODBC time requires a value within one day"));
                        }
                        let hour = (v / 3600.0).floor() as u32;
                        let minute = ((v % 3600.0) / 60.0).floor() as u32;
                        VarWCharBox::from_str_slice(&format!(
                            "{hour:02}:{minute:02}:{:012.9}",
                            v % 60.0
                        ))
                    }
                };
                Ok(boxed(WithDataType {
                    value: text,
                    data_type: DataType::Time { precision: 9 },
                }))
            })
            .collect();
    }
    if column.inherits("factor") {
        let levels = column
            .get_attrib("levels")
            .and_then(|v| Strings::try_from(v).ok())
            .ok_or_else(|| parameter("Invalid factor levels"))?;
        let codes = column
            .as_integer_slice()
            .ok_or_else(|| parameter("Invalid factor storage"))?;
        return codes
            .iter()
            .map(|&code| {
                if code.is_na() {
                    return Ok(boxed(VarWCharBox::null()));
                }
                if code <= 0 || code as usize > levels.len() {
                    return Err(parameter("Invalid factor level index"));
                }
                let value = levels.elt(code as usize - 1);
                Ok(boxed(if value.is_na() {
                    VarWCharBox::null()
                } else {
                    VarWCharBox::from_str_slice(&value)
                }))
            })
            .collect();
    }
    match column.rtype() {
        Rtype::Logicals => Ok(column
            .as_logical_slice()
            .unwrap()
            .iter()
            .map(|v| {
                boxed(if v.is_na() {
                    SqlNullable::<Bit>::null()
                } else {
                    SqlNullable::new(Bit(u8::from(v.is_true())))
                })
            })
            .collect()),
        Rtype::Integers => Ok(column
            .as_integer_slice()
            .unwrap()
            .iter()
            .map(|&v| {
                boxed(if v.is_na() {
                    SqlNullable::<i32>::null()
                } else {
                    SqlNullable::new(v)
                })
            })
            .collect()),
        Rtype::Doubles => Ok(numeric_values(column)?
            .into_iter()
            .map(|v| boxed(v.map_or_else(SqlNullable::<f64>::null, SqlNullable::new)))
            .collect()),
        Rtype::Strings => Strings::try_from(column)
            .map_err(|_| parameter("Invalid text vector"))?
            .iter()
            .map(|v| {
                if v.is_na() {
                    return Ok(boxed(VarWCharBox::null()));
                }
                if v.contains('\0') {
                    return Err(parameter("Embedded NUL in text parameter"));
                }
                Ok(boxed(VarWCharBox::from_str_slice(v)))
            })
            .collect(),
        Rtype::List => column
            .as_list()
            .unwrap()
            .values()
            .map(|v| {
                if v.is_null() {
                    return Ok(boxed(VarBinaryBox::null()));
                }
                let bytes = v
                    .as_raw_slice()
                    .ok_or_else(|| parameter("Binary columns must contain raw vectors or NULL"))?;
                Ok(boxed(VarBinaryBox::from_vec(bytes.to_vec())))
            })
            .collect(),
        _ => Err(parameter("Unsupported parameter column type")),
    }
}
