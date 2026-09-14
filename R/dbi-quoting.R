# DBI quoting and type declaration scaffold.
setMethod("dbQuoteIdentifier", c("OdbcRsConnection", "character"), function(conn, x, ...) {
  .dbi_not_implemented("dbQuoteIdentifier")
})
setMethod("dbQuoteIdentifier", c("OdbcRsConnection", "Id"), function(conn, x, ...) {
  .dbi_not_implemented("dbQuoteIdentifier")
})
setMethod("dbQuoteIdentifier", c("OdbcRsConnection", "SQL"), function(conn, x, ...) {
  .dbi_not_implemented("dbQuoteIdentifier")
})
setMethod("dbQuoteString", c("OdbcRsConnection", "character"), function(conn, x, ...) {
  .dbi_not_implemented("dbQuoteString")
})
setMethod("dbQuoteString", c("OdbcRsConnection", "SQL"), function(conn, x, ...) {
  .dbi_not_implemented("dbQuoteString")
})
setMethod("dbQuoteIdentifier", "OdbcRsConnection", function(conn, x, ...) {
  .dbi_not_implemented("dbQuoteIdentifier")
})
setMethod("dbUnquoteIdentifier", "OdbcRsConnection", function(conn, x, ...) {
  .dbi_not_implemented("dbUnquoteIdentifier")
})
setMethod("dbQuoteString", "OdbcRsConnection", function(conn, x, ...) {
  .dbi_not_implemented("dbQuoteString")
})
setMethod("dbQuoteLiteral", "OdbcRsConnection", function(conn, x, ...) {
  .dbi_not_implemented("dbQuoteLiteral")
})
setMethod("dbDataType", "OdbcRsDriver", function(dbObj, obj, ...) {
  .dbi_not_implemented("dbDataType")
})
setMethod("dbDataType", "OdbcRsConnection", function(dbObj, obj, ...) {
  .dbi_not_implemented("dbDataType")
})
