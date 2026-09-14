# Batch 1: connection and result wiring

This batch replaces the connection/result DBI placeholders with public methods.
Execution methods (`dbSendQuery`, `dbBind`, `dbSendStatement`, `dbGetQuery`, and
`dbExecute`) remain batch 2. Table, quoting, and transaction methods are also still
scaffolded. This is an implementation milestone, not DBI conformance evidence.

## Public connection path

```r
con <- DBI::dbConnect(odbc.rs::odbc_rs(), dsn = "my_dsn")
DBI::dbIsValid(con)
DBI::dbGetInfo(con)
DBI::dbDisconnect(con)
```

The caller can instead supply `.connection_string`, named `driver`, `server`,
`database`, `uid`, `pwd`, and named driver-specific attributes through `...`.
Named scalar values are escaped as ODBC braced values, including doubled closing
braces. Already rendered strings are preserved; their attribute names are parsed
to reject duplicates case-insensitively, including duplicates between the string
and named arguments. The parser recognizes braced and single/double-quoted values
with doubled delimiters. Vendor-specific syntax outside that grammar may require
an extension; it is not silently reinterpreted. No passwords or complete strings
are stored in R connection slots or information lists.

`bigint`, `timezone`, `timezone_out`, and login `timeout` are package options rather
than ODBC attributes. The timeout accepts whole nonnegative seconds, `NA`, and
`Inf`; zero/NA/Inf disable the login timeout. Named connection attributes must be
single nonmissing values. Invalid input is rejected before opening a session.
Nondefault encoding/name_encoding, dbms.name, pre-connect attributes, and
interruptible execution currently raise explicit errors.

Construction transfers a native pointer into `OdbcRsConnection@ptr`. If S4
construction fails, the private constructor attempts explicit cleanup while
preserving the original R error. An R finalizer warns on abandonment without
closing a session retained by live results. The native finalizer releases its
ownership reference. The R finalizer does not capture the connection or pointer
in a closure and does not invoke observers.

## Validity, metadata, and display

* Driver validity is true for a driver instance.
* Connection validity calls the crate's `is_dead()` (SQL_ATTR_CONNECTION_DEAD)
  where available. Unsupported-attribute diagnostics fall back to local state.
  Other failures or a reported dead connection mark it uncertain and return false.
  This never issues SQL and cannot predict the next network operation.
* Result validity checks local resource/session state; an exhausted result is
  still valid. Empty/serialized pointers and foreign-process objects are invalid.
* Display uses local snapshots only. Fixed lifecycle labels are shown, without
  raw pointers, SQL, connection strings, or server-provided text.
* Driver information contains package version and an unknown client version:
  the ODBC client depends on the connection's vendor driver.
* Connection information reads DBMS name and catalog using the safe native API.
  `odbc-api` 29's high-level connection type has no generic connected-driver
  name/version, DBMS version, server-name, or username getters. Those fields are
  explicitly `NA`, as is port. This is an upstream API gap, not a claim that the
  underlying ODBC SQLGetInfo interface cannot provide them. A safe upstream
  accessor is the preferred follow-up; no connection-layout casts or fabricated
  metadata were introduced.
* Result information contains `statement`, `row.count`, `rows.affected`, and
  `has.completed`. Access after invalidation raises an error.

## Result access and counts

The result records `Query` or `Statement` at construction, separately from whether
the executed SQL produces a cursor. This is necessary because DBI specifies:

| API origin | dbGetRowCount | dbGetRowsAffected |
| --- | --- | --- |
| dbSendQuery | Rows delivered by dbFetch | Zero |
| dbSendStatement | Zero | Known native affected rows, otherwise NA |

Counts return an R integer where it fits, otherwise numeric. Affected counts are
captured immediately after the native execution, including statements that produce
a cursor. They do not change with fetching. Calling dbFetch on an executed statement
result warns and returns an empty data frame; callers wanting returned rows should
use a query result. These distinctions are driven by the API origin, not SQL text.

