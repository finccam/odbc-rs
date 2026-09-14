# DBI transaction scaffold.
setMethod("dbBegin", "OdbcRsConnection", function(conn, ...) {
  .dbi_not_implemented("dbBegin")
})
setMethod("dbCommit", "OdbcRsConnection", function(conn, ...) {
  .dbi_not_implemented("dbCommit")
})
setMethod("dbRollback", "OdbcRsConnection", function(conn, ...) {
  .dbi_not_implemented("dbRollback")
})
setMethod("dbWithTransaction", "OdbcRsConnection", function(conn, code, ...) {
  .dbi_not_implemented("dbWithTransaction")
})
