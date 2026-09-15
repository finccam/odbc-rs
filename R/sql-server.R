# SQL rendering is deliberately separate from Rust's bound-value conversion.
# It never executes SQL. Unknown DBMSs must not inherit SQL Server declarations.
.require_sql_server <- function(conn) {
  info <- .native_connection_info(conn@ptr)
  if (!identical(info$dbms_name, "Microsoft SQL Server")) {
    stop("SQL rendering is currently implemented only for Microsoft SQL Server", call. = FALSE)
  }
  invisible(NULL)
}

.sql_server_strings <- function(x) {
  if (!length(x)) return(character())
  out <- paste0("N'", gsub("'", "''", enc2utf8(x), fixed = TRUE), "'")
  out[is.na(x)] <- "NULL"
  out
}

# Parse delimited identifiers, not general SQL. Dots inside delimiters are data;
# doubled ] or " is an escaped delimiter. DBI's symmetric-quote parser cannot
# handle SQL Server's asymmetric brackets.
.identifier_parts <- function(x) {
  chars <- strsplit(enc2utf8(x), "", fixed = TRUE)[[1L]]
  size <- length(chars)
  if (!size) return("")
  pos <- 1L
  parts <- character()
  malformed <- function() .dbi_argument_error("Malformed SQL identifier")
  while (pos <= size) {
    while (pos <= size && grepl("[[:space:]]", chars[[pos]])) pos <- pos + 1L
    if (pos > size) malformed()
    if (chars[[pos]] %in% c("[", '"')) {
      delimiter <- if (chars[[pos]] == "[") "]" else '"'
      pos <- pos + 1L
      value <- character()
      closed <- FALSE
      while (pos <= size) {
        char <- chars[[pos]]
        pos <- pos + 1L
        if (char == delimiter) {
          if (pos <= size && chars[[pos]] == delimiter) {
            pos <- pos + 1L
          } else {
            closed <- TRUE
            break
          }
        }
        value <- c(value, char)
      }
      if (!closed) malformed()
      parts <- c(parts, paste0(value, collapse = ""))
      while (pos <= size && grepl("[[:space:]]", chars[[pos]])) pos <- pos + 1L
      if (pos <= size && chars[[pos]] != ".") malformed()
    } else {
      start <- pos
      while (pos <= size && chars[[pos]] != ".") pos <- pos + 1L
      if (pos == start) malformed()
      value <- trimws(paste0(chars[start:(pos - 1L)], collapse = ""))
      if (!nzchar(value) || grepl('[\\[\\]"]', value, perl = TRUE)) malformed()
      parts <- c(parts, value)
    }
    if (pos == size && chars[[pos]] == ".") malformed()
    pos <- pos + 1L
  }
  parts
}

.typed_literal <- function(text, type) {
  if (!length(text)) return(character())
  out <- paste0("CAST(", .sql_server_strings(text), " AS ", type, ")")
  out[is.na(text)] <- "NULL"
  out
}

.sql_server_literals <- function(x, timezone) {
  if (!length(x) && !is.raw(x)) return(character())
  if (is.factor(x)) return(.sql_server_strings(as.character(x)))
  if (is.character(x)) return(.sql_server_strings(x))
  if (inherits(x, "integer64")) {
    text <- as.character(x)
    text[is.na(x)] <- "NULL"
    return(text)
  }
  if (inherits(x, "POSIXt")) {
    seconds <- as.numeric(as.POSIXct(x))
    if (any(is.infinite(seconds))) .dbi_argument_error("Infinite timestamps cannot be SQL literals")
    # SQL Server datetime2 supports 100 ns precision. Format the integer seconds
    # separately to avoid strftime's platform-dependent fractional-second limit.
    whole <- floor(seconds)
    fraction <- round((seconds - whole) * 1e7)
    carry <- !is.na(fraction) & fraction == 1e7
    whole[carry] <- whole[carry] + 1
    fraction[carry] <- 0
    text <- paste0(format(as.POSIXct(whole, origin = "1970-01-01", tz = timezone),
                         "%Y-%m-%dT%H:%M:%S", tz = timezone), ".", sprintf("%07d", as.integer(fraction)))
    text[is.na(seconds)] <- NA_character_
    return(.typed_literal(text, "DATETIME2(7)"))
  }
  if (inherits(x, "Date")) {
    if (any(is.infinite(as.numeric(x)))) .dbi_argument_error("Infinite dates cannot be SQL literals")
    text <- format(x, "%Y-%m-%d")
    text[is.na(x)] <- NA_character_
    return(.typed_literal(text, "DATE"))
  }
  if (inherits(x, "difftime")) {
    seconds <- as.double(x, units = "secs")
    if (any(!is.na(seconds) & (!is.finite(seconds) | seconds < 0 | seconds >= 86400))) {
      .dbi_argument_error("SQL Server TIME literals must be within one day")
    }
    ticks <- round(seconds * 1e7)
    if (any(ticks >= 86400 * 1e7, na.rm = TRUE)) .dbi_argument_error("Rounded TIME literal exceeds one day")
    whole <- floor(ticks / 1e7)
    text <- sprintf("%02d:%02d:%02d.%07d", as.integer(whole %/% 3600),
                    as.integer(whole %/% 60 %% 60), as.integer(whole %% 60), as.integer(ticks %% 1e7))
    text[is.na(seconds)] <- NA_character_
    return(.typed_literal(text, "TIME(7)"))
  }
  if (is.raw(x)) x <- list(x)
  if (is.list(x)) {
    return(vapply(x, function(value) {
      if (is.null(value)) return("NULL")
      if (!is.raw(value)) .dbi_argument_error("Binary literals must be raw vectors or NULL")
      paste0("0x", paste(format(value), collapse = ""))
    }, character(1)))
  }
  if (is.logical(x)) return(.typed_literal(as.character(as.integer(x)), "BIT"))
  if (is.integer(x) || is.double(x)) {
    if (any(is.infinite(x))) .dbi_argument_error("SQL Server has no infinite numeric literal")
    text <- if (is.integer(x)) as.character(x) else chartr(",", ".", sprintf("%.17g", x))
    # Match DBI's NULL convention for both NA and NaN.
    text[is.na(x)] <- "NULL"
    return(text)
  }
  .dbi_argument_error("Unsupported SQL literal type")
}
