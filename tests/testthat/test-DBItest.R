# Local disposable SQL Server fixture from compose.yaml. The ODBC driver must
# also be installed on the host running R. These credentials are test-only.
ctx <- DBItest::make_context(
  methods::new(
    "DBIConnector",
    .drv = odbc.rs::odbc_rs(),
    .conn_args = list(
      driver = "ODBC Driver 18 for SQL Server",
      server = "tcp:127.0.0.1,1433",
      database = "dbitest",
      uid = "sa",
      pwd = "OdbcRs-TestFixture1!",
      Encrypt = "yes",
      TrustServerCertificate = "yes"
    )
  ),
  tweaks = DBItest::tweaks(
    constructor_name = "odbc_rs",
    placeholder_pattern = "?",
    date_cast = function(x) paste0("CAST('", x, "' AS DATE)"),
    time_cast = function(x) paste0("CAST('", x, "' AS TIME)"),
    timestamp_cast = function(x) paste0("CAST('", x, "' AS DATETIME2)"),
    create_table_as = function(table_name, query = "SELECT 1 AS a") {
      paste0("SELECT * INTO ", table_name, " FROM (", query, ") AS dbitest_source")
    }
  ),
  set_as_default = FALSE
)

DBItest::test_all(ctx = ctx)
