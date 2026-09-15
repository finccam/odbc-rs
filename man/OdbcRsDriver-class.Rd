\name{OdbcRsDriver-class}
\alias{OdbcRsDriver-class}
\alias{OdbcRsConnection-class}
\alias{OdbcRsResult-class}
\title{DBI objects for the Rust-backed ODBC driver}
\description{S4 subclasses of \code{DBIDriver}, \code{DBIConnection}, and
\code{DBIResult}. Connections and results contain a private
\code{ptr} external-pointer slot for the native resource layer. Connections also
retain their server timezone for literal formatting. An object
constructed directly has an empty pointer and no database resources.}
\details{Use \code{odbc_rs()} to construct a driver and \code{DBI::dbConnect()}
to open a connection. Connection lifecycle, information, validity, display,
and result access methods are implemented. SQL Server quoting and type declarations
and scalar execution are available. Results are created by the public send
methods. Batch binding, table methods, and transaction methods are not implemented.
Native resources must not be serialized or reused in another process.}
\seealso{\code{\link{odbc_rs}}, \code{\link{OdbcRs-connection}},
\code{\link{OdbcRs-result}}, \code{\link{OdbcRs-sql}}, \code{\link{OdbcRs-execution}}}
