\name{OdbcRsDriver-class}
\alias{OdbcRsDriver-class}
\alias{OdbcRsConnection-class}
\alias{OdbcRsResult-class}
\title{DBI objects for the Rust-backed ODBC driver}
\description{S4 subclasses of \code{DBIDriver}, \code{DBIConnection}, and
\code{DBIResult}. They currently provide an interface scaffold only, with no
public database behavior. Connections and results contain a private
\code{ptr} external-pointer slot for the native resource layer. An object
constructed directly has an empty pointer and no database resources.}
\details{Connection, execution, binding, result, quoting, type, table,
transaction, information, validity, and display methods are registered as
not-implemented placeholders. Method signatures follow the DBI generics;
additional operation options are accepted through \code{...} but are not
processed. See the corresponding DBI documentation for the intended contracts.
Use \code{odbc_rs()} to construct a driver. Connection and result objects will
be created by database methods once the native backend is implemented.}
\seealso{\code{\link{odbc_rs}}}
