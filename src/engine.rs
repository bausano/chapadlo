//! Processes transactions into a client state data structure and outputs the
//! state as CSV string.

mod client;

use crate::prelude::*;
use client::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};

const CSV_HEADERS: &[u8] = b"client,available,held,total,locked\n";

/// See the README for more information.
#[derive(Debug, Deserialize, PartialEq, Copy, Clone)]
#[serde(rename_all = "lowercase")]
pub enum TransactionKindCsv {
    /// Is associated with a deposit transaction which have been disputed.
    /// Freezes client's account.
    ChargeBack,
    /// Opens a dispute which moves the transaction's amount into held funds.
    Dispute,
    /// Closes a dispute which moves the transaction's amount into available
    /// funds.
    Resolve,
    /// Increases available funds of a client unless the transaction is disputed
    /// or changed back.
    Deposit,
    /// Decreases available funds of a client. Cannot be disputed or charged
    /// back.
    Withdrawal,
}

#[derive(Debug, Deserialize)]
struct TransactionCsv {
    #[serde(rename(deserialize = "type"))]
    kind: TransactionKindCsv,
    #[serde(rename(deserialize = "client"))]
    client_id: ClientId,
    /// Transaction ID is referenced by [`TransactionKindCsv::Resolve`],
    /// [`TransactionKindCsv::ChargeBack`] and [`TransactionKindCsv::Dispute`]
    /// transactions. For these kinds, the id should refer to a chronologically
    /// previous [`TransactionKindCsv::Deposit`] transaction.
    ///
    /// For [`TransactionKindCsv::Deposit`], [`TransactionKindCsv::Withdrawal`]
    /// this represents the ID of those transactions and is irrelevant for
    /// the latter in the logic of this program.
    #[serde(rename(deserialize = "tx"))]
    id: TxId,
    /// We could use a crate such as [`rust_decimal`][rust-decimal]. However,
    /// since we're working in the realm of positive numbers only, and we know
    /// that the precision is always set to 4 decimal places, [`u64`] saves us
    /// 8 bytes per transaction.
    ///
    /// Another option is to implement a custom deserialization type for the
    /// amount. However, since we're not working with this type beyond the
    /// parsing logic, we might as well parse the string in the body of the
    /// function and avoid over-complication of implementing deser for a custom
    /// type.
    ///
    /// [rust-decimal]: https://github.com/paupino/rust-decimal
    amount: Option<String>,
}

/// Given a CSV buffer (with header) of transactions, groups them by client
/// to create client state representation.
pub fn read_transactions(
    handle: impl Read,
) -> Result<HashMap<ClientId, Client>> {
    // adding new clients to this hashmap will be expensive, but we assume that
    // there are many more transactions than clients and optimize for
    // retrieval
    let mut clients: HashMap<ClientId, Client> = Default::default();

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(handle);
    for result in rdr.deserialize() {
        let tx: TransactionCsv =
            result.with_context(|| "Invalid transaction row format")?;

        let client = clients.entry(tx.client_id).or_default();
        client.process_transaction(tx.id, tx.kind, tx.amount.as_deref())?;
    }

    Ok(clients)
}

/// Given client states, writes them into a buffer as CSV string according
/// to the API described in README.
pub fn write_clients(
    mut handle: impl Write,
    mut clients: HashMap<ClientId, Client>,
) -> Result<()> {
    // Enables the piped recipient to process the output as stream if they
    // wish so
    const FLUSH_EVERY_N_ROWS: usize = 100;

    handle.write_all(CSV_HEADERS)?;

    for (index, (id, client)) in clients.drain().enumerate() {
        handle.write_all(&client.into_csv_row(id)?.into_bytes())?;

        if index % FLUSH_EVERY_N_ROWS == 0 {
            handle.flush()?;
        }
    }

    handle.flush()?;

    Ok(())
}
