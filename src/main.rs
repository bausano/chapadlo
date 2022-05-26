mod amount;
mod client;
mod prelude;

use client::Client;
use prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, Read, Write};

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
    // skip first arg as that's the binary path
    let file = if let Some(csv_path) = env::args().skip(1).next() {
        File::open(csv_path).map_err(|_| "cannot open csv file")
    } else {
        Err("no input file path provided")
    }?;

    let clients = read_csv(file)?;

    write_csv(io::stdout(), clients)?;

    Ok(())
}

fn read_csv(
    handle: impl Read,
) -> Result<HashMap<ClientId, Client>, &'static str> {
    // adding new clients to this hashmap will be expensive, but we assume that
    // there are many more transactions than clients and optimize for
    // retrieval
    let mut clients: HashMap<ClientId, Client> = Default::default();

    let mut rdr = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_reader(handle);
    for result in rdr.deserialize() {
        let tx: TransactionCsv = result.map_err(|_| "Invalid row")?;

        let client = clients.entry(tx.client_id).or_insert(Client::default());
        client.process_transaction(tx.id, tx.kind, tx.amount)?;
    }

    Ok(clients)
}

fn write_csv(
    mut handle: impl Write,
    mut clients: HashMap<ClientId, Client>,
) -> Result<(), &'static str> {
    const FLUSH_EVERY_N_ROWS: usize = 100;

    println!("client,available,held,total,locked");

    for (index, (id, client)) in clients.drain().enumerate() {
        handle
            .write_all(&client.into_csv_row(id)?.into_bytes())
            .map_err(|_| "cannot write into buffer")?;

        if index % FLUSH_EVERY_N_ROWS == 0 {
            handle.flush().map_err(|_| "cannot flush buffer")?;
        }
    }

    handle.flush().map_err(|_| "cannot flush buffer")?;

    Ok(())
}
