#' DBI objects for the Rust-backed ODBC driver
#'
#' Connection lifecycle and result access methods use the native resource layer.
#' SQL Server quoting, type declarations, execution, binding, and table methods
#' and DBI-managed transactions are supported.
#' Connections and results have private external-pointer slots.
#' @import methods
#' @import DBI
#' @export
setClass("OdbcRsDriver", contains = "DBIDriver")

#' @rdname OdbcRsDriver-class
#' @export
setClass(
  "OdbcRsConnection",
  contains = "DBIConnection",
  slots = c(ptr = "externalptr", timezone = "character", table_state = "environment"),
  prototype = list(ptr = new("externalptr"), timezone = "UTC", table_state = new.env(parent = emptyenv()))
)

#' @rdname OdbcRsDriver-class
#' @export
setClass(
  "OdbcRsResult",
  contains = "DBIResult",
  slots = c(ptr = "externalptr"),
  prototype = list(ptr = new("externalptr"))
)

#' Create a Rust-backed ODBC driver object
#'
#' Constructs the driver object without opening a database connection.
#' Use DBI::dbConnect() to open a connection. Query execution, parameter batches,
#' table methods, and DBI-managed transactions are supported.
#' @return An OdbcRsDriver object.
#' @export
odbc_rs <- function() {
  new("OdbcRsDriver")
}

.dbi_not_implemented <- function(method) {
  stop(sprintf("%s() is not implemented for odbc.rs yet.", method), call. = FALSE)
}
