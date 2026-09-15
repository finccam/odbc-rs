#' Execute SQL and bind scalar parameters
#'
#' Run SQL through the prepared or direct ODBC execution path. Both paths retain
#' the same connection and use the shared result lifecycle and fetching code.
#'
#' @param conn An open OdbcRsConnection.
#' @param statement A single SQL string, optionally wrapped in DBI::SQL().
#' @param params A positional list of scalar parameter columns or a one-row data
#'   frame. Names do not select placeholders. For send/convenience methods, NULL
#'   means no supplied parameters. Binary values use a list-column of raw vectors
#'   or NULL; use an R missing value for a scalar SQL NULL of other types.
#' @param immediate TRUE uses direct execution, FALSE prepares the statement.
#'   NULL selects the method's default: FALSE for send methods; TRUE without
#'   params and FALSE with params for convenience methods. Supplied scalar
#'   parameters are also bound on the direct path; they are not ignored.
#' @param res An OdbcRsResult returned by a send method.
#' @param n Rows requested by dbGetQuery(), with the same meaning as dbFetch().
#' @param ... Unused arguments; supplying any raises an error.
#' @return Send methods return an OdbcRsResult. dbBind() returns that result
#'   invisibly. dbGetQuery() returns a data frame. dbExecute() returns a scalar
#'   affected-row count, or NA when unavailable.
#' @details
#' A prepared statement without placeholders executes during the send call. A
#' parameterized preparation without params remains unexecuted until dbBind().
#' Supplying params executes once before returning. Direct statements execute
#' during the send call and use driver diagnostics to validate their marker
#' count; the backend does not prepare them secretly or parse SQL to count '?'.
#'
#' Rebinding closes the old cursor, replaces the parameters, executes on the same
#' session, refreshes column metadata, and resets per-execution counts. Structural
#' and conversion validation occurs before closing the old cursor where possible.
#' Native execution failure invalidates the result; clear it and create a new
#' result. A failed query never causes an automatic retry or reconnection.
#'
#' This batch supports scalar binding only. Zero-row and multirow parameter
#' batches, table-valued parameters, query cancellation, and R interruption of
#' native execution are not implemented. Unsupported parameter batches fail
#' before execution instead of silently running a subset of their rows.
#'
#' Convenience methods compose the send/bind/fetch/count/clear lifecycle and
#' always clear their result. On an execution, fetch, or R error, cleanup failures
#' do not replace the original condition. After successful work, cleanup failures
#' are surfaced. Bounded dbGetQuery() deliberately clears any unread rows without
#' re-executing the query. Invalid n arguments fail before executing SQL.
#'
#' Whether an execution yields rows is determined by ODBC, not its first SQL
#' keyword. Query-origin results expose fetched rows; statement-origin results
#' expose affected counts. Use dbSendQuery()/dbGetQuery() for statements whose
#' returned rows you want to fetch. Multiple-result-set traversal is not provided.
#' @seealso \code{\link{OdbcRs-result}}, \code{\link{OdbcRs-connection}}
#' @rdname OdbcRs-execution
#' @export
setMethod("dbSendQuery", c("OdbcRsConnection", "character"), function(
  conn, statement, params = NULL, ..., immediate = FALSE
) {
  .dbi_no_dots(...)
  .send_result(conn, statement, params, .execution_mode(immediate, FALSE), statement_result = FALSE)
})

#' @rdname OdbcRs-execution
#' @export
setMethod("dbSendStatement", c("OdbcRsConnection", "character"), function(
  conn, statement, params = NULL, ..., immediate = FALSE
) {
  .dbi_no_dots(...)
  .send_result(conn, statement, params, .execution_mode(immediate, FALSE), statement_result = TRUE)
})

#' @rdname OdbcRs-execution
#' @export
setMethod("dbBind", "OdbcRsResult", function(res, params, ...) {
  .dbi_no_dots(...)
  params <- .scalar_parameters(params)
  .native_bind_scalar(res@ptr, params)
  invisible(res)
})

#' @rdname OdbcRs-execution
#' @export
setMethod("dbGetQuery", c("OdbcRsConnection", "character"), function(
  conn, statement, n = -1, params = NULL, ..., immediate = is.null(params)
) {
  .dbi_no_dots(...)
  n <- .fetch_size(n)
  immediate <- .execution_mode(immediate, is.null(params))
  res <- dbSendQuery(conn, statement, params = params, immediate = immediate)
  .with_result_cleanup(res, dbFetch(res, n = n))
})

#' @rdname OdbcRs-execution
#' @export
setMethod("dbExecute", c("OdbcRsConnection", "character"), function(
  conn, statement, params = NULL, ..., immediate = is.null(params)
) {
  .dbi_no_dots(...)
  immediate <- .execution_mode(immediate, is.null(params))
  res <- dbSendStatement(conn, statement, params = params, immediate = immediate)
  .with_result_cleanup(res, {
    info <- .result_info(res)
    if (!isTRUE(info$executed)) .dbi_argument_error("Statement requires parameters before it can execute")
    dbGetRowsAffected(res)
  })
})

.execution_mode <- function(immediate, default) {
  if (is.null(immediate)) return(default)
  if (!is.logical(immediate) || length(immediate) != 1L || is.na(immediate) || is.object(immediate)) {
    .dbi_argument_error("immediate must be TRUE, FALSE, or NULL")
  }
  immediate
}

.scalar_parameters <- function(params) {
  if (!is.list(params)) .dbi_argument_error("params must be a list or a one-row data frame")
  if (is.data.frame(params) && nrow(params) != 1L) {
    .dbi_argument_error("Zero-row and multirow parameter batches are not implemented")
  }
  if (any(vapply(params, is.data.frame, logical(1)))) {
    .dbi_argument_error("Table-valued parameters are not implemented")
  }
  if (any(lengths(params) != 1L)) {
    .dbi_argument_error("Scalar binding requires one value per parameter; batches are not implemented")
  }
  # Question-mark placeholders are positional, including data-frame columns.
  unname(as.list(params))
}

.send_result <- function(conn, statement, params, immediate, statement_result) {
  if (!is.null(params)) params <- .scalar_parameters(params)
  res <- .new_result(conn, statement, statement = statement_result, immediate = immediate)
  returned <- FALSE
  on.exit(if (!returned) .clear_result_after_error(res), add = TRUE)
  info <- .result_info(res)
  if (immediate || !is.null(params) || isTRUE(info$parameter_count == 0L)) {
    .native_bind_scalar(res@ptr, if (is.null(params)) list() else params)
  }
  returned <- TRUE
  res
}

.clear_result_after_error <- function(res) {
  # Cleanup may also fail; retain the original error or interrupt in that case.
  suspendInterrupts(tryCatch(suppressWarnings(dbClearResult(res)), error = function(e) NULL))
  invisible(NULL)
}

.with_result_cleanup <- function(res, code) {
  succeeded <- FALSE
  on.exit({
    if (succeeded) dbClearResult(res) else .clear_result_after_error(res)
  }, add = TRUE)
  value <- force(code)
  succeeded <- TRUE
  value
}
