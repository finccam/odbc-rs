.table_bool <- function(x, name) {
  if (!is.logical(x) || length(x) != 1L || is.na(x)) .dbi_argument_error(paste0(name, " must be TRUE or FALSE"))
}

.table_row_names <- function(x) {
  if (is.null(x)) return(FALSE)
  if ((is.logical(x) && length(x) == 1L) || (is.character(x) && length(x) == 1L && !is.na(x) && nzchar(x))) return(x)
  .dbi_argument_error("row.names must be NULL, TRUE, FALSE, NA, or a column name")
}

.table_data <- function(value, row.names) {
  if (!is.data.frame(value)) .dbi_argument_error("value must be a data frame")
  value <- DBI::sqlRownamesToColumn(value, .table_row_names(row.names))
  if (!ncol(value) || anyNA(names(value)) || any(!nzchar(names(value))) || anyDuplicated(names(value))) {
    .dbi_argument_error("Data must have nonempty, unique column names")
  }
  if (any(vapply(value, function(x) !is.null(dim(x)), logical(1)))) .dbi_argument_error("Matrix/array columns are not supported")
  value
}

.table_fields <- function(conn, fields, row.names, field.types) {
  if (is.data.frame(fields)) fields <- dbDataType(conn, .table_data(fields, row.names)) else {
    row.names <- .table_row_names(row.names)
    if (isTRUE(row.names) || is.character(row.names)) {
      column <- if (isTRUE(row.names)) "row_names" else row.names
      fields <- c(structure("varchar(255)", names = column), fields)
    }
  }
  valid <- function(x) is.character(x) && !anyNA(x) && all(nzchar(x)) && !is.null(names(x)) &&
    !anyNA(names(x)) && all(nzchar(names(x))) && !anyDuplicated(names(x))
  if (!length(fields) || !valid(fields)) .dbi_argument_error("fields must be named SQL type declarations or a data frame")
  if (!is.null(field.types)) {
    if (is.character(field.types) && !length(field.types)) return(fields)
    if (!valid(field.types) || !all(names(field.types) %in% names(fields))) .dbi_argument_error("field.types must name existing columns and contain SQL declarations")
    fields[names(field.types)] <- field.types
  }
  fields
}

.table_path <- function(conn, name, resolve = TRUE, temporary = FALSE) {
  .require_sql_server(conn)
  if (is(name, "SQL")) {
    if (length(name) != 1L || anyNA(name)) .dbi_argument_error("Expected one table identifier")
    parts <- dbUnquoteIdentifier(conn, name)[[1L]]@name
  } else if (is(name, "Id")) parts <- name@name else parts <- .connection_scalar(name, "name")
  if (!length(parts) || length(parts) > 3L || anyNA(parts) || any(!nzchar(parts))) .dbi_argument_error("Expected a table, schema/table, or catalog/schema/table identifier")
  if (!is.null(names(parts)) && any(nzchar(names(parts)))) {
    names(parts)[names(parts) %in% c("database", "db")] <- "catalog"
    if (any(!names(parts) %in% c("catalog", "schema", "table")) || anyDuplicated(names(parts)) || !"table" %in% names(parts)) {
      .dbi_argument_error("Table Id components must be catalog, schema, and table")
    }
    parts <- parts[intersect(c("catalog", "schema", "table"), names(parts))]
  }
  if (!is.null(names(parts)) && "catalog" %in% names(parts) && !"schema" %in% names(parts)) {
    .dbi_argument_error("Catalog-qualified tables must specify a schema")
  }
  label <- unname(parts[[length(parts)]])
  key <- as.character(dbQuoteIdentifier(conn, DBI::Id(unname(parts))))
  if (resolve && exists(key, envir = conn@table_state, inherits = FALSE)) {
    path <- get(key, envir = conn@table_state)
    if (.table_exists_path(conn, path)) return(path)
    rm(list = key, envir = conn@table_state)
  }
  if (temporary && length(parts) != 1L) .dbi_argument_error("Temporary destinations must be unqualified")
  physical <- parts
  if (temporary && !startsWith(label, "#")) physical[[length(physical)]] <- paste0("#", label)
  temp <- startsWith(physical[[length(physical)]], "#")
  sql <- as.character(dbQuoteIdentifier(conn, DBI::Id(unname(physical))))
  object_name <- if (temp) paste0("tempdb..", dbQuoteIdentifier(conn, unname(physical[[length(physical)]]))) else sql
  list(sql = sql, object_name = object_name, key = key, label = label, temporary = temp)
}

.table_exists_path <- function(conn, path) {
  isTRUE(dbGetQuery(conn,
    "SELECT CAST(CASE WHEN OBJECT_ID(?, 'U') IS NOT NULL OR OBJECT_ID(?, 'V') IS NOT NULL THEN 1 ELSE 0 END AS BIT) AS present",
    params = list(path$object_name, path$object_name))$present[[1L]])
}
