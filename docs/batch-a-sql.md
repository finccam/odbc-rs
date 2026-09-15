# Batch A: SQL representation

This batch implements quoting and SQL type declarations in R, using the existing
native connection metadata to select the SQL Server path. No Rust resource or
execution code changes are required.

## Public methods

* `dbQuoteIdentifier`: SQL Server brackets, escaped closing brackets, vector names,
  literal dots in character names, and DBI's `Id` component ordering/joining.
* `dbUnquoteIdentifier`: bracketed, double-quoted, and qualified names; escaped
  delimiters and dots inside quoted components are preserved. This parses
  identifiers, not arbitrary SQL or omitted qualification components.
* `dbQuoteString`: Unicode `N'...'` literals with escaped apostrophes and SQL NULLs.
* `dbQuoteLiteral`: strings/factors, logicals, integers, doubles, integer64,
  dates/timestamps/times, and binary values. Rendered SQL passes through unchanged.
* `dbDataType`: generic driver defaults and SQL Server connection declarations;
  data frames return a named declaration vector.

DBI's exact `DBIConnection,Id` method is reused instead of copying its component
logic. `callNextMethod()` is not used for these overloaded signatures because it
can prefer our character/ANY method to DBI's more specific method on the other
argument. SQL identity is an explicit pass-through.

## Choices

Connection declarations follow the current `odbc` SQL Server conventions: BIT,
INT, BIGINT, FLOAT, DATE, DATETIME, TIME, and sized varchar/varbinary. The variable
widths have a minimum of 255 and become max above 8000. Generic driver defaults
follow `odbc`, including INTEGER for integer64; use the connection for actual
database-specific declarations. There is no dependency on the R odbc package.

Unicode string literals use the N prefix. That does not change the underlying
VARCHAR storage convention or imply lossless Unicode storage under every collation.
Datetime literals use DATETIME2(7), independently of the legacy DATETIME column
default. They use the validated server timezone copied into a private connection
slot and round to SQL Server's 100 ns precision. Date/time missing values become
NULL. Infinite values are rejected; numeric NA/NaN follow DBI's NULL convention.
Binary NULL and empty bytes are distinct (`NULL` versus `0x`).

Connection-specific rendering rejects unknown/non-SQL-Server DBMS names rather
than silently applying SQL Server syntax. Metadata discovery uses existing native
APIs and does not execute SQL. No general dialect/plugin framework is introduced.

## Full-suite baseline

The complete existing DBItest configuration was run before and after implementation,
with `testthat::set_max_fails(Inf)` and the standard `devtools::test()` runner via
`rpx run`. Test configuration, compatibility tweaks, and skips were not changed.

| Run | Failed expectations/errors | Warnings | Skips | Passed expectations |
| --- | ---: | ---: | ---: | ---: |
| Before | 517 | 0 | 58 | 475 |
| After | 435 | 0 | 79 | 781 |

This is 82 fewer failures and 306 more passed expectations, not a claim that every
quoting/type path has been verified against SQL Server. Several DBItest scenarios
also require the still-scaffolded public execution/table methods.

The additional 21 skips are previously unreached checks: 19 compatibility-version
gates and 2 DBItest internal limitations. The fixture still uses DBItest's default
compatibility level (1.7.1). No new skip patterns or weakened expectations were added.

An intermediate post-change run exposed S4 dispatch notes; delegation was corrected
and the full suite rerun. Final run: no dispatch notes. Documentation syntax and
`git diff --check` also passed. No native recompilation or cleanup was performed.

References: [DBI identifier semantics](https://dbi.r-dbi.org/reference/dbUnquoteIdentifier.html),
[odbc generic mappings and sizing](https://github.com/r-dbi/odbc/blob/main/R/aaa-odbc-data-type.R),
[odbc SQL Server declarations](https://github.com/r-dbi/odbc/blob/main/R/driver-sql-server.R).
