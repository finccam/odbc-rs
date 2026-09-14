//! Forward paging over one execution. SQL is never rewritten or re-executed.
use crate::{
    error::{DriverError, Result},
    types::{self, Column, Kind, Options, Values},
};
use odbc_api::{
    buffers::ColumnarDynBuffer,
    handles::{Statement, StatementRef},
    sys::{Date, Timestamp},
    Bit, BlockCursor, Cursor, CursorImpl, CursorRow, Nullable,
};

type BorrowedCursor<'a> = CursorImpl<StatementRef<'a>>;
enum Source<'a> {
    Block(BlockCursor<BorrowedCursor<'a>, ColumnarDynBuffer>),
    Row(BorrowedCursor<'a>),
}

pub struct Fetcher<'a> {
    source: Source<'a>,
    pub columns: Vec<Column>,
    options: Options,
    pending: Vec<Values>,
    offset: usize,
    eof: bool,
}

impl<'a> Fetcher<'a> {
    pub fn close(self) -> Result<()> {
        let cursor = match self.source {
            Source::Block(cursor) => {
                cursor
                    .unbind()
                    .map_err(|e| DriverError::odbc("clear", e))?
                    .0
            }
            Source::Row(cursor) => cursor,
        };
        // Consume the cursor without its panicking Drop, close explicitly, and
        // preserve the ODBC diagnostic for callers.
        let mut statement = cursor.into_stmt();
        statement
            .close_cursor()
            .into_result_without_logging(&statement)
            .map_err(|e| DriverError::odbc("clear", e))
    }

    pub fn new(mut cursor: BorrowedCursor<'a>, options: Options) -> Result<Self> {
        let columns = types::describe(&mut cursor, &options)?;
        let descs: Option<Vec<_>> = columns.iter().map(|c| c.buffer).collect();
        let source = if let Some(descs) = descs {
            let width = descs
                .iter()
                .try_fold(0usize, |total, d| total.checked_add(d.bytes_per_row()));
            match width {
                Some(width) if width > 0 && width <= options.buffer_bytes => {
                    let capacity = (options.buffer_bytes / width).min(options.max_rows).max(1);
                    let buffer = ColumnarDynBuffer::try_from_descs(capacity, descs)
                        .map_err(|e| DriverError::odbc("fetch_buffer", e))?;
                    Source::Block(
                        cursor
                            .bind_buffer(buffer)
                            .map_err(|e| DriverError::odbc("fetch_buffer", e))?,
                    )
                }
                _ => Source::Row(cursor),
            }
        } else {
            Source::Row(cursor)
        };
        let pending = columns.iter().map(|c| Values::empty(c.kind)).collect();
        Ok(Self {
            source,
            columns,
            options,
            pending,
            offset: 0,
            eof: false,
        })
    }

    pub fn complete(&self) -> bool {
        self.eof && self.offset == self.pending_rows()
    }
    pub fn has_completed(&mut self) -> Result<bool> {
        self.refill()?;
        Ok(self.complete())
    }
    fn pending_rows(&self) -> usize {
        self.pending.first().map_or(0, Values::len)
    }

    fn refill(&mut self) -> Result<()> {
        if self.eof || self.offset < self.pending_rows() {
            return Ok(());
        }
        // Drop the old converted block before allocating the next one. The native
        // buffer is reused; pending contains owned values, never references into it.
        self.pending = self.columns.iter().map(|c| Values::empty(c.kind)).collect();
        self.offset = 0;
        match &mut self.source {
            Source::Block(cursor) => {
                match cursor
                    .fetch_with_truncation_check(true)
                    .map_err(|e| DriverError::odbc("fetch", e))?
                {
                    Some(batch) => {
                        self.pending = decode_block(batch, &self.columns, &self.options)?
                    }
                    None => self.eof = true,
                }
            }
            Source::Row(cursor) => {
                match cursor
                    .next_row()
                    .map_err(|e| DriverError::odbc("fetch", e))?
                {
                    Some(mut row) => {
                        self.pending = decode_row(&mut row, &self.columns, &self.options)?
                    }
                    None => self.eof = true,
                }
            }
        }
        // An empty successful rowset must not cause an unbounded refill loop.
        if !self.eof && self.pending_rows() == 0 {
            return Err(DriverError::new(
                "fetch",
                "database",
                "Driver returned an empty successful rowset",
            ));
        }
        Ok(())
    }

    /// None means all remaining rows; zero performs no native fetch.
    pub fn fetch(&mut self, limit: Option<usize>) -> Result<Vec<Values>> {
        let mut out: Vec<_> = self.columns.iter().map(|c| Values::empty(c.kind)).collect();
        if limit == Some(0) {
            return Ok(out);
        }
        let target = limit.unwrap_or(usize::MAX);
        let mut delivered = 0;
        while delivered < target {
            self.refill()?;
            if self.complete() {
                break;
            }
            let take = (target - delivered).min(self.pending_rows() - self.offset);
            for (dest, source) in out.iter_mut().zip(&self.pending) {
                dest.append_range(source, self.offset, self.offset + take);
            }
            self.offset += take;
            delivered += take;
        }
        // Establish exhaustion at an exact boundary, retaining a nonempty next
        // block. A failure here is a consumption failure, never a re-execution.
        self.refill()?;
        Ok(out)
    }
}

