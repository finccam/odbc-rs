# Batch C: parameter arrays and SQL Server tables

Implemented vector/data-frame binding and the required table access/management
methods. The baseline was 258 failures, 0 warnings, 79 skips, and 2,260 passed
expectations. Tests remain the existing DBItest configuration, with one additional
regression covering partial native-array failure reporting.

## Native batches

One input binding uses one ODBC parameter-array execution. Inputs are validated
and converted before any execution and before closing an old result where possible.
Prepared/direct modes share the crate's ParameterCollectionRef execution interface.
The bridge owns every value, NULL indicator, status array, and processed-count
buffer until the statement no longer references them. Variable-width columns use
the crate's TextColumn/BinColumn buffers; a small fixed-width adapter exposes
typed vectors and NULL indicators through the crate's CData interface. Unsafe
code is confined to those documented buffer/binding contracts and statement
attribute calls for parameter status/processed counts.

Native arrays use column-wise layout, equal column lengths, and widths calculated
from all input values. There is no truncation, scalar fallback loop, automatic
retry, transaction wrapper, or implicit batch splitting. Memory depends on row
count times the largest value in each variable-width column. Very large batches
should be submitted explicitly in chunks until configurable chunking is added.

Zero-row prepared bindings close the prior cursor, obtain metadata without
execution, and produce typed empty query results or zero affected rows. They mark
the result bound/completed without creating a native-attempt record. Empty direct
query bindings are explicitly rejected because an unprepared statement cannot
provide the required schema without executing. Zero-row statement bindings are
no-ops. A no-parameter list() represents one parameterless execution; parameter
data frames require at least one column.

For batch queries, compatible driver-produced result sets are consumed with
SQLMoreResults internally and passed through the same bounded-fetch path. This
does not expose a general stored-procedure multiple-result-set API. Statement
batches drain and sum available per-result affected counts; if any required count
is unknown, the aggregate stays unknown.

SQL Server can return SQL_SUCCESS_WITH_INFO while individual parameters fail.
The bridge checks parameter status arrays in addition to the overall return code.
Conditions expose batch_size, batch_processed, batch_succeeded where known, and
batch_outcome_uncertain. The successful count describes driver-reported execution,
not durable commits. Per-row diagnostics may no longer be available after the
crate's result-metadata call; missing SQLSTATE/native codes are not invented.

Native-attempt bookkeeping remains private and is not the future observer API.
Later consumption failures retain the original native call outcome. No fake
per-row execution notifications are introduced.

## Table operations

* SQLTables supplies regular table/view discovery. Default listing omits sys and
  INFORMATION_SCHEMA; an explicit schema filter can include them.
* OBJECT_ID checks resolve SQL Server tables/views on the same session. Field
  discovery executes SELECT TOP (0) through the normal driver query path.
* Character names are literal identifiers; Id/SQL names support schema and
  catalog/schema qualification. Named catalog identifiers must include schema.
* CREATE TABLE uses DBI's SQL generator with the shared quoting/type methods.
* Append uses one parameter-array INSERT with explicit column names, so input
  column order need not match physical column order.
* Read/write compose the shared methods and DBI's row-name helpers. Overwrite and
  append are explicit and mutually exclusive. Field-type overrides are honored.
* Zero-row and all-missing typed data are supported; empty character conversion
  avoids an upstream empty STRING_PTR iterator precondition that can abort Rust.

Temporary names receive a # prefix when needed. A private environment allocated
per R connection maps logical table names to physical session-local names for the
table methods. Aliases are removed on drop or when a stale entry is discovered.
Connections do not share this registry. Explicit # names work directly. Raw SQL
must use the physical # name; raw-SQL-created temporary tables are accessible by
name but are not added to the registry/listing. A future dbplyr adapter must use
the table-path resolver for generated destinations rather than assume an alias
is a server-side object name.

Input shape, type declarations, and native conversion are checked before
destructive write steps. Multi-step writes are not atomic: a new empty table or
partially inserted rows may remain after failure, and an authorized overwrite can
remove the old table before a later error. No cleanup drops unrelated/pre-existing
objects to hide partial effects. Explicit transactions are the next layer.

## Verification

Build/documentation use `rpx run Rscript -e 'devtools::document()'`. The standard
debug build retains the Rust target cache; no explicit clean command was run.

Focused runtime checks exercised native query batches, temporary table writing,
append/read/drop, empty data frames, and a primary-key failure within a batch.
That failure reported 3 processed parameters and 2 successful parameters, matching
the visible rows, while keeping connection usability separate from outcome
uncertainty. No individual ordinary DBItest failures were used to drive feature
scope; the location reporter was used specifically to isolate the native abort.

The first complete post-change DBItest run finished with 70 failures, 0 warnings,
80 skips, and 7,751 passes, without a native abort. The final full suite, including
six passing expectations in the additional regression, finished with **70 failures,
0 warnings, 80 skips, and 7,757 passes**. Existing DBItest configuration and skip
patterns were unchanged. Documentation generation and diff checks completed.
