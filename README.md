A showcase CLI tool for CSV processing.

# Motivation

A common use-case for a pipeline is grouping events to some entity, such as
transactions to clients.

# Specification

A CLI tool which processes a CSV file in which a row represents a payment
transaction performed by a client and outputs a CSV string of client state.

The input is a process argument with file path. The output is piped to stdout.

A transaction is defined by _(i)_ an enumerable string representing type of
transaction; _(ii)_ a client ID as a 16-bit integer which is the grouping
index; _(iii)_ a transaction ID as a 32-bit integer; _(iv)_ an amount in a
common currency presented by a decimal number with precision up to 4 decimal
places.

A client is defined by _(i)_ its ID; _(ii)_ an amount of available funds;
_(iii)_ an amount of held funds; _(iv)_ an amount of total funds; _(v)_ a flag
whether the client's account is frozen.

The transaction type can be as follows:

* `deposit` increases client's available funds.

* `withdrawal` decreases client's available funds.

* `dispute` marks a transaction as _potentially_ erroneous and moves the amount
  to client's held funds. A disputed transaction _must_ be of type `deposit`.

* `resolve` marks a transaction as valid again, ie. reverses `dispute`. A
  resolved transaction _must_ be of type `deposit`.

* `chargeback` marks a transaction as _definitely_ erroneous and subtracts the
  amount from client's held funds. It also marks client's account as frozen.


# Implemented solution
The [`csv` crate][csv] buffers a CSV file and we consume its deserialized
output into a map of client IDs to client state objects. We opt for a hash map
as order is not important, however a BTree map is a viable and simple drop-in
replacement which would give us ordering by client id if needed.

We opt for a map of ids to states because we assume that there will be many
more transactions than clients. While inserting into a does incur expensive
inserts when map needs to be resized, the grouping provided by a map enables
fast transaction associations.

The client state is composed out of several properties:
* a flag `is_frozen` which is set to true upon disputed followed by a charge
  back and causes the engine to ignore any further deposits or withdraws;
* a hash set `disputes` which tracks currently disputed tx ids;
* a hash map `deposits` which associates tx id with deposited amounts;
* an integer `available` which tracks currently available funds;
* an integer `held` which tracks currently disputed funds.

Key points:
* Withdrawals over available amount are skipped.
* Final amount of available funds _can_ be lower than 0 (see test asset 4.)
* Clients hash map memory grows only with deposit txs, 12 bytes per deposit tx.
  The disputes are assumed to be rare and withdrawals don't project into memory
  footprint.

Parallelization can be achieved for example by
* spawning a single thread which owns the client's hash map and consumes a
  channel over which producers batch txs;
* rw-locking client state and using concurrent hash map for client states. Then
  an atomic reference counter can be given out to producers who load txs and
  update global state.

Some edge cases (see `Client::process_transaction` for a better understanding):
* Only deposit tx can be disputed, resolved or charged back. Txs which try to
  change the state of withdrawal txs are ignored.
* Once charged back, a deposit tx cannot go back to disputed or resolved. If a
  sequence of txs that leads to this scenario occurs, we ignore tx so that
  charge back is a final state of any tx.
* If a math overflow is encountered at any point, we abort.
* We are gracious with empty rows, however if we come across a malformed input
  in a row with expected length, we abort.
* If we encounter duplicate deposit tx id, we skip it. We don't track
  withdrawals, so duplicate withdrawal tx id will be counted twice.
* Once a client is frozen we ignore all further deposits and withdrawals, but
  disputes are still possible.
* Once charged back, a deposit tx cannot be disputed again.

# Commands
This binary has been tested on a 64bit linux distro with rustc 1.61.

Run the test suite with `./bin/test.sh` or just unit tests with `cargo test`.

A prerequisite for code coverage tool is _rustc 1.61_ and following
dependencies:

```
$ cargo install grcov
$ rustup component add llvm-tools-preview
```

Then run `./bin/codecov.sh` and see the `target/debug/coverage/index.html`.

<!-- List of References -->
[csv]: https://crates.io/crates/csv
