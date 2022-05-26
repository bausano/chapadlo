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
more transactions than clients. While inserting into a map is expensive, the
grouping provided by a map enables fast transaction associations.

The client state is composed of three properties. A vector of tuples
representing deposits, ie. each tuple is a tx id and an amount that was
deposited. We opt for vector here for fast insertions, as it will be obvious in
the next few paragraphs that we don't need to access the deposit transactions
by id and therefore don't fast reads by id. Next property is a single integer
representing withdrawn amount. Third property is a hash map of deposit tx ids
to tx states. A deposit tx state can be "fine", which is represented by that tx
id _not_ being present in the map. Or it can be "disputed", or "charged back".
We only store in the map states for txs of the latter two variants. When we
process resolve tx, we remove "disputed" state from the map.

When we serialize client's state to CSV, we iterate over all deposit txs. For
each tx in the vector we check the hash map of states whether the tx has been
"disputed" or "charged back". Accordingly, we increment available/total/held
integers and in the end print them.

Some edge cases:
* Only deposit tx can be disputed, resolved or charged back. Txs which try to
  change the state of withdrawal txs are ignored.
* Once charged back, a deposit tx cannot go back to disputed or resolved. If a
  sequence of txs that leads to this scenario occurs, we ignore tx so that
  charge back is a final state of any tx.
* If the sum of successful deposit txs is larger than the sum of withdrawal
  txs, we abort.
* If a math overflow is encountered at any point, we abort.
* We are gracious with empty rows, however if we come across a malformed input
  in a row with expected length, we abort.

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
