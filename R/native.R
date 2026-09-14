# Private bridge for the next DBI wiring batch. Conditions are raised here,
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

.native_prepare <- function(ptr, sql) {
  .native_value(.Call(wrap__native_prepare, ptr, enc2utf8(sql)))
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
