\name{OdbcRsDriver-class}
\alias{OdbcRsDriver-class}
\alias{OdbcRsConnection-class}
\alias{OdbcRsResult-class}
\title{DBI objects for the Rust-backed ODBC driver}
\description{S4 subclasses of \code{DBIDriver}, \code{DBIConnection}, and
\code{DBIResult}. Connections and results contain a private
\code{ptr} external-pointer slot for the native resource layer. An object
constructed directly has an empty pointer and no database resources.}
\details{Use \code{odbc_rs()} to construct a driver and \code{DBI::dbConnect()}
to open a connection. Connection lifecycle, information, validity, display,
and result access methods are implemented. Public execution, binding, quoting,
type declaration, table, and transaction methods remain scaffolded. Result
objects are constructed by a private preparation helper until execution is wired.
Native resources must not be serialized or reused in another process.}
\seealso{\code{\link{odbc_rs}}, \code{\link{OdbcRs-connection}},
\code{\link{OdbcRs-result}}}
