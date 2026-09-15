# Native foundation (first implementation batch)

This document records the private Rust resource and conversion foundation.
[Batch 1](batch-1-connection-results.md) wires connection/result methods to
these primitives, and [Batch B](batch-b-execution.md) adds public scalar execution. This is not yet a
usable general DBI driver or a compatibility claim.

[Batch A](batch-a-sql.md) adds SQL Server quoting and type declarations in R;
the native resource layer is shared by prepared and direct execution.

## Ownership and lifecycle

`OdbcRsConnection@ptr` and `OdbcRsResult@ptr` are private extendr external pointers.
Direct S4 construction creates empty pointers, not native handles.

```text
R connection -> ConnectionHandle -> Rc<ConnectionState>
                                      native: Arc<odbc_api::Connection>
                                      weak registry of results

R result -> ResultHandle -> Rc<RefCell<ResultState>>
                              connection lease: Rc<ConnectionState>
                              statement -> Arc<same native connection>
```

The crate's `ConnectionTransitions::into_prepared()` gives each result an owned
statement on the same physical session. `Arc` shares ownership; it does not open
another connection. Calls run serially on R's thread. Multiple live results have
separate statements, parameters, buffers, and fetch positions. Whether the native
driver permits multiple active cursors remains driver-specific. This layer never
silently closes a different result to execute a query.

`Prepared::execute()` returns a cursor borrowing its statement. An `ouroboros`
owning/dependent container keeps that relationship valid across R calls. Its owner
contains the statement and Rust-owned input parameters; its dependent is the
fetcher. No fabricated `'static` cursor borrow or native-pointer cast is used in
package code. The environment's static lifetime comes from `odbc_api::environment()`.

Resource transitions:

```text
prepare -> Prepared
scalar execution -> Active cursor, or Prepared with a completed no-row operation
fetch -> same Active cursor (possibly exhausted)
rebind -> close dependent cursor -> recover Prepared -> execute again
consumption failure -> Failed (native resources released)
clear -> Cleared (native resources and strong connection lease released)
```

Parameter conversion and validation finish before an existing cursor is closed.
Cleared results retain descriptive metadata and a weak connection reference, so
they do not keep an otherwise abandoned connection open. Repeated clearing is
idempotent. Invalid fetch arguments are rejected before native state changes.

Explicit disconnect transitions through Closing, clears registered results, and
releases the native connection. Result borrows are acquired before invalidation.
The crate's connection destructor handles disconnection and its rollback fallback.
Cleanup errors leave connection status Uncertain. There is no transparent retry,
reconnect, or session switching. Dropping only the R connection variable releases
an ownership reference, allowing live results to continue owning the session.

Finalizers release native resources without R callbacks. Package finalizer paths
catch ordinary Rust cleanup panics. Foreign-process use is rejected using the
creating PID; inherited native resources are deliberately not finalized in a fork
child. Create connections inside each worker process instead. This is not process
isolation from a crashing native driver, allocation abort, or R non-local exit.

## Fetching

The conversion plan comes from result metadata after execution, independently of
whether SQL begins with SELECT. Metadata and types are refreshed on rebinding.

For suitable bounded columns, `ColumnarDynBuffer` and `BlockCursor` use typed,
nullable, reusable buffers. Internal defaults are a 4 MiB native-buffer budget,
1,024-row cap, and 64 KiB per-field threshold. They are initial tuning choices,
not release performance budgets. The plan uses the native buffer descriptions'
bytes-per-row estimates to calculate capacity.

Long/unknown-width fields, fractional time, timestamp-with-offset, or a row wider
than the budget select row-wise fetching for the whole result before fetching.
This path calls `get_data()`, `get_wide_text()`, or `get_binary()` in increasing
column order, without bound columns. It obeys the portable SQLGetData ordering
rules without relying on multirow SQLGetData extensions. Large individual values
can still require large allocations; a rowset budget is not a maximum value size.

Each retrieved block is converted into owned native column vectors. These vectors
allow unread rows to survive reuse of the ODBC buffer. An offset tracks which rows
have been delivered. The native buffer and converted pending block are separate
allocations; the output requested by R is additional memory.

* `n = 0`: typed empty output, no cursor advance.
* Positive `n`: consume pending rows first, then fetch as many blocks as needed.
* `n = -1` or `Inf`: accumulate all remaining rows.
* At a consumed block boundary: look ahead and retain the next block, or record EOF.
* Completion: EOF is known and no buffered rows remain. Batch 1 also permits an
  initial bounded prefetch by dbHasCompleted to establish whether a result is empty.
* Truncation: block fetching uses `fetch_with_truncation_check(true)`; it does not
  silently return shortened fields or re-execute the query.

The private interface currently limits a requested/returned data frame to the
ordinary signed-32-bit row-count representation. Batch 1 supports `n = NA` as a
driver-sized fetch; longer data frames are not implemented. Driver/server buffering is outside the package-owned buffer
budget. No OFFSET/LIMIT pagination, cardinality queries, or scrollable cursors are
introduced.

