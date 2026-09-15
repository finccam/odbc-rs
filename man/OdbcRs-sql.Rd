\name{OdbcRs-sql}
\alias{OdbcRs-sql}
\alias{dbQuoteIdentifier,OdbcRsConnection,character-method}
\alias{dbQuoteIdentifier,OdbcRsConnection,Id-method}
\alias{dbQuoteIdentifier,OdbcRsConnection,SQL-method}
\alias{dbQuoteIdentifier,OdbcRsConnection,ANY-method}
\alias{dbUnquoteIdentifier,OdbcRsConnection,ANY-method}
\alias{dbQuoteString,OdbcRsConnection,character-method}
\alias{dbQuoteString,OdbcRsConnection,SQL-method}
\alias{dbQuoteString,OdbcRsConnection,ANY-method}
\alias{dbQuoteLiteral,OdbcRsConnection,ANY-method}
\alias{dbDataType,OdbcRsDriver,ANY-method}
\alias{dbDataType,OdbcRsConnection,ANY-method}
\title{SQL Server quoting and SQL type declarations}
\description{Render identifiers, values, and type declarations for Microsoft SQL
Server. These operations format SQL; they do not execute it.}
\usage{
\S4method{dbQuoteIdentifier}{OdbcRsConnection,character}(conn, x, ...)
\S4method{dbQuoteIdentifier}{OdbcRsConnection,Id}(conn, x, ...)
\S4method{dbQuoteIdentifier}{OdbcRsConnection,SQL}(conn, x, ...)
\S4method{dbUnquoteIdentifier}{OdbcRsConnection,ANY}(conn, x, ...)
\S4method{dbQuoteString}{OdbcRsConnection,character}(conn, x, ...)
\S4method{dbQuoteString}{OdbcRsConnection,SQL}(conn, x, ...)
\S4method{dbQuoteLiteral}{OdbcRsConnection,ANY}(conn, x, ...)
\S4method{dbDataType}{OdbcRsDriver,ANY}(dbObj, obj, ...)
\S4method{dbDataType}{OdbcRsConnection,ANY}(dbObj, obj, ...)
}
\arguments{
\item{conn}{An open OdbcRsConnection reporting Microsoft SQL Server as its DBMS.}
\item{dbObj}{An OdbcRsDriver or OdbcRsConnection.}
\item{x}{Identifiers or values to quote. Unquoting accepts character, SQL, or Id.}
\item{obj}{An R vector, binary list, or data frame whose SQL type is required.}
\item{...}{Unused arguments; supplying any raises an error.}
}
\value{Quoting returns DBI SQL objects, preserving vector names. Unquoting returns
a named list of DBI Id objects. Type declarations are character scalars, or named
character vectors for data frames.}
\details{
Character identifiers are literal names: dots inside them are not separators.
Id components are quoted separately and joined using DBI's component ordering.
SQL Server brackets delimit names, and closing brackets are doubled. Missing
identifiers are rejected. Unquoting recognizes brackets, double quotes, escaped
delimiters, and qualification dots outside delimiters. It is an identifier parser,
not a parser for general SQL expressions or omitted qualification components.
Rendered SQL passes through unchanged, and unquoting an Id returns a list
containing that object.

Strings use Unicode N-prefixed SQL literals, with apostrophes doubled. Missing
values become SQL NULL. Empty vectors produce empty SQL vectors. Literal quoting
also supports logical BIT casts, integer and floating-point values, integer64,
factor labels, Date, POSIXt, difftime, and raw/binary data. A raw vector represents
one binary value; a list represents multiple binary values with NULL entries.
Empty binary values use 0x. Non-binary list entries are rejected.

Double literals use 17 significant digits and a decimal point. NA and NaN become
NULL; infinite values are rejected. Datetime literals use DATETIME2(7) in the
server timezone selected at connection construction, with fractional seconds
rounded to 100 ns. TIME(7) literals must fit within one day. These literal casts
are separate from the default column declarations.

Connection type declarations follow the current odbc SQL Server conventions:
logical BIT, integer INT, integer64 BIGINT, double FLOAT, Date DATE, datetime
DATETIME, and time TIME. Character/factor/list declarations use varchar, and
binary declarations use varbinary. Width is at least 255 and becomes max beyond
8000; text width is based on character count, as in odbc. VARCHAR storage depends
on the database collation and is not a universal lossless Unicode declaration.
DATETIME has SQL Server's narrower range and precision; callers needing different
storage will be able to use explicit table field types when table methods land.

The driver object has no selected DBMS. Its generic declarations follow odbc's
defaults: INTEGER (including integer64), DOUBLE PRECISION, VARCHAR(255),
VARBINARY(255), DATE, TIMESTAMP, TIME, and BIT. Prefer connection-specific types
for actual table creation. These are SQL declarations, not native binding or
fetching representations.

Connection-specific rendering requires Microsoft SQL Server, determined via
native connection metadata without executing a query. Other DBMSs raise an
explicit unsupported-rendering error. SQL identity and Id pass-through do not
require dialect discovery. The connection's private timezone slot mirrors its
validated native timezone setting and must not be modified by applications.
}
\seealso{\code{\link{OdbcRs-connection}}, \code{\link{OdbcRs-result}}}