`dbFetch` validates n first: zero, a nonnegative whole integer/numeric scalar,
`-1`, `Inf`, and `NA` are supported. NA selects up to the internal default 1,024
rows. Negative infinity, NaN, classed vectors, fractional sizes, and vector sizes
are rejected without advancing the cursor. The existing 32-bit data-frame row
limit remains explicit. Complete and partial fetches retain the existing buffer
offset, long-value fallback, look-ahead, and typed-empty behavior.

`dbHasCompleted` may prefetch the initial block to establish whether an executed
query is empty. Prefetched rows are retained, not counted as delivered. Afterward,
completion normally reads the buffered/EOF state established by look-ahead.
This is result consumption, not another SQL execution. An unexecuted preparation
returns false. A consumption failure preserves the native attempt outcome and
invalidates the result while retaining its diagnostic.

`dbColumnInfo` returns name/type first, then `.sql.type` and `.nullable`. Native
metadata and R type names are kept distinct. `.sql.type` describes the ODBC type;
it is not a rendered SQL declaration. Unnamed native columns get ordinal V1/V2
names shared by fetching and column information; existing names are untouched.
Metadata for an unexecuted prepared statement uses driver metadata APIs, without
executing it or fetching rows. Unsupported metadata queries raise their native
diagnostic rather than inventing a schema.

## Cleanup

Repeated clear/disconnect calls warn and return TRUE invisibly. Empty/stale and
foreign-process objects follow the invalid-object warning path without touching
inherited native resources. Failed results can still be cleared. An uncertain
connection can still be explicitly disposed of.

Disconnect with active result/statement resources warns before any mutation,
then clears them through the native registry and releases the session. This also
means `options(warn = 2)` stops the operation before cleanup starts. Other cleanup
failures preserve the first error, mark uncertainty, and release resources as far
as the native layer permits. Finalizer cleanup remains an idempotent fallback.

## Exact handoff to batch 2

`R/native.R` supplies `.new_result(conn, sql, statement = FALSE)`:

1. Validate the connection type, SQL scalar, and result-kind flag.
2. Call `.native_prepare(conn@ptr, sql, statement)`.
3. Put the pointer into a new OdbcRsResult, with failure cleanup around S4 creation.

It **only prepares**. Batch 2 must:

* Pass `statement = FALSE` for dbSendQuery and TRUE for dbSendStatement.
* Preserve that kind for both immediate and prepared native execution paths.
* Execute parameterless SQL before returning from a send method; leave genuinely
  parameterized statements ready for dbBind unless params are supplied.
* Reuse the same result pointer for scalar/repeated binding; the private
  `.native_bind_scalar()` is available for scalar execution.
* Implement native parameter arrays and empty-batch semantics rather than treating
  the scalar primitive as a batch implementation.
* Clear a newly created result on send/bind construction failure, preserving the
  original error if cleanup fails.
* Build convenience methods by composing the same lifecycle rather than creating
  an independent execution/accounting path.

The private `native_attempt` remains a crate-call diagnostic, not the public
execution observer interface. It must not be exported directly as a Sentry span.

## Evidence

Rust compilation and diff/static review only. No tests, database queries, package
installation, or runtime validation were run, as requested. R package-management
changes use rpx; any R commands must use `rpx run`. Compilation uses the separate
`/tmp/opencode/odbc-rs-check` target and does not clean the existing build cache.

References: [DBI connection information](https://dbi.r-dbi.org/reference/dbGetInfo.html),
[row counts](https://dbi.r-dbi.org/reference/dbGetRowCount.html),
[affected counts](https://dbi.r-dbi.org/reference/dbGetRowsAffected.html),
[column information](https://dbi.r-dbi.org/reference/dbColumnInfo.html),
[clear](https://dbi.r-dbi.org/reference/dbClearResult.html),
[disconnect](https://dbi.r-dbi.org/reference/dbDisconnect.html).
