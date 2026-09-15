\name{OdbcRs-result}
\alias{OdbcRs-result}
\alias{dbFetch,OdbcRsResult,ANY-method}
\alias{dbClearResult,OdbcRsResult-method}
\alias{dbHasCompleted,OdbcRsResult-method}
\alias{dbGetRowCount,OdbcRsResult-method}
\alias{dbGetRowsAffected,OdbcRsResult-method}
\alias{dbColumnInfo,OdbcRsResult-method}
\alias{dbGetStatement,OdbcRsResult-method}
\title{Fetch and manage a Rust-backed ODBC result}
\description{Access rows, metadata, counts, and lifecycle state for a result
created by dbSendQuery() or dbSendStatement().}
\usage{
\S4method{dbFetch}{OdbcRsResult,ANY}(res, n = -1, ...)
\S4method{dbClearResult}{OdbcRsResult}(res, ...)
\S4method{dbHasCompleted}{OdbcRsResult}(res, ...)
\S4method{dbGetRowCount}{OdbcRsResult}(res, ...)
\S4method{dbGetRowsAffected}{OdbcRsResult}(res, ...)
\S4method{dbColumnInfo}{OdbcRsResult}(res, ...)
\S4method{dbGetStatement}{OdbcRsResult}(res, ...)
}
\arguments{
\item{res}{An OdbcRsResult owning a native statement/result.}
\item{n}{Maximum rows to fetch. Zero returns typed empty columns. \code{-1}
or \code{Inf} returns all remaining rows; \code{NA} requests up to 1,024 rows.
Other values must be nonnegative whole integer/numeric scalars no greater than
\code{.Machine$integer.max}. NaN and classed values are rejected.}
\item{...}{Unused arguments; supplying any raises an error.}
}
\value{\code{dbFetch()} returns a data frame. \code{dbClearResult()} returns
\code{TRUE} invisibly. \code{dbHasCompleted()} returns a logical. Count methods
return scalar integers or numerics. \code{dbColumnInfo()} returns a data frame
with name, type, .sql.type, and .nullable columns. \code{dbGetStatement()} returns
the original SQL string.}
\details{
Fetching proceeds forward through the same cursor, consuming buffered rows before
requesting more. Names, types, and attributes remain consistent across chunks,
including empty and exhausted output. Invalid fetch sizes fail before native
state changes. Fetching an unexecuted preparation raises an error.

Counts distinguish the DBI entry point used to construct the result: query results
report delivered rows and zero affected rows; statement results report zero
delivered rows and known affected rows (or \code{NA}). Counts are not inferred from
the first SQL keyword. Fetching an executed statement result warns and returns an
empty data frame. Affected counts are captured at execution, not recomputed after
fetching.

Completion may prefetch the first native block to discover an empty result, but
does not count those rows as delivered. Subsequent checks use pending-buffer and
EOF state. No new SQL execution is created. Prepared statements are not completed
until executed. A failed fetch/completion invalidates the result; cleanup preserves
the original error. Native success is not rewritten after a consumption failure.

Column information for an unexecuted preparation is obtained through driver
metadata APIs when supported. Driver metadata failures are surfaced. Empty native
column names receive V1, V2, etc. according to ordinal position; existing names are
not repaired. The .sql.type column describes the native ODBC type and is not a
portable CREATE TABLE declaration. Unknown nullability is \code{NA}.

Clearing promptly releases the cursor, statement, buffers, and connection lease.
Repeated clearing warns. Failed or uncertain results can still be cleared.
Metadata/count methods and fetching raise an error after clear or disconnect.
Results and their pointers cannot be transferred between processes.
}
\seealso{\code{\link{OdbcRs-connection}}}
