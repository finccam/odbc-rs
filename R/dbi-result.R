# DBI result lifecycle and metadata scaffold.
setMethod("dbFetch", "OdbcRsResult", function(res, n = -1, ...) {
  .dbi_not_implemented("dbFetch")
})
setMethod("dbClearResult", "OdbcRsResult", function(res, ...) {
  .dbi_not_implemented("dbClearResult")
})
setMethod("dbHasCompleted", "OdbcRsResult", function(res, ...) {
  .dbi_not_implemented("dbHasCompleted")
})
setMethod("dbGetRowCount", "OdbcRsResult", function(res, ...) {
  .dbi_not_implemented("dbGetRowCount")
})
setMethod("dbGetRowsAffected", "OdbcRsResult", function(res, ...) {
  .dbi_not_implemented("dbGetRowsAffected")
})
setMethod("dbColumnInfo", "OdbcRsResult", function(res, ...) {
  .dbi_not_implemented("dbColumnInfo")
})
setMethod("dbGetStatement", "OdbcRsResult", function(res, ...) {
  .dbi_not_implemented("dbGetStatement")
})
