# Batch D: DBI-managed transactions

Implemented `dbBegin`, `dbCommit`, `dbRollback`, and `dbWithTransaction` using
`odbc-api`'s connection transaction primitives. Table writing and batch binding
remain transaction-neutral; they participate in the transaction the caller opened.
No additional connection, SQL transaction parser, retry, or savepoint mechanism
was introduced.

## Native state

Each connection tracks Autocommit, Active, Finishing, or Failed. Begin disables
autocommit and marks Active after success. Nested begin and completion without a
DBI transaction fail before changing the connection. State is set pessimistically
around native calls so a panic cannot leave an uncertain operation marked healthy.

Commit/rollback call the native operation first. Only an acknowledged success
permits `set_autocommit(true)`, since enabling autocommit can otherwise commit
pending work. Completion errors mark connection usability uncertain and transaction
state Failed. Explicit rollback is allowed as a cleanup attempt, but does not
automatically restore the health of a previously uncertain connection.

R conditions expose `transaction_outcome`: unknown on failed native completion,
or committed/rolled_back when completion succeeded but autocommit restoration
failed. New queries are blocked on uncertain connections; dispose of them. There
is no automatic replay or switch to another session. A reported serialization/
deadlock failure inside a managed transaction also blocks silent continuation of
what might now be a different server transaction.

The state describes transactions opened with the DBI methods. Mixing raw SQL
BEGIN/COMMIT/ROLLBACK or implicit-transaction settings with these methods is not
supported. Isolation remains the database/driver default. SQL Server permits the
ordinary table DDL used here in transactions; administrative DDL can have separate
engine restrictions.

## Cursor and alias lifecycle

Before completion, the backend obtains all result borrows and closes native cursor
streams explicitly. A partially consumed query becomes invalid rather than
returning buffered rows after rollback. It remains clearable. Exhausted queries,
statement results, and unexecuted preparations retain their statement handles,
metadata, and counts for inspection/rebinding. This is a documented transaction
boundary policy, not a claim that every ODBC driver closes every cursor itself.
Cursor invalidation is not recorded as an execution or a consumption failure.

R temporary-table aliases are snapshotted after a successful begin, preserved on
commit, and restored on rollback. This handles both creating and dropping local
temporary tables transactionally. The snapshot contains only client-side name
bookkeeping; the native connection remains the authority for transaction state.

Explicit disconnect clears results and attempts rollback before releasing an open
transaction. Final release of an abandoned connection attempts rollback without R
callbacks. Live results may retain the connection, so garbage collection timing is
not a substitute for explicit cleanup.

## R composition

`dbWithTransaction` acquires ownership only after begin succeeds. Its exit cleanup
rolls back errors, R interrupts, and nonlocal exits while preserving the original
condition if rollback also fails. `DBI::dbBreak()` exits silently with rollback.
The code is evaluated in the caller's environment; its value and visibility are
preserved. An explicit completion is attempted once, outside the code-error cleanup
path, so an ambiguous commit is not followed by automatic rollback/retry.

This small override of DBI's default wrapper is needed for original-error
preservation, nonlocal-exit cleanup, and commit-failure policy. Native execution
remains blocking; attempting cancellation of an in-flight operation belongs to
the next batch.

## Verification

The unchanged full DBItest baseline was 70 failures, 0 warnings, 80 skips, and
7,757 passed expectations. Additional regression cases exercise temporary-table
alias rollback, prepared-handle reuse, pending-cursor invalidation, result
visibility/caller effects, cleanup-error preservation, and loss of this test's own
SQL Server session during commit. The native autocommit-restoration failure path
has not been fault-injected.

Build and documentation use `rpx run Rscript -e 'devtools::document()'`; the full
suite uses `testthat::set_max_fails(Inf)` and `devtools::test()` through rpx.
No explicit clean operation or test-configuration changes were made.

Final full-suite result: **55 failures, 0 warnings, 80 skips, 7,844 passes**.
The existing DBItest file accounts for 7,817 passes; the batch-outcome regression
adds 6 and the new transaction lifecycle regressions add 21. All new regression
expectations passed, including the killed-session commit check. Relative to the
baseline there are 15 fewer failures and 87 more passing expectations; this is not
yet a full conformance claim.
