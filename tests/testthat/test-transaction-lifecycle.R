with_transaction_connection <- function(code) {
  con <- DBI::dbConnect(odbc_rs(),
    driver = "ODBC Driver 18 for SQL Server", server = "tcp:127.0.0.1,1433",
    database = "dbitest", uid = "sa", pwd = "OdbcRs-TestFixture1!",
    Encrypt = "yes", TrustServerCertificate = "yes"
  )
  on.exit(try(suppressWarnings(DBI::dbDisconnect(con)), silent = TRUE), add = TRUE)
  code(con)
}

test_that("rollback restores temporary-table aliases after transactional DDL", {
  with_transaction_connection(function(con) {
    DBI::dbWriteTable(con, "before_tx", data.frame(id = 1L), temporary = TRUE)
    DBI::dbBegin(con)
    DBI::dbRemoveTable(con, "before_tx")
    DBI::dbWriteTable(con, "inside_tx", data.frame(id = 2:3), temporary = TRUE)
    DBI::dbRollback(con)
    expect_true(DBI::dbExistsTable(con, "before_tx"))
    expect_equal(DBI::dbReadTable(con, "before_tx")$id, 1L)
    expect_false(DBI::dbExistsTable(con, "inside_tx"))
    expect_identical(DBI::dbGetInfo(con)$transaction.state, "Autocommit")
  })
})

test_that("transaction completion preserves exhausted preparations, not pending rows", {
  with_transaction_connection(function(con) {
    res <- DBI::dbSendQuery(con, "SELECT CAST(? AS INT) AS id")
    DBI::dbBegin(con)
    DBI::dbBind(res, list(42L))
    expect_equal(DBI::dbFetch(res)$id, 42L)
    DBI::dbCommit(con)
    expect_true(DBI::dbIsValid(res))
    DBI::dbBind(res, list(43L))
    expect_equal(DBI::dbFetch(res)$id, 43L)
    DBI::dbClearResult(res)

    DBI::dbBegin(con)
    res <- DBI::dbSendQuery(con, "SELECT 1 AS id UNION ALL SELECT 2 AS id")
    expect_equal(nrow(DBI::dbFetch(res, 1)), 1L)
    DBI::dbRollback(con)
    expect_false(DBI::dbIsValid(res))
    expect_error(DBI::dbFetch(res), "invalidated")
    expect_true(DBI::dbClearResult(res))
  })
})

test_that("with-transaction preserves caller effects and result visibility", {
  with_transaction_connection(function(con) {
    changed <- FALSE
    value <- withVisible(DBI::dbWithTransaction(con, {
      changed <- TRUE
      invisible(7L)
    }))
    expect_true(changed)
    expect_identical(value, list(value = 7L, visible = FALSE))
    DBI::dbBegin(con)
    expect_error(DBI::dbWithTransaction(con, stop("must not execute")), "already active")
    expect_identical(DBI::dbGetInfo(con)$transaction.state, "Active")
    DBI::dbRollback(con)
  })
})

test_that("cleanup rollback failure cannot replace the original R error", {
  with_transaction_connection(function(con) {
    original <- simpleError("original failure")
    error <- tryCatch(DBI::dbWithTransaction(con, {
      DBI::dbDisconnect(con)
      stop(original)
    }), error = identity)
    expect_identical(error, original)
  })
})

test_that("a lost session during commit blocks further work without replay", {
  with_transaction_connection(function(con) {
    session <- DBI::dbGetQuery(con, "SELECT @@SPID AS session_id")$session_id[[1L]]
    DBI::dbExecute(con, "CREATE TABLE #lost_commit (id INT)")
    DBI::dbBegin(con)
    DBI::dbExecute(con, "INSERT INTO #lost_commit VALUES (1)")
    # The fixture's SA login kills only this test's own session.
    with_transaction_connection(function(peer) {
      DBI::dbExecute(peer, sprintf("KILL %d", as.integer(session)))
    })
    error <- tryCatch(DBI::dbCommit(con), error = identity)
    expect_s3_class(error, "odbc_rs_error")
    expect_identical(error$transaction_outcome, "unknown")
    expect_true(error$uncertain)
    expect_false(DBI::dbIsValid(con))
    expect_error(DBI::dbGetQuery(con, "SELECT 1"), "closed or uncertain")
  })
})
