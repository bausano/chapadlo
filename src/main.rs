use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Write};

type Amount = u64;
type TxId = u32;
type ClientId = u16;

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
            let amount = amount::str_to_u64(
                tx.amount.ok_or("missing amount for withdrawal")?,
            )?;
            client.withdrawn = client
                .withdrawn
                .checked_add(amount)
                .ok_or("withdrawal amount too large for u64")?;
        }
        TransactionKindCsv::Deposit => {
            let amount = amount::str_to_u64(
                tx.amount.ok_or("missing amount for deposit")?,
            )?;
            client.deposits.push((tx.id, amount));
        }
    };

    Ok(())
}

mod amount {
    /// Represents 4 decimal places that the amounts are scaled by in the program
    /// so that we can have the amount represented by [`u64`].
    const DECIMALS: usize = 4;

    const DECIMAL_MULTIPLIER: u64 = 10_u64.pow(DECIMALS as u32);

    use std::str::FromStr;

    pub fn str_to_u64(input: impl AsRef<str>) -> Result<u64, &'static str> {
        let input = input.as_ref();

        match input.find('.') {
            // special case for omitting decimal dot
            None => u64::from_str(input)
                .map_err(|_| "cannot parse input to u64")?
                .checked_mul(DECIMAL_MULTIPLIER)
                .ok_or("deposit amount too large for u64"),
            Some(decimal_dot_index)
                if decimal_dot_index == 0
                    || decimal_dot_index == input.len() - 1 =>
            {
                // TBD: we could also parse ".123" or "123." here
                Err("not a decimal number")
            }
            // if more than 4 decimal places "0.1231"
            Some(decimal_dot_index)
                if decimal_dot_index + DECIMALS + 1 <= input.len() =>
            {
                Err("at most 4 decimal places allowed")
            }
            Some(decimal_dot_index) => {
                let integer_part = u64::from_str(&input[..decimal_dot_index])
                    .map_err(|_| "cannot parse input to u64")?
                    .checked_mul(DECIMAL_MULTIPLIER)
                    .ok_or("deposit amount too large for u64")?;

                // cases:
                // "0.1" => 4 - 1 => 10^3 => 1 * 1000 => 0_1000
                // "0.15" => 4 - 2 => 10^2 => 15 * 100 => 0_1500
                // "0.153" => 4 - 3 => 10^1 => 153 * 10 => 0_1530
                // "0.1535" => 4 - 4 => 10^0 => 1535 * 1 => 0_1535
                let decimal_multiplier = DECIMALS - decimal_dot_index;

                // we know that "i" is not the last char in the string due to prev
                // match branch
                let decimal_part =
                    u64::from_str(&input[(decimal_dot_index + 1)..])
                        .map_err(|_| "cannot parse input to u64")?
                        .checked_mul(10_u64.pow(decimal_multiplier as u32))
                        .ok_or("deposit amount too large for u64")?;

                integer_part
                    .checked_add(decimal_part)
                    .ok_or("deposit amount too large for u64")
            }
        }
    }

    pub fn u64_to_string(input: u64) -> String {
        let decimal_part = input.rem_euclid(DECIMAL_MULTIPLIER);
        let integer_part = input / DECIMAL_MULTIPLIER;

        format!("{}.{}", integer_part, decimal_part)
    }
}

impl Client {
    fn into_csv_row(self, id: ClientId) -> Result<String, &'static str> {
        let mut frozen = false;
        let mut available = 0u64;
        let mut held = 0u64;
        for (tx_id, amount) in self.deposits {
            match self.flagged.get(&tx_id) {
                Some(FlaggedTransactionState::Disputed) => {
                    held = held
                        .checked_add(amount)
                        .ok_or("held amount too large for u64")?;
                }
                Some(FlaggedTransactionState::ChargedBack) => {
                    frozen = true;
                }
                None => {
                    available = available
                        .checked_add(amount)
                        .ok_or("available amount too large for u64")?;
                }
            }
        }

        let available = available
            .checked_sub(self.withdrawn)
            .ok_or("withdrawn more than deposited")?;

        let total = held
            .checked_add(available)
            .ok_or("total amount too large for u64")?;

        Ok(format!(
            "{},{},{},{},{}\n",
            id,
            amount::u64_to_string(available),
            amount::u64_to_string(held),
            amount::u64_to_string(total),
            frozen
        ))
    }
}
