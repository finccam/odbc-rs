# DBI connection lifecycle scaffold.
setMethod("dbConnect", "OdbcRsDriver", function(drv, ...) {
  .dbi_not_implemented("dbConnect")
})

setMethod("dbDisconnect", "OdbcRsConnection", function(conn, ...) {
  .dbi_not_implemented("dbDisconnect")
})

setMethod("dbIsValid", "OdbcRsDriver", function(dbObj, ...) {
  .dbi_not_implemented("dbIsValid")
})
setMethod("dbIsValid", "OdbcRsConnection", function(dbObj, ...) {
  .dbi_not_implemented("dbIsValid")
})
setMethod("dbIsValid", "OdbcRsResult", function(dbObj, ...) {
  .dbi_not_implemented("dbIsValid")
})

setMethod("dbGetInfo", "OdbcRsDriver", function(dbObj, ...) {
  .dbi_not_implemented("dbGetInfo")
})
setMethod("dbGetInfo", "OdbcRsConnection", function(dbObj, ...) {
  .dbi_not_implemented("dbGetInfo")
})
setMethod("dbGetInfo", "OdbcRsResult", function(dbObj, ...) {
  .dbi_not_implemented("dbGetInfo")
})

setMethod("show", "OdbcRsDriver", function(object) {
  .dbi_not_implemented("show")
})
setMethod("show", "OdbcRsConnection", function(object) {
  .dbi_not_implemented("show")
})
setMethod("show", "OdbcRsResult", function(object) {
  .dbi_not_implemented("show")
})
