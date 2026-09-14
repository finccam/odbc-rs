# DBI table access, management, and discovery scaffold.
setMethod("dbReadTable", c("OdbcRsConnection", "character"), function(conn, name, ...) {
  .dbi_not_implemented("dbReadTable")
})
setMethod("dbReadTable", c("OdbcRsConnection", "Id"), function(conn, name, ...) {
  .dbi_not_implemented("dbReadTable")
})
setMethod("dbWriteTable", c("OdbcRsConnection", "Id"), function(conn, name, value, ...) {
  .dbi_not_implemented("dbWriteTable")
})
setMethod("dbExistsTable", c("OdbcRsConnection", "Id"), function(conn, name, ...) {
  .dbi_not_implemented("dbExistsTable")
})
setMethod("dbRemoveTable", c("OdbcRsConnection", "Id"), function(conn, name, ...) {
  .dbi_not_implemented("dbRemoveTable")
})
setMethod("dbListFields", c("OdbcRsConnection", "character"), function(conn, name, ...) {
  .dbi_not_implemented("dbListFields")
})
setMethod("dbListFields", c("OdbcRsConnection", "Id"), function(conn, name, ...) {
  .dbi_not_implemented("dbListFields")
})
setMethod("dbReadTable", "OdbcRsConnection", function(conn, name, ...) {
  .dbi_not_implemented("dbReadTable")
})
setMethod("dbWriteTable", "OdbcRsConnection", function(conn, name, value, ...) {
  .dbi_not_implemented("dbWriteTable")
})
setMethod("dbCreateTable", "OdbcRsConnection", function(conn, name, fields, ..., row.names = NULL, temporary = FALSE) {
  .dbi_not_implemented("dbCreateTable")
})
setMethod("dbAppendTable", "OdbcRsConnection", function(conn, name, value, ..., row.names = NULL) {
  .dbi_not_implemented("dbAppendTable")
})
setMethod("dbExistsTable", "OdbcRsConnection", function(conn, name, ...) {
  .dbi_not_implemented("dbExistsTable")
})
setMethod("dbRemoveTable", "OdbcRsConnection", function(conn, name, ...) {
  .dbi_not_implemented("dbRemoveTable")
})
setMethod("dbListTables", "OdbcRsConnection", function(conn, ...) {
  .dbi_not_implemented("dbListTables")
})
setMethod("dbListFields", "OdbcRsConnection", function(conn, name, ...) {
  .dbi_not_implemented("dbListFields")
})
