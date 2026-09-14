//! One conversion plan for both buffered and long-value fetching.
use crate::error::{DriverError, Result};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike};
use chrono_tz::Tz;
use odbc_api::{buffers::BufferDesc, ColumnDescription, DataType, ResultSetMetadata};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Logical,
    Integer,
    Integer64,
    Double,
    Text,
    Binary,
    Date,
    Time,
    Timestamp,
    TimestampOffset,
}

#[derive(Clone, Copy, Debug)]
pub enum BigInt {
    Integer64,
    Integer,
    Numeric,
    Character,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub bigint: BigInt,
    pub timezone: Tz,
    pub timezone_out: String,
    pub buffer_bytes: usize,
    pub max_rows: usize,
    pub max_field_bytes: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bigint: BigInt::Integer64,
            timezone: chrono_tz::UTC,
            timezone_out: "UTC".into(),
            buffer_bytes: 4 * 1024 * 1024,
            max_rows: 1024,
            max_field_bytes: 64 * 1024,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub native_type: DataType,
    pub nullable: Option<bool>,
    pub kind: Kind,
    pub buffer: Option<BufferDesc>,
}

pub fn describe(cursor: &mut impl ResultSetMetadata, options: &Options) -> Result<Vec<Column>> {
    let count = cursor
        .num_result_cols()
        .map_err(|e| DriverError::odbc("metadata", e))?;
    (1..=count as u16)
        .map(|index| {
            let mut desc = ColumnDescription::default();
            cursor
                .describe_col(index, &mut desc)
                .map_err(|e| DriverError::odbc("metadata", e))?;
            let name = desc
                .name_to_string()
                .map_err(|e| DriverError::new("metadata", "encoding", e.to_string()))?;
            let name = if name.is_empty() {
                format!("V{index}")
            } else {
                name
            };
            let dt = desc.data_type;
            let kind = match dt {
                DataType::Bit => Kind::Logical,
                DataType::Integer | DataType::SmallInt | DataType::TinyInt => Kind::Integer,
                DataType::BigInt => match options.bigint {
                    BigInt::Integer64 => Kind::Integer64,
                    BigInt::Integer => Kind::Integer,
                    BigInt::Numeric => Kind::Double,
                    BigInt::Character => Kind::Text,
                },
                DataType::Numeric { .. }
                | DataType::Decimal { .. }
                | DataType::Float { .. }
                | DataType::Real
                | DataType::Double => Kind::Double,
                DataType::Date => Kind::Date,
                DataType::Time { .. } => Kind::Time,
                DataType::Timestamp { .. } => Kind::Timestamp,
                DataType::Binary { .. }
                | DataType::Varbinary { .. }
                | DataType::LongVarbinary { .. } => Kind::Binary,
                DataType::Other { data_type, .. } if data_type.0 == -154 => Kind::Time,
                DataType::Other { data_type, .. } if data_type.0 == -155 => Kind::TimestampOffset,
                _ => Kind::Text,
            };
            // All buffers include NULL indicators even if the driver declares NOT NULL.
            let buffer = match kind {
                Kind::Logical => Some(BufferDesc::Bit { nullable: true }),
                Kind::Integer => Some(BufferDesc::I32 { nullable: true }),
                Kind::Integer64 => Some(BufferDesc::I64 { nullable: true }),
                Kind::Double => Some(BufferDesc::F64 { nullable: true }),
                Kind::Date => Some(BufferDesc::Date { nullable: true }),
                Kind::Timestamp => Some(BufferDesc::Timestamp { nullable: true }),
                // Fetch time as text to preserve fractional seconds, unlike SQL_TIME_STRUCT.
                Kind::Time | Kind::TimestampOffset => None,
                Kind::Text => match dt {
                    DataType::Char { length }
                    | DataType::Varchar { length }
                    | DataType::WChar { length }
                    | DataType::WVarchar { length } => length.and_then(|n| {
                        let units = n.get().checked_mul(2)?;
                        (units.checked_mul(2)? <= options.max_field_bytes)
                            .then_some(BufferDesc::WText { max_str_len: units })
                    }),
                    DataType::BigInt => Some(BufferDesc::WText { max_str_len: 21 }),
                    _ => None,
                },
                Kind::Binary => match dt {
                    DataType::Binary { length } | DataType::Varbinary { length } => length
                        .and_then(|n| {
                            (n.get() <= options.max_field_bytes)
                                .then_some(BufferDesc::Binary { max_bytes: n.get() })
                        }),
                    _ => None,
                },
            };
            let nullable = match desc.nullability {
                odbc_api::Nullability::Nullable => Some(true),
                odbc_api::Nullability::NoNulls => Some(false),
                _ => None,
            };
            Ok(Column {
                name,
                native_type: dt,
                nullable,
                kind,
                buffer,
            })
        })
        .collect()
}

#[derive(Clone, Debug)]
pub enum Values {
    Logical(Vec<Option<bool>>),
    Integer(Vec<Option<i32>>),
    Integer64(Vec<Option<i64>>),
    Double(Vec<Option<f64>>),
    Text(Vec<Option<String>>),
    Binary(Vec<Option<Vec<u8>>>),
}

