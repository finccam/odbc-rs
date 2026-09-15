#' SQL Server table access and management
#'
#' Table operations reuse quoting, execution, and native parameter-array binding.
#' @param conn An open OdbcRsConnection.
#' @param name A table name, DBI::Id, or rendered SQL identifier. Character names
#'   containing dots are literal names; use Id for qualification.
#' @param value A data frame to insert.
#' @param fields A named character vector of SQL types or a data frame.
#' @param row.names Row-name handling passed to DBI's row-name helpers. Appending
#'   directly requires NULL; dbWriteTable handles row names before appending.
#' @param temporary Create a session-local temporary table. Unqualified names
#'   acquire a # prefix internally and remain usable by their original name in
#'   this connection's table methods. Raw SQL must use the physical # name.
#' @param overwrite Explicitly authorize replacing an existing destination.
#' @param append Append to an existing table, or create it if absent.
#' @param check.names Apply make.names() to returned column names when reading.
#' @param field.types Named SQL type overrides for data-frame columns.
#' @param catalog_name,schema_name,table_name Optional exact discovery filters.
#' @param ... Unused arguments.
#' @return Reading returns a data frame; listing returns character vectors;
#'   existence returns a logical. Appending returns an affected count. Other
#'   methods return TRUE invisibly.
#' @details
#' Multi-step writes do not open hidden transactions. On failure, a newly created
#' table or partial inserts may remain. Overwrite can remove the old table before
#' a later step fails. Input conversion is validated before destructive writes.
#' Cleanup never drops a pre-existing/unrelated object to conceal a failed write.
#' Native batch errors expose known processed/successful parameter counts and
#' outcome uncertainty; successful parameters are not a durability guarantee.
#'
#' Discovery uses ODBC SQLTables for regular tables/views. Session-local tables
#' created through these methods are tracked per connection. Raw-SQL-created
#' temporary tables can be accessed by #name but are not added to that registry.
#' Existence checks use SQL Server OBJECT_ID on the same session; field discovery
#' uses a zero-row SELECT. These real SQL operations pass through normal execution.
#' Catalog-qualified Id names must also specify a schema. Identifier/type overrides
#' are explicit SQL declarations, not arbitrary value interpolation.
#' @rdname OdbcRs-tables
#' @export
setMethod("dbCreateTable", "OdbcRsConnection", function(conn, name, fields, field.types = NULL, ..., row.names = NULL, temporary = FALSE) {
  .dbi_no_dots(...)
  .table_bool(temporary, "temporary")
  path <- .table_path(conn, name, resolve = FALSE, temporary = temporary)
  fields <- .table_fields(conn, fields, row.names, field.types)
  if (.table_exists_path(conn, path)) .dbi_argument_error("Destination already exists")
  sql <- DBI::sqlCreateTable(conn, DBI::SQL(path$sql), fields, row.names = FALSE, temporary = FALSE)
  dbExecute(conn, sql, immediate = TRUE)
  if (path$temporary) assign(path$key, path, envir = conn@table_state)
  invisible(TRUE)
})

#' @rdname OdbcRs-tables
#' @export
setMethod("dbAppendTable", "OdbcRsConnection", function(conn, name, value, ..., row.names = NULL) {
  .dbi_no_dots(...)
  if (!is.null(row.names)) .dbi_argument_error("dbAppendTable requires row.names = NULL")
  value <- .table_data(value, FALSE)
  path <- .table_path(conn, name)
  if (!.table_exists_path(conn, path)) .dbi_argument_error("Destination does not exist")
  columns <- dbListFields(conn, DBI::SQL(path$sql))
  if (!all(names(value) %in% columns)) .dbi_argument_error("Input contains columns absent from the destination")
  if (!nrow(value)) {
    .native_validate_parameters(conn@ptr, value)
    return(0L)
  }
  sql <- paste0("INSERT INTO ", path$sql, " (", paste(dbQuoteIdentifier(conn, names(value)), collapse = ", "),
                ") VALUES (", paste(rep("?", ncol(value)), collapse = ", "), ")")
  dbExecute(conn, sql, params = value, immediate = FALSE)
})

