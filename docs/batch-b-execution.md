# Batch B: public execution and scalar binding

Implemented `dbSendQuery`, `dbSendStatement`, `dbBind`, `dbGetQuery`, and
`dbExecute`. Public methods use one shared result lifecycle for both prepared and
direct execution. No SQL rewriting, session switching, or retry path was added.

## Native ownership

`NativeStatement` wraps either the crate's owned `Prepared` or `Preallocated`
statement. `StatementOwner` retains it with Rust-owned input parameters. Both
execution APIs return the same borrowed cursor type, so the existing owning/
dependent container, block/row fetching, metadata, and cleanup remain shared.

Direct execution uses `Preallocated::execute(sql, params)`. Preparing uses
`ConnectionTransitions::into_prepared()` and `Prepared::execute(params)`.
Prepared marker counts come from ODBC metadata. Direct counts are unknown before
execution and validated by the driver; the backend neither secretly prepares
direct SQL nor invents a marker count with a SQL-text scan.

## Public semantics

| Operation | Default behavior |
| --- | --- |
| Send without immediate | Prepared mode |
| Parameterless prepared send | Prepare, then execute once |
| Parameterized prepared send without params | Prepare only; bind later |
| Send with scalar params | Bind and execute once before returning |
| Send with immediate=TRUE | Direct native execution, including scalar params if supplied |
| dbGetQuery/dbExecute without params | Direct mode |
| dbGetQuery/dbExecute with params | Prepared mode |

Explicit TRUE/FALSE values select the mode. NULL selects the method's default.
The result kind is selected by the DBI entry point, not the SQL keyword. Native
result metadata determines whether a cursor exists. Use the query entry points
to retrieve returned rows, including row-returning DML.

`dbBind` accepts a positional list or a one-row data frame. Names are ignored,
matching positional question-mark placeholders. Input shape and conversion are
validated before executing or replacing an existing cursor where possible.
Rebinding uses the same result and connection, discards the previous cursor,
refreshes metadata, and resets counts. Zero-row/multirow batches and TVPs are
explicitly rejected; native array execution is the next layer, not an implicit
scalar loop hidden here.

Convenience methods compose the public send/fetch/count/clear methods. They clear
on success and failure, preserving an original error or interrupt if cleanup also
fails. Invalid fetch limits are rejected before SQL executes. A partial
`dbGetQuery(n=...)` intentionally clears unread rows without a warning or another
execution. Prepared convenience calls that still lack required parameters fail
and clear their result.

The private native-attempt diagnostic remains a measurement of the crate call,
including its binding and result-set detection. It is not the public observer
contract or a Sentry span. Cancellation, transaction APIs, table APIs, native
parameter batches, and multiple-result-set traversal remain subsequent work.

## Evidence

The full, unchanged DBItest configuration ran before and after this batch using
`rpx run Rscript`, `testthat::set_max_fails(Inf)`, and `devtools::test()`:

| Run | Fail | Warn | Skip | Pass |
| --- | ---: | ---: | ---: | ---: |
| Before | 435 | 0 | 79 | 781 |
| After | 258 | 0 | 79 | 2260 |

This is implementation progress, not complete conformance. No test skips or
compatibility tweaks were changed. The remaining surface is still incomplete.

Rust type-checking and a native build/relink completed without cleaning the
existing cache. The initial build was invoked manually; after workflow correction,
`devtools::document()` was run before the post-change suite. Use that project
workflow for subsequent documentation/build updates. Older hand-written Rd files
and NAMESPACE are protected from regeneration by roxygen; this batch's execution
documentation is generated from `R/dbi-execution.R`.
