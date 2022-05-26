A showcase CLI tool for parallel CSV processing.

# Motivation

A common use-case for a parallel pipeline is grouping events to some entity,
such as transactions to a client.

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
TODO

# Discussion
TODO

---
TODO:
- out of order transactions
- throughput
- empty file
- profiling
- We could avoid dispute, resolve and charge back to be a variants of
this enum, and instead have flag on the deposit variant. However, in
practice there are going to be many more deposit transactions that those
combined, and therefore we'd be storing twice as much data, because memory
alignment and enum largest variant size. Hence, it's more economical to
store the events as entries.
- implement tests for empty csv, empty last/first row
- works only on 64bit
- withdrawn more than deposited

# Commands

Run the test suite with `./bin/test.sh` or just unit tests with `cargo test`.

A prerequisite for code coverage tool is _Rust 1.61_ and following dependencies:

```
$ cargo install grcov
$ rustup component add llvm-tools-preview
```

Then run `./bin/codecov.sh` and see the `target/debug/coverage/index.html`.
