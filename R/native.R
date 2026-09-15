# Private bridge used by the DBI methods. Conditions are raised here,
# after Rust has returned and released its native borrows.
.native_value <- function(reply) {
  if (isTRUE(reply$ok)) {
    return(reply$value)
  }
  error <- reply$error
  error$call <- NULL
  class(error) <- c(
    paste0("odbc_rs_", error$kind, "_error"),
    "odbc_rs_error", "error", "condition"
  )
  stop(error)
}

.native_connect <- function(connection_string, config = list()) {
  .native_value(.Call(wrap__native_connect, connection_string, config))
}

.native_prepare <- function(ptr, sql, statement = FALSE, immediate = FALSE) {
  .native_value(.Call(wrap__native_prepare, ptr, enc2utf8(sql), statement, immediate))
}

.native_bind_scalar <- function(ptr, params = list()) {
  # Normalize R text encodings before the Rust-owned UTF-16 conversion. Preserve
  # factors' missingness and labels without treating their codes as integers.
  if (is.list(params)) {
    params <- lapply(params, function(x) {
      if (is.factor(x)) x <- as.character(x)
      if (is.character(x)) x <- enc2utf8(x)
      x
    })
  }
  .native_value(.Call(wrap__native_bind_scalar, ptr, params))
}

.native_fetch <- function(ptr, n = -1) {
  .native_value(.Call(wrap__native_fetch, ptr, n))
}

.native_clear <- function(ptr) {
  .native_value(.Call(wrap__native_clear, ptr))
}

.native_disconnect <- function(ptr) {
  .native_value(.Call(wrap__native_disconnect, ptr))
}

.native_connection_info <- function(ptr) {
  .native_value(.Call(wrap__native_connection_info, ptr))
}

.native_result_info <- function(ptr) {
  .native_value(.Call(wrap__native_result_info, ptr))
}

.native_connection_status <- function(ptr) {
  .native_value(.Call(wrap__native_connection_status, ptr))
}

.native_connection_valid <- function(ptr) {
  .native_value(.Call(wrap__native_connection_valid, ptr))
}

.native_result_status <- function(ptr) {
  .native_value(.Call(wrap__native_result_status, ptr))
}

.native_has_completed <- function(ptr) {
  .native_value(.Call(wrap__native_has_completed, ptr))
}

.native_column_info <- function(ptr) {
  .native_value(.Call(wrap__native_column_info, ptr))
}

# This helper only allocates/prepares. Send methods execute on the same pointer.
.new_result <- function(conn, sql, statement = FALSE, immediate = FALSE) {
  if (!is(conn, "OdbcRsConnection")) .dbi_argument_error("Expected an OdbcRsConnection")
  sql <- .connection_scalar(sql, "statement")
  if (!is.logical(statement) || length(statement) != 1L || is.na(statement)) {
    .dbi_argument_error("statement must be TRUE or FALSE")
  }
  immediate <- .execution_mode(immediate, FALSE)
  ptr <- .native_prepare(conn@ptr, sql, statement, immediate)
  installed <- FALSE
  on.exit(if (!installed) try(.native_clear(ptr), silent = TRUE), add = TRUE)
  res <- new("OdbcRsResult", ptr = ptr)
  installed <- TRUE
  res
}
