test_that("native array failures report partial success without hiding it", {
  check_batch_outcome <- function() {
    con <- DBI::dbConnect(odbc_rs(),
      driver = "ODBC Driver 18 for SQL Server", server = "tcp:127.0.0.1,1433",
      database = "dbitest", uid = "sa", pwd = "OdbcRs-TestFixture1!",
      Encrypt = "yes", TrustServerCertificate = "yes"
    )
    on.exit(DBI::dbDisconnect(con), add = TRUE)
    DBI::dbExecute(con, "CREATE TABLE #batch_outcomes (id INT PRIMARY KEY)")
    error <- tryCatch(
      DBI::dbExecute(con, "INSERT INTO #batch_outcomes VALUES (?)", params = list(c(1L, 1L, 2L))),
      error = identity
    )
    expect_s3_class(error, "odbc_rs_batch_error")
    expect_equal(error$batch_size, 3)
    expect_true(error$batch_outcome_uncertain)
    rows <- DBI::dbGetQuery(con, "SELECT id FROM #batch_outcomes ORDER BY id")
    expect_equal(error$batch_succeeded, nrow(rows))
    expect_true(error$batch_processed >= error$batch_succeeded)
    expect_true(DBI::dbIsValid(con))
  }
  check_batch_outcome()
})