## Data boundary

The initial conventions follow the current R `odbc` implementation, without an
`odbc` dependency or a pinned reference environment:

| SQL family | R output |
| --- | --- |
| BIT | logical |
| TINYINT/SMALLINT/INTEGER | integer |
| BIGINT | integer64 by default; integer/numeric/character options |
| REAL/FLOAT/DOUBLE/DECIMAL/NUMERIC | double |
| Character | character, converted through ODBC UTF-16 |
| Binary | blob list-column; NULL and empty raw remain distinct |
| DATE | Date |
| TIME | hms/difftime in seconds |
| TIMESTAMP | POSIXct/POSIXt with timezone_out |

Unknown/vendor types default to text except for explicit SQL Server TIME2 and
TIMESTAMPOFFSET mappings. Native type descriptions and nullable metadata remain
separate from R-facing classes. Vendor type behavior has not been verified.
DECIMAL-to-double and bigint='numeric' share the usual floating-point precision
limitations; they are not advertised as lossless. Integer values colliding with
R/bit64 missing-value sentinels raise a conversion error instead of becoming NULL.

`timezone` and `timezone_out` default to UTC. Timezone-free timestamps are interpreted
in the configured server zone. Ambiguous/nonexistent local times are rejected.
Timestamp offsets are parsed from driver text on the row-wise path. Fractional
seconds are represented as R doubles, with their normal precision limitations.
Dates and timestamps outside the implemented native/R conversion range fail
explicitly.

Input lists/data frames are validated for parameter count and equal column lengths,
then copied into Rust-owned nullable parameter representations. Supported input
families include logical/integer/double, integer64, character/factor labels, binary
lists, Date, POSIXct, and difftime. Time inputs must fit within one day. No pointer
into an R vector is retained by an ODBC statement. `ParameterBatch` prepares all
rows, but **the private executing entry point is scalar only**: native parameter
arrays, empty-batch execution policy, and DBI batch result aggregation belong to
the next operation-wiring batch. No hidden per-row execution loop is supplied.

The private connect primitive accepts a complete connection string (including DSN
strings), bigint, timezone, timezone_out, and login timeout. Named connection-string
assembly is now provided by batch 1. Encoding/name_encoding overrides, immediate
execution, transactions, and table operations are still to be wired/implemented. Unsupported private options
are rejected rather than ignored.

## Diagnostics and private bridge

`R/native.R` unwraps success/error envelopes after Rust returns. Conditions contain
operation, kind, available SQLSTATE/native code, diagnostic message, and uncertainty.
Raw diagnostics and `dbGetStatement`-style SQL metadata are for R callers, not
observer payloads. Connection strings and credentials are not stored in wrapper
metadata. Native library logging is not configured by this package.

The private `native_attempt` metadata measures the `Prepared::execute()` call,
including the crate's parameter binding and result-set detection. It is **not** the
public execution observer contract or a database span. Native-call success remains
separate from a subsequent fetch/metadata/conversion failure. Observer registration,
exact execution-boundary instrumentation, interruption/cancellation, and adapters
remain later work.

## Build and evidence

* `odbc-api` 29, `ouroboros` 0.18, chrono/chrono-tz, existing extendr 0.9.
* Rust 2024 dependencies use let-chains; package floor is 1.88.
  The minimum-version floor has not been verified on that toolchain.
* Unix builds link `libodbc` (install unixODBC development headers/libraries).
  Windows builds link the system `odbc32` library. Install the vendor ODBC driver
  separately. iODBC and WebAssembly are not configured by this batch.
* No crate sources are vendored in this change. Existing vendor build support is
  retained; its vendor archive must include the new lockfile dependencies before
  an offline build is attempted.
* Compilation checking uses a separate `/tmp/opencode/odbc-rs-check` target, so the
  existing package compilation cache is not cleaned or replaced.
* Tests, database queries, R package installation, and runtime validation were not
  run, as requested. Compilation does not establish native-driver compatibility.

The existing extendr build generates unexported native wrappers. `R/native.R` uses
registered native symbols directly, so it does not require manually modifying the
generated wrapper file in this batch.

References:

* [DBI forward retrieval contract](https://dbi.r-dbi.org/reference/dbFetch.html)
* [SQLFetch rowsets and cursor positioning](https://learn.microsoft.com/en-us/sql/odbc/reference/syntax/sqlfetch-function)
* [ODBC long-data restrictions](https://learn.microsoft.com/en-us/sql/odbc/reference/develop-app/getting-long-data)
* [odbc-api BlockCursor](https://docs.rs/odbc-api/latest/odbc_api/struct.BlockCursor.html)
* [odbc-api owned connection transitions](https://docs.rs/odbc-api/latest/odbc_api/trait.ConnectionTransitions.html)
* [R odbc conversion implementation](https://github.com/r-dbi/odbc/blob/main/src/odbc_result.cpp)
