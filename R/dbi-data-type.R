# Default declarations follow odbc's generic and SQL Server mappings. These are
# CREATE TABLE type choices, not a claim about lossless fetched-value conversion.
.sql_data_type <- function(obj, sql_server = FALSE) {
  if (is.data.frame(obj)) {
    return(vapply(obj, .sql_data_type, character(1), sql_server = sql_server))
  }
  binary <- is.raw(obj) || inherits(obj, "blob") ||
    (is.list(obj) && all(vapply(obj, function(x) is.null(x) || is.raw(x), logical(1))))
  kind <- if (is.factor(obj)) "character" else if (inherits(obj, "POSIXt")) "datetime" else
    if (inherits(obj, "Date")) "date" else if (inherits(obj, "difftime")) "time" else
    if (inherits(obj, "integer64")) "int64" else if (binary) "binary" else typeof(obj)
  generic <- c(logical = "BIT", integer = "INTEGER", int64 = "INTEGER", double = "DOUBLE PRECISION",
               character = "VARCHAR(255)", list = "VARCHAR(255)", binary = "VARBINARY(255)",
               date = "DATE", datetime = "TIMESTAMP", time = "TIME")
  if (!kind %in% names(generic)) .dbi_argument_error("Unsupported SQL column type")
  if (!sql_server) return(unname(generic[[kind]]))
  switch(kind,
    integer = "INT", int64 = "BIGINT", double = "FLOAT", datetime = "DATETIME",
    character = .sql_server_width(obj, "varchar"),
    list = .sql_server_width(obj, "varchar"),
    binary = .sql_server_width(obj, "varbinary"),
    unname(generic[[kind]])
  )
}

.sql_server_width <- function(obj, type) {
  width <- if (type == "varbinary") {
    if (is.raw(obj)) length(obj) else lengths(obj)
  } else {
    nchar(as.character(obj), type = "chars")
  }
  # Current odbc convention: minimum 255; use MAX beyond 8000.
  width <- max(c(255, width), na.rm = TRUE)
  paste0(type, "(", if (width > 8000) "max" else format(width, scientific = FALSE, trim = TRUE), ")")
}

#' SQL type declarations for R values
#' @rdname OdbcRs-sql
#' @export
setMethod("dbDataType", "OdbcRsDriver", function(dbObj, obj, ...) {
  .dbi_no_dots(...)
  .sql_data_type(obj)
})

#' @rdname OdbcRs-sql
#' @export
setMethod("dbDataType", "OdbcRsConnection", function(dbObj, obj, ...) {
  .dbi_no_dots(...)
  .require_sql_server(dbObj)
  .sql_data_type(obj, sql_server = TRUE)
})
