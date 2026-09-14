# Connection-string construction is separate from object state. Only the final
# string is passed to Rust; it is never retained on the R connection object.
.dbi_argument_error <- function(message) {
  stop(structure(
    list(message = message, call = NULL, operation = "arguments", kind = "parameter"),
    class = c("odbc_rs_parameter_error", "odbc_rs_error", "error", "condition")
  ))
}

.dbi_no_dots <- function(...) {
  if (length(list(...))) .dbi_argument_error("Unused arguments in ...")
}

.connection_scalar <- function(value, label, numeric = FALSE) {
  if (!(is.character(value) || (numeric && (is.numeric(value) || is.logical(value)))) ||
      length(value) != 1L || is.na(value)) {
    .dbi_argument_error(paste0(label, " must be a single nonmissing value"))
  }
  if (is.numeric(value) && !is.finite(value)) {
    .dbi_argument_error(paste0(label, " must be finite"))
  }
  enc2utf8(as.character(value))
}

# Read only attribute names from an already rendered string. Braced or quoted
# values can contain semicolons; doubled delimiters escape the delimiter itself.
# Preserve the supplied text instead of parsing and re-rendering its values.
.connection_string_names <- function(text) {
  chars <- strsplit(text, "", fixed = TRUE)[[1L]]
  n <- length(chars)
  pos <- 1L
  keys <- character()
  malformed <- function() .dbi_argument_error("Malformed .connection_string")
  while (pos <= n) {
    while (pos <= n && (chars[[pos]] == ";" || grepl("[[:space:]]", chars[[pos]]))) pos <- pos + 1L
    if (pos > n) break
    start <- pos
    while (pos <= n && !chars[[pos]] %in% c("=", ";")) pos <- pos + 1L
    if (pos > n || chars[[pos]] != "=") malformed()
    if (pos == start) malformed()
    key <- trimws(paste0(chars[start:(pos - 1L)], collapse = ""))
    if (!nzchar(key) || grepl("[={}\r\n]", key)) malformed()
    keys <- c(keys, tolower(key))
    pos <- pos + 1L
    while (pos <= n && grepl("[[:space:]]", chars[[pos]])) pos <- pos + 1L
    if (pos <= n && chars[[pos]] %in% c("{", "'", '"')) {
      end <- if (chars[[pos]] == "{") "}" else chars[[pos]]
      pos <- pos + 1L
      closed <- FALSE
      while (pos <= n) {
        if (chars[[pos]] == end) {
          if (pos < n && chars[[pos + 1L]] == end) {
            pos <- pos + 2L
            next
          }
          closed <- TRUE
          pos <- pos + 1L
          break
        }
        pos <- pos + 1L
      }
      if (!closed) malformed()
      while (pos <= n && grepl("[[:space:]]", chars[[pos]])) pos <- pos + 1L
      if (pos <= n && chars[[pos]] != ";") malformed()
    } else {
      while (pos <= n && chars[[pos]] != ";") pos <- pos + 1L
    }
    pos <- pos + 1L
  }
  keys
}

.build_connection_string <- function(base, attributes) {
  if (is.null(base)) base <- ""
  base <- .connection_scalar(base, ".connection_string")
  keys <- names(attributes)
  if (length(attributes) && (is.null(keys) || anyNA(keys) ||
      any(!nzchar(trimws(keys))) || any(grepl("[=;{}\r\n]", keys)))) {
    .dbi_argument_error("All ODBC attributes must have valid, nonempty names")
  }
  keys <- trimws(keys)
  all_keys <- c(.connection_string_names(base), tolower(keys))
  if (anyDuplicated(all_keys)) {
    .dbi_argument_error("Duplicate ODBC attributes (names are case-insensitive)")
  }
  values <- lapply(attributes, .connection_scalar, label = "ODBC attribute", numeric = TRUE)
  # Named values are ordinary data, not pre-rendered connection-string fragments.
  values <- vapply(values, function(x) paste0("{", gsub("}", "}}", x, fixed = TRUE), "}"), character(1))
  rendered <- paste0(keys, "=", values, ";", collapse = "")
  if (!length(attributes)) rendered <- ""
  if (nzchar(base) && nzchar(rendered) && !endsWith(trimws(base), ";")) base <- paste0(base, ";")
  out <- paste0(base, rendered)
  if (!length(all_keys)) .dbi_argument_error("Supply a DSN, connection string, or named ODBC connection attributes")
  out
}

.connection_timeout <- function(timeout) {
  if (length(timeout) != 1L || is.object(timeout) ||
      !(typeof(timeout) %in% c("integer", "double") || identical(timeout, NA))) {
    .dbi_argument_error("timeout must be a single number or NA")
  }
  if (is.nan(timeout)) .dbi_argument_error("timeout must not be NaN")
  if (is.na(timeout) || identical(timeout, Inf)) return(0)
  if (!is.finite(timeout) || timeout < 0 || timeout != floor(timeout) || timeout > 2^32 - 1) {
    .dbi_argument_error("timeout must be nonnegative whole seconds, Inf, or NA")
  }
  as.double(timeout)
}