impl Values {
    pub fn empty(kind: Kind) -> Self {
        match kind {
            Kind::Logical => Self::Logical(vec![]),
            Kind::Integer => Self::Integer(vec![]),
            Kind::Integer64 => Self::Integer64(vec![]),
            Kind::Text => Self::Text(vec![]),
            Kind::Binary => Self::Binary(vec![]),
            _ => Self::Double(vec![]),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Logical(v) => v.len(),
            Self::Integer(v) => v.len(),
            Self::Integer64(v) => v.len(),
            Self::Double(v) => v.len(),
            Self::Text(v) => v.len(),
            Self::Binary(v) => v.len(),
        }
    }
    pub fn append_range(&mut self, source: &Self, start: usize, end: usize) {
        match (self, source) {
            (Self::Logical(a), Self::Logical(b)) => a.extend_from_slice(&b[start..end]),
            (Self::Integer(a), Self::Integer(b)) => a.extend_from_slice(&b[start..end]),
            (Self::Integer64(a), Self::Integer64(b)) => a.extend_from_slice(&b[start..end]),
            (Self::Double(a), Self::Double(b)) => a.extend_from_slice(&b[start..end]),
            (Self::Text(a), Self::Text(b)) => a.extend_from_slice(&b[start..end]),
            (Self::Binary(a), Self::Binary(b)) => a.extend_from_slice(&b[start..end]),
            _ => unreachable!("conversion plan and buffer disagree"),
        }
    }
}

pub fn conversion(message: impl Into<String>) -> DriverError {
    DriverError::new("conversion", "conversion", message)
}

pub fn date_to_days(d: odbc_api::sys::Date) -> Result<f64> {
    let date = NaiveDate::from_ymd_opt(d.year.into(), d.month.into(), d.day.into())
        .ok_or_else(|| conversion("Invalid database date"))?;
    Ok((date - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days() as f64)
}

pub fn timestamp_to_seconds(t: odbc_api::sys::Timestamp, zone: Tz) -> Result<f64> {
    let dt = NaiveDate::from_ymd_opt(t.year.into(), t.month.into(), t.day.into())
        .and_then(|d| {
            d.and_hms_nano_opt(t.hour.into(), t.minute.into(), t.second.into(), t.fraction)
        })
        .ok_or_else(|| conversion("Invalid database timestamp"))?;
    let dt = zone
        .from_local_datetime(&dt)
        .single()
        .ok_or_else(|| conversion("Ambiguous or nonexistent local timestamp"))?;
    Ok(dt.timestamp() as f64 + dt.timestamp_subsec_nanos() as f64 / 1e9)
}

pub fn parse_time(text: &str) -> Result<f64> {
    let t = NaiveTime::parse_from_str(text, "%H:%M:%S%.f")
        .map_err(|_| conversion("Invalid database time"))?;
    Ok(t.num_seconds_from_midnight() as f64 + t.nanosecond() as f64 / 1e9)
}

pub fn parse_offset(text: &str) -> Result<f64> {
    let dt = chrono::DateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f %:z")
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(text))
        .map_err(|_| conversion("Unsupported timestamp-with-offset representation"))?;
    Ok(dt.timestamp() as f64 + dt.timestamp_subsec_nanos() as f64 / 1e9)
}

pub fn days_to_date(days: f64) -> Result<odbc_api::sys::Date> {
    if !days.is_finite() || days.fract() != 0.0 || days.abs() > 100_000_000.0 {
        return Err(conversion("Date must contain finite whole days"));
    }
    let date = NaiveDate::from_ymd_opt(1970, 1, 1)
        .unwrap()
        .checked_add_signed(chrono::Duration::days(days as i64))
        .ok_or_else(|| conversion("Date outside supported range"))?;
    Ok(odbc_api::sys::Date {
        year: date
            .year()
            .try_into()
            .map_err(|_| conversion("Date year outside ODBC range"))?,
        month: date.month() as u16,
        day: date.day() as u16,
    })
}

pub fn seconds_to_timestamp(seconds: f64, zone: Tz) -> Result<odbc_api::sys::Timestamp> {
    if !seconds.is_finite() || seconds.abs() > 8e12 {
        return Err(conversion("Timestamp outside supported range"));
    }
    let whole = seconds.floor();
    let nanos = ((seconds - whole) * 1e9).floor() as u32;
    let dt: NaiveDateTime = chrono::DateTime::from_timestamp(whole as i64, nanos)
        .ok_or_else(|| conversion("Timestamp outside supported range"))?
        .with_timezone(&zone)
        .naive_local();
    Ok(odbc_api::sys::Timestamp {
        year: dt
            .year()
            .try_into()
            .map_err(|_| conversion("Timestamp year outside ODBC range"))?,
        month: dt.month() as u16,
        day: dt.day() as u16,
        hour: dt.hour() as u16,
        minute: dt.minute() as u16,
        second: dt.second() as u16,
        fraction: dt.nanosecond(),
    })
}
