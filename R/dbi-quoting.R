#' Quote SQL Server identifiers and values
#' @rdname OdbcRs-sql
#' @export
setMethod("dbQuoteIdentifier", c("OdbcRsConnection", "character"), function(conn, x, ...) {
  .dbi_no_dots(...)
  .require_sql_server(conn)
  if (anyNA(x)) .dbi_argument_error("Identifiers cannot contain NA")
  out <- if (length(x)) paste0("[", gsub("]", "]]", enc2utf8(x), fixed = TRUE), "]") else character()
  DBI::SQL(out, names = names(x))
})

# Reuse DBI's component ordering/joining. Select its exact method because
# callNextMethod() can prefer our character/ANY methods on another argument.
#' @rdname OdbcRs-sql
#' @export
setMethod("dbQuoteIdentifier", c("OdbcRsConnection", "Id"), function(conn, x, ...) {
  .dbi_no_dots(...)
  methods::getMethod("dbQuoteIdentifier", c("DBIConnection", "Id"))(conn, x)
})

#' @rdname OdbcRs-sql
#' @export
setMethod("dbQuoteIdentifier", c("OdbcRsConnection", "SQL"), function(conn, x, ...) {
  .dbi_no_dots(...)
  x
})

#' @rdname OdbcRs-sql
#' @export
setMethod("dbQuoteIdentifier", "OdbcRsConnection", function(conn, x, ...) {
  .dbi_no_dots(...)
  .dbi_argument_error("x must be character, Id, or SQL")
})

#' @rdname OdbcRs-sql
#' @export
setMethod("dbUnquoteIdentifier", "OdbcRsConnection", function(conn, x, ...) {
  .dbi_no_dots(...)
  if (is(x, "Id")) return(list(x))
  if (!is.character(x)) .dbi_argument_error("x must be character, Id, or SQL")
  .require_sql_server(conn)
  if (anyNA(x)) .dbi_argument_error("Identifiers cannot contain NA")
  out <- lapply(as.character(x), function(value) DBI::Id(.identifier_parts(value)))
  names(out) <- names(x)
  out
})

#' @rdname OdbcRs-sql
#' @export
setMethod("dbQuoteString", c("OdbcRsConnection", "character"), function(conn, x, ...) {
  .dbi_no_dots(...)
  .require_sql_server(conn)
  DBI::SQL(.sql_server_strings(x), names = names(x))
})

#' @rdname OdbcRs-sql
#' @export
setMethod("dbQuoteString", c("OdbcRsConnection", "SQL"), function(conn, x, ...) {
  .dbi_no_dots(...)
  x
})

#' @rdname OdbcRs-sql
#' @export
setMethod("dbQuoteString", "OdbcRsConnection", function(conn, x, ...) {
  .dbi_no_dots(...)
  .dbi_argument_error("x must be character or SQL")
})

#' @rdname OdbcRs-sql
#' @export
setMethod("dbQuoteLiteral", "OdbcRsConnection", function(conn, x, ...) {
  .dbi_no_dots(...)
  if (is(x, "SQL")) return(x)
  .require_sql_server(conn)
  DBI::SQL(.sql_server_literals(x, conn@timezone), names = if (is.raw(x)) NULL else names(x))
})
