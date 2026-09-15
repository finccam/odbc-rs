#' DBI-managed transactions on one ODBC session
#'
#' Begin, commit, or roll back a transaction using the ODBC connection APIs.
#' Table writing and batch binding do not introduce their own transactions.
#' @param conn An open OdbcRsConnection.
#' @param code R code evaluated in the caller's environment.
#' @param ... Unused arguments. Nested/named transactions and savepoints are not
#'   implemented by these methods.
#' @return Begin/commit/rollback return TRUE invisibly. dbWithTransaction returns
#'   the code's value and visibility, or invisible NULL after DBI::dbBreak().
#' @details
#' Connections start in autocommit mode. dbBegin disables autocommit; the first
#' database operation starts work in the transaction. A second dbBegin raises an
#' error without changing the caller's transaction. Commit/rollback without an
#' active DBI transaction also raises an error.
#'
#' Transaction completion closes cursor streams explicitly. Partially consumed
#' query results become invalid and must be cleared. Exhausted queries, statement
#' results, and unexecuted prepared statements retain their handles for reuse.
#' Cursor closure is not reported as a fetch failure or another SQL execution.
#'
#' Autocommit is restored only after successful commit/rollback. Failed completion
#' or mode restoration marks the connection uncertain; new work is blocked.
#' Explicit rollback may be attempted for a failed transaction, but it does not
#' rehabilitate an already uncertain connection. Dispose of that connection.
#' A condition's transaction_outcome distinguishes unknown outcomes from an
#' acknowledged commit/rollback followed by an autocommit-restoration failure.
#'
#' dbWithTransaction commits successful code, rolls back errors/interrupts and
#' DBI::dbBreak(), and preserves the original error if cleanup rollback also fails.
#' Nonlocal exits from the code also trigger cleanup rollback. Commit errors are
#' propagated without automatic rollback/retry because the outcome may be
#' ambiguous. A failed nested call never takes ownership of the existing transaction.
#'
#' Explicit disconnect attempts rollback before releasing an open transaction.
#' Abandoned connections roll back on final release; live results can retain the
#' session, so explicit cleanup is recommended. No SQL is parsed to detect raw
#' BEGIN/COMMIT/ROLLBACK statements. Do not mix SQL transaction control with these
#' DBI-managed methods. SQL Server transactional DDL applies to supported table
#' operations; administrative statements have their own engine restrictions.
#' Native execution remains blocking; cancellation is a separate capability.
#' Temporary-table aliases are snapshotted at begin and restored after rollback
#' so transactional create/drop operations do not leave stale client-side names.
#' @rdname OdbcRs-transactions
#' @export
setMethod("dbBegin", "OdbcRsConnection", function(conn, ...) {
  .dbi_no_dots(...)
  aliases <- as.list(conn@table_state, all.names = TRUE)
  .native_transaction(conn@ptr, "begin")
  # Snapshot only client-side temporary-name aliases, not native transaction
  # state. Do not replace an existing snapshot if nested begin failed.
  attr(conn@table_state, "transaction_snapshot") <- list(aliases = aliases)
  invisible(TRUE)
})

#' @rdname OdbcRs-transactions
#' @export
setMethod("dbCommit", "OdbcRsConnection", function(conn, ...) {
  .dbi_no_dots(...)
  tryCatch(.native_transaction(conn@ptr, "commit"), error = function(e) {
    if (identical(e$transaction_outcome, "committed")) attr(conn@table_state, "transaction_snapshot") <- NULL
    stop(e)
  })
  attr(conn@table_state, "transaction_snapshot") <- NULL
  invisible(TRUE)
})

#' @rdname OdbcRs-transactions
#' @export
setMethod("dbRollback", "OdbcRsConnection", function(conn, ...) {
  .dbi_no_dots(...)
  tryCatch(.native_transaction(conn@ptr, "rollback"), error = function(e) {
    if (identical(e$transaction_outcome, "rolled_back")) try(.restore_table_aliases(conn), silent = TRUE)
    stop(e)
  })
  .restore_table_aliases(conn)
  invisible(TRUE)
})

.restore_table_aliases <- function(conn) {
  snapshot <- attr(conn@table_state, "transaction_snapshot", exact = TRUE)
  if (!is.null(snapshot)) {
    rm(list = ls(conn@table_state, all.names = TRUE), envir = conn@table_state)
    list2env(snapshot$aliases, envir = conn@table_state)
    attr(conn@table_state, "transaction_snapshot") <- NULL
  }
  invisible(NULL)
}

#' @rdname OdbcRs-transactions
#' @export
setMethod("dbWithTransaction", "OdbcRsConnection", function(conn, code, ...) {
  .dbi_no_dots(...)
  dbBegin(conn)
  owned <- TRUE
  on.exit({
    if (owned) suspendInterrupts(tryCatch(suppressWarnings(dbRollback(conn)), error = function(e) NULL))
  }, add = TRUE)
  aborted <- FALSE
  value <- tryCatch(withVisible(force(code)), dbi_abort = function(e) {
    aborted <<- TRUE
    NULL
  })
  # Finish once, and do not retry an uncertain completion from on.exit.
  suspendInterrupts({
    owned <- FALSE
    if (aborted) dbRollback(conn) else dbCommit(conn)
  })
  if (aborted) return(invisible(NULL))
  if (value$visible) value$value else invisible(value$value)
})
