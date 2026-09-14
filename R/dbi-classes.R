#' DBI objects for the Rust-backed ODBC driver
#'
#' These classes currently provide the DBI interface scaffold only. Database
#' operations are not implemented. Native resource slots will be added with
#' the Rust backend.
#' @import methods
#' @import DBI
#' @export
setClass("OdbcRsDriver", contains = "DBIDriver")

#' @rdname OdbcRsDriver-class
#' @export
setClass("OdbcRsConnection", contains = "DBIConnection")

#' @rdname OdbcRsDriver-class
#' @export
setClass("OdbcRsResult", contains = "DBIResult")

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
