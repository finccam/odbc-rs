#' Connect and inspect Rust-backed ODBC objects
#'
#' Connections use the same named ODBC attribute conventions as odbc. Public
#' execution methods are implemented in the following batch.
#' @rdname OdbcRs-connection
#' @export
setMethod("dbConnect", "OdbcRsDriver", function(
  drv, dsn = NULL, ..., timezone = "UTC", timezone_out = "UTC",
  bigint = c("integer64", "integer", "numeric", "character"), timeout = 10,
  driver = NULL, server = NULL, database = NULL, uid = NULL, pwd = NULL,
  .connection_string = NULL, encoding = "", name_encoding = "",
  dbms.name = NULL, attributes = NULL, interruptible = FALSE
) {
  if (!identical(encoding, "") || !identical(name_encoding, "")) {
    .dbi_argument_error("encoding and name_encoding overrides are not implemented; native text uses ODBC Unicode conversion")
  }
  if (!is.null(dbms.name) || !is.null(attributes)) {
    .dbi_argument_error("dbms.name overrides and pre-connect attributes are not implemented")
  }
  if (!identical(interruptible, FALSE)) {
    .dbi_argument_error("Interruptible execution is not implemented in this batch")
  }
  config <- list(
    bigint = match.arg(bigint),
    timezone = .connection_scalar(timezone, "timezone"),
    timezone_out = .connection_scalar(timezone_out, "timezone_out"),
    timeout = .connection_timeout(timeout)
  )
  common <- list(DSN = dsn, Driver = driver, Server = server,
                 Database = database, UID = uid, PWD = pwd)
  common <- common[!vapply(common, is.null, logical(1))]
  text <- .build_connection_string(.connection_string, c(common, list(...)))
  ptr <- .native_connect(text, config)
  installed <- FALSE
  on.exit(if (!installed) try(.native_disconnect(ptr), silent = TRUE), add = TRUE)
  conn <- new("OdbcRsConnection", ptr = ptr, timezone = config$timezone)
  # This callback captures no connection or pointer. It reports abandonment but
  # leaves the lifetime of a session retained by live results to native ownership.
  reg.finalizer(ptr, .finalize_connection, onexit = TRUE)
  installed <- TRUE
  conn
})

.finalize_connection <- function(ptr) {
  tryCatch({
    info <- .native_connection_status(ptr)
    if (isTRUE(info$local) && isTRUE(info$has_native)) {
      warning("ODBC connection was released without dbDisconnect(); live results may still retain the session", call. = FALSE)
    }
  }, error = function(e) NULL)
  invisible(NULL)
}

#' @rdname OdbcRs-connection
#' @export
setMethod("dbDisconnect", "OdbcRsConnection", function(conn, ...) {
  .dbi_no_dots(...)
  info <- tryCatch(.native_connection_status(conn@ptr), odbc_rs_state_error = function(e) NULL)
  if (is.null(info) || !isTRUE(info$local) || !isTRUE(info$has_native)) {
    warning("Connection is already disconnected or invalid", call. = FALSE)
    return(invisible(TRUE))
  }
  # Warnings-as-errors must occur before any cleanup or state transition.
  if (!isTRUE(info$valid)) warning("Disposing of an invalid or uncertain connection", call. = FALSE)
  if (info$active_results > 0) warning("Disconnecting with active results; those results will be cleared", call. = FALSE)
  .native_disconnect(conn@ptr)
  invisible(TRUE)
})

#' @rdname OdbcRs-connection
#' @export
setMethod("dbIsValid", "OdbcRsDriver", function(dbObj, ...) {
  .dbi_no_dots(...)
  TRUE
})

#' @rdname OdbcRs-connection
#' @export
setMethod("dbIsValid", "OdbcRsConnection", function(dbObj, ...) {
  .dbi_no_dots(...)
  tryCatch(isTRUE(.native_connection_valid(dbObj@ptr)), odbc_rs_error = function(e) FALSE)
})

#' @rdname OdbcRs-connection
#' @export
setMethod("dbIsValid", "OdbcRsResult", function(dbObj, ...) {
  .dbi_no_dots(...)
  tryCatch(isTRUE(.native_result_status(dbObj@ptr)$valid), odbc_rs_error = function(e) FALSE)
})

#' @rdname OdbcRs-connection
#' @export
setMethod("dbGetInfo", "OdbcRsDriver", function(dbObj, ...) {
  .dbi_no_dots(...)
  list(driver.version = as.character(utils::packageVersion("odbc.rs")),
       client.version = NA_character_, client = "ODBC (connection-specific driver)")
})

#' @rdname OdbcRs-connection
#' @export
setMethod("dbGetInfo", "OdbcRsConnection", function(dbObj, ...) {
  .dbi_no_dots(...)
  info <- .native_connection_info(dbObj@ptr)
  list(
    db.version = .metadata_string(info$dbms_version),
    dbname = .metadata_string(info$database),
    username = .metadata_string(info$username),
    host = .metadata_string(info$server), port = NA_integer_,
    dbms.name = .metadata_string(info$dbms_name),
    driver.name = .metadata_string(info$driver_name),
    driver.version = .metadata_string(info$driver_version),
    connection.id = info$id
  )
})

.metadata_string <- function(value) {
  if (is.null(value) || !length(value)) NA_character_ else as.character(value)
}

#' @rdname OdbcRs-connection
#' @export
setMethod("dbGetInfo", "OdbcRsResult", function(dbObj, ...) {
  .dbi_no_dots(...)
  info <- .result_info(dbObj)
  list(statement = info$statement, row.count = .result_row_count(info),
       rows.affected = .result_rows_affected(info), has.completed = dbHasCompleted(dbObj))
})

# Display uses local snapshots only. It never sends SQL, probes the connection,
# or renders SQL, server-provided text, credentials, or native addresses.
#' @rdname OdbcRs-connection
#' @export
setMethod("show", "OdbcRsDriver", function(object) {
  cat("<OdbcRsDriver>\n")
  invisible(object)
})

#' @rdname OdbcRs-connection
#' @export
setMethod("show", "OdbcRsConnection", function(object) {
  info <- tryCatch(.native_connection_status(object@ptr), odbc_rs_error = function(e) NULL)
  status <- if (is.null(info) || !isTRUE(info$local)) "invalid" else tolower(info$status)
  cat("<OdbcRsConnection: ", status, ">\n", sep = "")
  invisible(object)
})

#' @rdname OdbcRs-connection
#' @export
setMethod("show", "OdbcRsResult", function(object) {
  info <- tryCatch(.native_result_status(object@ptr), odbc_rs_error = function(e) NULL)
  status <- if (is.null(info) || !isTRUE(info$local)) "invalid" else info$state
  cat("<OdbcRsResult: ", status, ">\n", sep = "")
  invisible(object)
})
