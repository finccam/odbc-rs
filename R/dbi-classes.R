#' DBI objects for the Rust-backed ODBC driver
#'
#' These classes currently provide the DBI interface scaffold only. Database
#' operations are not implemented. Connection and result objects have private
#' external-pointer slots for the native resource layer.
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
#' Database methods currently stop with a not-implemented error.
#' @return An OdbcRsDriver object.
#' @export
odbc_rs <- function() {
  new("OdbcRsDriver")
}

.dbi_not_implemented <- function(method) {
  stop(sprintf("%s() is not implemented for odbc.rs yet.", method), call. = FALSE)
}
