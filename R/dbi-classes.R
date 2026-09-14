#' DBI objects for the Rust-backed ODBC driver
#'
#' Connection lifecycle and result access methods use the native resource layer.
#' Public execution, table, quoting, and transaction methods remain scaffolded.
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
  slots = c(ptr = "externalptr"),
  prototype = list(ptr = new("externalptr"))
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
#' Use DBI::dbConnect() to open a connection. Public query execution is not yet
#' wired; execution, table, quoting, and transaction methods remain scaffolded.
#' @return An OdbcRsDriver object.
#' @export
odbc_rs <- function() {
  new("OdbcRsDriver")
}

.dbi_not_implemented <- function(method) {
  stop(sprintf("%s() is not implemented for odbc.rs yet.", method), call. = FALSE)
}
