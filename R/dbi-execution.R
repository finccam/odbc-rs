# DBI execution and binding scaffold. Execution options are accepted via ...
# until their native implementations are introduced.
setMethod("dbSendQuery", c("OdbcRsConnection", "character"), function(conn, statement, ...) {
  .dbi_not_implemented("dbSendQuery")
})
setMethod("dbSendStatement", c("OdbcRsConnection", "character"), function(conn, statement, ...) {
  .dbi_not_implemented("dbSendStatement")
})
setMethod("dbGetQuery", c("OdbcRsConnection", "character"), function(conn, statement, ...) {
  .dbi_not_implemented("dbGetQuery")
})
setMethod("dbExecute", c("OdbcRsConnection", "character"), function(conn, statement, ...) {
  .dbi_not_implemented("dbExecute")
})
setMethod("dbBind", "OdbcRsResult", function(res, params, ...) {
  .dbi_not_implemented("dbBind")
})
