mod amount;
mod client;
mod prelude;

use client::Client;
use prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Write};

#[derive(Debug, Deserialize)]
struct TransactionCsv {
    #[serde(rename(deserialize = "type"))]
    kind: TransactionKindCsv,
    #[serde(rename(deserialize = "client"))]
    client_id: ClientId,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum TransactionKindCsv {
    /// Is associated with a deposit transaction which have been disputed.
    /// Freezes client's account.
    ChargeBack,
    /// Opens a dispute which moves the transaction's amount into held funds.
    Dispute,
    /// Closes a dispute which moves the transaction's amount into available
    /// funds.
    Resolve,
    Deposit,
    Withdrawal,
}

#[derive(Debug, Serialize)]
struct ClientCsv {
    #[serde(rename(serialize = "client"))]
    id: ClientId,
    available: String,
    held: String,
    total: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let data = "\
    type, client, tx, amount
    deposit,1,6,2.0
    deposit,1,1, 1.0
    deposit,2,2,   6.0
    withdrawal,1,4,1.5
    dispute,1,6,
    deposit,1,3,2.0
    withdrawal,2,5,3.0
    resolve,1,6,
    dispute,1,6,
    deposit,2,7,5.0
    dispute,1,1,
    chargeback,2,7,";

    // adding new clients to this hashmap will be expensive, but we assume that
    // there are many more transactions than clients and optimize for
    // retrieval
    let mut clients: HashMap<ClientId, Client> = Default::default();

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(data.as_bytes());
    for result in rdr.deserialize() {
        let tx: TransactionCsv = result?;

        let client = clients.entry(tx.client_id).or_insert(Client::default());
        client.process_transaction(tx.id, tx.kind, tx.amount)?;
    }

    println!("client,available,held,total,locked");

    let mut stdout = io::stdout();
    for (index, (id, client)) in clients.drain().enumerate() {
        stdout
            .write_all(&client.into_csv_row(id)?.into_bytes())
            .map_err(|_| "cannot write to stdout")?;

        if index % 100 == 0 {
            stdout.flush().map_err(|_| "cannot flush to stdout")?;
        }
    }

    stdout.flush().map_err(|_| "cannot flush to stdout")?;

    Ok(())
}
