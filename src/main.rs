mod amount;
mod prelude;

use prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Write};
use std::str::FromStr;

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

#[derive(Debug)]
enum FlaggedTransactionState {
    Disputed,
    ChargedBack,
}

#[derive(Default, Debug)]
struct Client {
    deposits: Vec<(TxId, Amount)>,
    withdrawn: Amount,
    flagged: HashMap<TxId, FlaggedTransactionState>,
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
        process_transaction(&mut clients, tx)?;
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

fn process_transaction(
    clients: &mut HashMap<ClientId, Client>,
    tx: TransactionCsv,
) -> Result<(), &'static str> {
    let client = clients.entry(tx.client_id).or_insert(Client::default());

    match tx.kind {
        TransactionKindCsv::ChargeBack => {
            client
                .flagged
                .insert(tx.id, FlaggedTransactionState::ChargedBack);
        }
        TransactionKindCsv::Dispute => {
            client
                .flagged
                .insert(tx.id, FlaggedTransactionState::Disputed);
        }
        TransactionKindCsv::Resolve => {
            match client
                .flagged
                .get(&tx.id)
                .ok_or("Cannot resolve what's not disputed")?
            {
                FlaggedTransactionState::Disputed => {
                    // we don't care if [`None`], it's provider's feed err
                    client.flagged.remove(&tx.id);
                }
                // cannot resolve charged back tx, account frozen
                FlaggedTransactionState::ChargedBack => (),
            };
        }
        TransactionKindCsv::Withdrawal => {
            let amount = Amount::from_str(
                &tx.amount.ok_or("missing amount for withdrawal")?,
            )?;
            client.withdrawn = client
                .withdrawn
                .checked_add(amount)
                .ok_or("withdrawal amount too large")?;
        }
        TransactionKindCsv::Deposit => {
            let amount = Amount::from_str(
                &tx.amount.ok_or("missing amount for deposit")?,
            )?;
            client.deposits.push((tx.id, amount));
        }
    };

    Ok(())
}

impl Client {
    fn into_csv_row(self, id: ClientId) -> Result<String, &'static str> {
        let mut frozen = false;
        let mut available = Amount(0);
        let mut held = Amount(0);
        for (tx_id, amount) in self.deposits {
            match self.flagged.get(&tx_id) {
                Some(FlaggedTransactionState::Disputed) => {
                    held = held
                        .checked_add(amount)
                        .ok_or("held amount too large")?;
                }
                Some(FlaggedTransactionState::ChargedBack) => {
                    frozen = true;
                }
                None => {
                    available = available
                        .checked_add(amount)
                        .ok_or("available amount too large")?;
                }
            }
        }

        let available = available
            .checked_sub(self.withdrawn)
            .ok_or("withdrawn more than deposited")?;

        let total = held
            .checked_add(available)
            .ok_or("total amount too large")?;

        Ok(format!(
            "{},{},{},{},{}\n",
            id, available, held, total, frozen
        ))
    }
}