fn decode_block(
    batch: &ColumnarDynBuffer,
    columns: &[Column],
    options: &Options,
) -> Result<Vec<Values>> {
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let b = batch.column(index);
            let mismatch =
                || types::conversion("Native buffer does not match the column conversion plan");
            Ok(match column.kind {
                Kind::Logical => Values::Logical(
                    b.as_nullable_slice::<Bit>()
                        .ok_or_else(mismatch)?
                        .map(|v| v.map(|v| v.0 != 0))
                        .collect(),
                ),
                Kind::Integer => Values::Integer(
                    b.as_nullable_slice::<i32>()
                        .ok_or_else(mismatch)?
                        .map(|v| check_integer(v.copied()))
                        .collect::<Result<_>>()?,
                ),
                Kind::Integer64 => Values::Integer64(
                    b.as_nullable_slice::<i64>()
                        .ok_or_else(mismatch)?
                        .map(|v| check_integer64(v.copied()))
                        .collect::<Result<_>>()?,
                ),
                Kind::Double => Values::Double(
                    b.as_nullable_slice::<f64>()
                        .ok_or_else(mismatch)?
                        .map(|v| v.copied())
                        .collect(),
                ),
                Kind::Date => Values::Double(
                    b.as_nullable_slice::<Date>()
                        .ok_or_else(mismatch)?
                        .map(|v| v.copied().map(types::date_to_days).transpose())
                        .collect::<Result<_>>()?,
                ),
                Kind::Timestamp => Values::Double(
                    b.as_nullable_slice::<Timestamp>()
                        .ok_or_else(mismatch)?
                        .map(|v| {
                            v.copied()
                                .map(|t| types::timestamp_to_seconds(t, options.timezone))
                                .transpose()
                        })
                        .collect::<Result<_>>()?,
                ),
                Kind::Text => Values::Text(
                    b.as_wide_text()
                        .ok_or_else(mismatch)?
                        .iter()
                        .map(|v| {
                            v.map(|s| {
                                String::from_utf16(s.as_slice()).map_err(|_| {
                                    types::conversion("Invalid UTF-16 returned by driver")
                                })
                            })
                            .transpose()
                        })
                        .collect::<Result<_>>()?,
                ),
                Kind::Binary => Values::Binary(
                    b.as_binary()
                        .ok_or_else(mismatch)?
                        .iter()
                        .map(|v| v.map(<[u8]>::to_vec))
                        .collect(),
                ),
                _ => return Err(mismatch()),
            })
        })
        .collect()
}

fn check_integer(value: Option<i32>) -> Result<Option<i32>> {
    if value == Some(i32::MIN) {
        Err(types::conversion(
            "SQL integer collides with R's NA sentinel",
        ))
    } else {
        Ok(value)
    }
}
fn check_integer64(value: Option<i64>) -> Result<Option<i64>> {
    if value == Some(i64::MIN) {
        Err(types::conversion(
            "SQL bigint collides with integer64's NA sentinel; use bigint='character'",
        ))
    } else {
        Ok(value)
    }
}

fn decode_row(
    row: &mut CursorRow<'_>,
    columns: &[Column],
    options: &Options,
) -> Result<Vec<Values>> {
    // No bound columns on this path; SQLGetData is called strictly in increasing
    // column order, satisfying the portable ODBC long-data restrictions.
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let index = (index + 1) as u16;
            macro_rules! value {
                ($t:ty) => {{
                    let mut v = Nullable::<$t>::null();
                    row.get_data(index, &mut v)
                        .map_err(|e| DriverError::odbc("fetch", e))?;
                    v.into_opt()
                }};
            }
            Ok(match column.kind {
                Kind::Logical => Values::Logical(vec![value!(Bit).map(|v| v.0 != 0)]),
                Kind::Integer => Values::Integer(vec![check_integer(value!(i32))?]),
                Kind::Integer64 => Values::Integer64(vec![check_integer64(value!(i64))?]),
                Kind::Double => Values::Double(vec![value!(f64)]),
                Kind::Date => {
                    Values::Double(vec![value!(Date).map(types::date_to_days).transpose()?])
                }
                Kind::Timestamp => Values::Double(vec![value!(Timestamp)
                    .map(|t| types::timestamp_to_seconds(t, options.timezone))
                    .transpose()?]),
                Kind::Binary => {
                    let mut bytes = vec![];
                    let present = row
                        .get_binary(index, &mut bytes)
                        .map_err(|e| DriverError::odbc("fetch", e))?;
                    Values::Binary(vec![present.then_some(bytes)])
                }
                Kind::Text | Kind::Time | Kind::TimestampOffset => {
                    let mut units = vec![];
                    let present = row
                        .get_wide_text(index, &mut units)
                        .map_err(|e| DriverError::odbc("fetch", e))?;
                    let text =
                        if present {
                            Some(String::from_utf16(&units).map_err(|_| {
                                types::conversion("Invalid UTF-16 returned by driver")
                            })?)
                        } else {
                            None
                        };
                    match column.kind {
                        Kind::Time => Values::Double(vec![text
                            .as_deref()
                            .map(types::parse_time)
                            .transpose()?]),
                        Kind::TimestampOffset => Values::Double(vec![text
                            .as_deref()
                            .map(types::parse_offset)
                            .transpose()?]),
                        _ => Values::Text(vec![text]),
                    }
                }
            })
        })
        .collect()
}