#' @rdname OdbcRs-tables
#' @export
setMethod("dbWriteTable", "OdbcRsConnection", function(conn, name, value, ..., row.names = FALSE, overwrite = FALSE, append = FALSE, temporary = FALSE, field.types = NULL) {
  .dbi_no_dots(...)
  .table_bool(overwrite, "overwrite"); .table_bool(append, "append"); .table_bool(temporary, "temporary")
  if (overwrite && append) .dbi_argument_error("overwrite and append cannot both be TRUE")
  if (append && !is.null(field.types)) .dbi_argument_error("field.types cannot be supplied when appending")
  value <- .table_data(value, row.names)
  fields <- .table_fields(conn, value, FALSE, field.types)
  path <- .table_path(conn, name, resolve = append, temporary = temporary)
  exists <- .table_exists_path(conn, path)
  if (exists && !overwrite && !append) .dbi_argument_error("Destination exists; use append or overwrite explicitly")
  # Validate the complete input before DROP/CREATE. This native preflight uses
  # the same converters as binding and performs no database execution.
  .native_validate_parameters(conn@ptr, value)
  if (exists && overwrite) dbRemoveTable(conn, DBI::SQL(path$sql))
  if (!exists || overwrite) {
    dbExecute(conn, DBI::sqlCreateTable(conn, DBI::SQL(path$sql), fields, row.names = FALSE, temporary = FALSE), immediate = TRUE)
    if (path$temporary) assign(path$key, path, envir = conn@table_state)
  }
  if (nrow(value)) dbAppendTable(conn, DBI::SQL(path$sql), value)
  invisible(TRUE)
})

#' @rdname OdbcRs-tables
#' @export
setMethod("dbReadTable", "OdbcRsConnection", function(conn, name, ..., row.names = FALSE, check.names = TRUE) {
  .dbi_no_dots(...)
  .table_bool(check.names, "check.names")
  row.names <- .table_row_names(row.names)
  path <- .table_path(conn, name)
  value <- DBI::sqlColumnToRownames(dbGetQuery(conn, paste0("SELECT * FROM ", path$sql)), row.names)
  if (check.names) names(value) <- make.names(names(value), unique = TRUE)
  value
})

#' @rdname OdbcRs-tables
#' @export
setMethod("dbExistsTable", "OdbcRsConnection", function(conn, name, ...) {
  .dbi_no_dots(...)
  .table_exists_path(conn, .table_path(conn, name))
})

#' @rdname OdbcRs-tables
#' @export
setMethod("dbRemoveTable", "OdbcRsConnection", function(conn, name, ..., temporary = FALSE) {
  .dbi_no_dots(...)
  .table_bool(temporary, "temporary")
  path <- .table_path(conn, name)
  if (temporary && !path$temporary) .dbi_argument_error("Requested destination is not a temporary table")
  if (!.table_exists_path(conn, path)) .dbi_argument_error("Destination does not exist")
  dbExecute(conn, paste0("DROP TABLE ", path$sql), immediate = TRUE)
  for (key in ls(conn@table_state, all.names = TRUE)) {
    if (identical(get(key, conn@table_state)$sql, path$sql)) rm(list = key, envir = conn@table_state)
  }
  invisible(TRUE)
})

#' @rdname OdbcRs-tables
#' @export
setMethod("dbListTables", "OdbcRsConnection", function(conn, ..., catalog_name = NULL, schema_name = NULL, table_name = NULL) {
  .dbi_no_dots(...)
  .require_sql_server(conn)
  pattern <- function(x, default) {
    if (is.null(x)) return(default)
    x <- .connection_scalar(x, "metadata filter")
    for (char in c("\\", "_", "%")) x <- gsub(char, paste0("\\", char), x, fixed = TRUE)
    x
  }
  entries <- .native_tables(conn@ptr, pattern(catalog_name, ""), pattern(schema_name, "%"), pattern(table_name, "%"))
  if (is.null(schema_name)) entries <- Filter(function(x) !x$schema %in% c("sys", "INFORMATION_SCHEMA"), entries)
  names <- vapply(entries, function(x) x$name, character(1))
  if (is.null(catalog_name) && is.null(schema_name)) {
    for (key in ls(conn@table_state, all.names = TRUE)) {
      path <- get(key, conn@table_state)
      if (.table_exists_path(conn, path)) {
        if (is.null(table_name) || identical(table_name, path$label)) names <- c(names, path$label)
      } else rm(list = key, envir = conn@table_state)
    }
  }
  unique(names)
})

#' @rdname OdbcRs-tables
#' @export
setMethod("dbListFields", "OdbcRsConnection", function(conn, name, ...) {
  .dbi_no_dots(...)
  path <- .table_path(conn, name)
  names(dbGetQuery(conn, paste0("SELECT TOP (0) * FROM ", path$sql), n = 0))
})

# Explicit signatures avoid ambiguous dispatch against DBI's Id/character
# defaults; all name forms still share the implementation above.
for (.method in c("dbReadTable", "dbWriteTable", "dbExistsTable", "dbRemoveTable", "dbListFields")) {
  for (.name_class in c("character", "Id", "SQL")) {
    setMethod(.method, c("OdbcRsConnection", .name_class), getMethod(.method, "OdbcRsConnection"))
  }
}
rm(.method, .name_class)
