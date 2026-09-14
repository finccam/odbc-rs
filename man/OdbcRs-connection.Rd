\name{OdbcRs-connection}
\alias{OdbcRs-connection}
\alias{dbConnect,OdbcRsDriver-method}
\alias{dbDisconnect,OdbcRsConnection-method}
\alias{dbIsValid,OdbcRsDriver-method}
\alias{dbIsValid,OdbcRsConnection-method}
\alias{dbIsValid,OdbcRsResult-method}
\alias{dbGetInfo,OdbcRsDriver-method}
\alias{dbGetInfo,OdbcRsConnection-method}
\alias{dbGetInfo,OdbcRsResult-method}
\alias{show,OdbcRsDriver-method}
\alias{show,OdbcRsConnection-method}
\alias{show,OdbcRsResult-method}
\title{Connect and inspect Rust-backed ODBC objects}
\description{Open a native ODBC connection, inspect its state, and release it.
Public query execution remains in the next implementation batch.}
\usage{
\S4method{dbConnect}{OdbcRsDriver}(drv, dsn = NULL, ...,
  timezone = "UTC", timezone_out = "UTC",
  bigint = c("integer64", "integer", "numeric", "character"),
  timeout = 10, driver = NULL, server = NULL, database = NULL,
  uid = NULL, pwd = NULL, .connection_string = NULL,
  encoding = "", name_encoding = "", dbms.name = NULL,
  attributes = NULL, interruptible = FALSE)
\S4method{dbDisconnect}{OdbcRsConnection}(conn, ...)
\S4method{dbIsValid}{OdbcRsDriver}(dbObj, ...)
\S4method{dbIsValid}{OdbcRsConnection}(dbObj, ...)
\S4method{dbIsValid}{OdbcRsResult}(dbObj, ...)
\S4method{dbGetInfo}{OdbcRsDriver}(dbObj, ...)
\S4method{dbGetInfo}{OdbcRsConnection}(dbObj, ...)
\S4method{dbGetInfo}{OdbcRsResult}(dbObj, ...)
\S4method{show}{OdbcRsDriver}(object)
\S4method{show}{OdbcRsConnection}(object)
\S4method{show}{OdbcRsResult}(object)
}
\arguments{
\item{drv}{An \code{odbc_rs()} driver.}
\item{dsn}{A configured ODBC data source name, or \code{NULL}.}
\item{...}{For connecting, named vendor ODBC connection attributes. Other
methods reject unused arguments.}
\item{timezone}{Server timezone interpretation for timezone-free timestamps.}
\item{timezone_out}{Timezone attached to returned POSIXct columns.}
\item{bigint}{R representation of SQL BIGINT values.}
\item{timeout}{Nonnegative whole login-timeout seconds. Zero, \code{Inf}, or
\code{NA} requests no login timeout. This is not a query timeout.}
\item{driver, server, database, uid, pwd}{Common ODBC connection attributes.
\code{NULL} values are omitted. Attribute names are driver-dependent.}
\item{.connection_string}{An already rendered ODBC connection string.
Additional named attributes are appended.}
\item{encoding, name_encoding}{Reserved options. Nonempty overrides currently
raise an error; fetched text uses ODBC Unicode conversion.}
\item{dbms.name, attributes}{Reserved options. Non-NULL values currently raise
an error.}
\item{interruptible}{Reserved option. Must currently be \code{FALSE}.}
\item{conn}{An OdbcRsConnection.}
\item{dbObj, object}{An OdbcRsDriver, OdbcRsConnection, or OdbcRsResult.}
}
\value{Connecting returns an OdbcRsConnection. Disconnecting returns
\code{TRUE} invisibly. Validity methods return a scalar logical; information
methods return named lists. Display returns the object invisibly.}
\details{
Named attribute values are brace-quoted, with closing braces escaped. Attribute
names, including those in the supplied connection string, must be unique ignoring
case. Named values are ordinary values, not pre-quoted fragments. Use
\code{.connection_string} for already rendered vendor-specific connection syntax.
The parser recognizes braced and single/double-quoted values with doubled
delimiter escapes; malformed strings fail before connecting.

The connection retains one physical session. Explicit disconnection clears its
active results with a warning. Dropping only the R connection variable does not
break results that still own the session. An R finalizer issues a best-effort
warning on abandonment; native finalization releases remaining ownership.
Repeated disconnection warns and returns invisibly. Cleanup failures raise a
condition and mark usability uncertain. No automatic reconnect or query retry
is performed.

Connection validity uses the ODBC connection-dead attribute where implemented,
falling back to local lifecycle state for unsupported attributes. It never
executes SQL and cannot guarantee the next operation will succeed. Empty, stale,
or foreign-process pointers are invalid. Results remain valid when exhausted,
until cleared or invalidated. Result validity checks inspect local state only.

Connection information obtains DBMS name and current catalog through native APIs.
The current high-level Rust dependency lacks connected-driver/server-version,
username, and server-name getters; these fields, and unknown port values, are
\code{NA}. Driver-level client version is also \code{NA} because the vendor client
is connection-specific. No credential or connection-string fields are returned.
Results report statement, row.count, rows.affected, and has.completed. Information
on cleared results or invalid connections raises an error.

Display uses local state only and never prints unrestricted SQL, credentials,
connection strings, native pointers, or server-provided text.
}
\seealso{\code{\link{odbc_rs}}, \code{\link{OdbcRs-result}}}
