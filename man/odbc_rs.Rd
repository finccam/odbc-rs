\name{odbc_rs}
\alias{odbc_rs}
\title{Create a Rust-backed ODBC driver object}
\usage{odbc_rs()}
\value{An \code{OdbcRsDriver} object.}
\description{Constructs a DBI driver object without opening a connection.
Connection lifecycle and result access methods are available. Public execution,
table, quoting, and transaction methods remain scaffolded.}
