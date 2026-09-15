#' Fetch and manage a Rust-backed ODBC result
#'
#' Results retain their cursor and any unread native buffer between calls.
#' @rdname OdbcRs-result
#' @export
setMethod("dbFetch", "OdbcRsResult", function(res, n = -1, ...) {
  .dbi_no_dots(...)
  n <- .fetch_size(n)
  info <- .result_info(res)
  if (!isTRUE(info$bound)) {
    .dbi_argument_error("Result must be bound/executed before fetching")
  }
  if (identical(info$kind, "statement")) {
    warning("Fetching a statement result returns an empty data frame", call. = FALSE)
  }
  .native_fetch(res@ptr, n)
})

.fetch_size <- function(n) {
  if (length(n) != 1L || is.object(n) || !(typeof(n) %in% c("integer", "double") || identical(n, NA))) {
    .dbi_argument_error("n must be a single whole number, NA, or Inf")
  }
  if (is.nan(n)) .dbi_argument_error("n must not be NaN")
  if (is.na(n)) return(NA_real_)
  if (n == Inf || n == -1) return(as.double(n))
  if (!is.finite(n) || n < 0 || n != floor(n) || n > .Machine$integer.max) {
    .dbi_argument_error("n must be -1, NA, Inf, or a nonnegative whole number within the supported data-frame row limit")
  }
  as.double(n)
}

.result_info <- function(res) {
  .native_result_info(res@ptr)
}

.result_row_count <- function(info) {
  if (identical(info$kind, "statement")) 0L else .dbi_count(info$rows_delivered)
}

.result_rows_affected <- function(info) {
  if (identical(info$kind, "query")) 0L else .dbi_count(info$rows_affected)
}

.dbi_count <- function(value) {
  if (is.null(value) || !length(value) || is.na(value)) return(NA_integer_)
  if (value <= .Machine$integer.max) as.integer(value) else as.double(value)
}

#' @rdname OdbcRs-result
#' @export
setMethod("dbClearResult", "OdbcRsResult", function(res, ...) {
  .dbi_no_dots(...)
  status <- tryCatch(.native_result_status(res@ptr), odbc_rs_state_error = function(e) NULL)
  if (is.null(status) || !isTRUE(status$local) || identical(status$state, "cleared")) {
    warning("Result is already cleared or invalid", call. = FALSE)
    return(invisible(TRUE))
  }
  # Failed/uncertain results must still be cleaned up; dbIsValid() is not a
  # prerequisite for releasing resources.
  .native_clear(res@ptr)
  invisible(TRUE)
})

#' @rdname OdbcRs-result
#' @export
setMethod("dbHasCompleted", "OdbcRsResult", function(res, ...) {
  .dbi_no_dots(...)
  .native_has_completed(res@ptr)
})

#' @rdname OdbcRs-result
#' @export
setMethod("dbGetRowCount", "OdbcRsResult", function(res, ...) {
  .dbi_no_dots(...)
  .result_row_count(.result_info(res))
})

#' @rdname OdbcRs-result
#' @export
setMethod("dbGetRowsAffected", "OdbcRsResult", function(res, ...) {
  .dbi_no_dots(...)
  .result_rows_affected(.result_info(res))
})

#' @rdname OdbcRs-result
#' @export
setMethod("dbColumnInfo", "OdbcRsResult", function(res, ...) {
  .dbi_no_dots(...)
  columns <- .native_column_info(res@ptr)
  data.frame(
    name = vapply(columns, function(x) x$name, character(1)),
    type = vapply(columns, function(x) x$r_type, character(1)),
    .sql.type = vapply(columns, function(x) x$native_type, character(1)),
    .nullable = vapply(columns, function(x) {
      if (is.null(x$nullable) || !length(x$nullable)) NA else as.logical(x$nullable)
    }, logical(1)),
    stringsAsFactors = FALSE, check.names = FALSE
  )
})

#' @rdname OdbcRs-result
#' @export
setMethod("dbGetStatement", "OdbcRsResult", function(res, ...) {
  .dbi_no_dots(...)
  .result_info(res)$statement
})
