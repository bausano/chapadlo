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
    for result in rdr.deserialize::<TransactionCsv>() {
        match result {
            Ok(tx) => {
                let client = clients.entry(tx.client_id).or_default();
                client.process_transaction(
                    tx.id,
                    tx.kind,
                    tx.amount.as_deref(),
                )?;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    csv::ErrorKind::UnequalLengths { .. }
                ) =>
            {
                // blank row, skip it
                continue;
            }
            Err(e) => {
                return Err(e).with_context(|| "Invalid transaction row format")
            }
        };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_parses_empty_csv() {
        let input = "";

        assert_eq!(
            read_transactions(input.as_bytes()).unwrap(),
            Default::default()
        );
    }

    #[test]
    fn it_ignores_white_spaces() -> Result<()> {
        let input = "\
        type   , client, tx, amount
        deposit ,2,6,   2.0
        deposit, 2,3,   6.0


        ";

        let mut client = Client::default();
        client.process_transaction(
            6,
            TransactionKindCsv::Deposit,
            Some("2.0"),
        )?;
        client.process_transaction(
            3,
            TransactionKindCsv::Deposit,
            Some("6.0"),
        )?;

        assert_eq!(
            read_transactions(input.as_bytes()).unwrap(),
            vec![(2, client)].into_iter().collect()
        );

        Ok(())
    }

    #[test]
    fn it_writes_empty_clients_to_buffer() -> Result<()> {
        let mut buf = vec![];
        write_clients(&mut buf, Default::default())?;

        let csv = String::from_utf8(buf)?;

        assert_eq!(csv, "client,available,held,total,locked\n");

        Ok(())
    }

    #[test]
    fn it_writes_clients_to_buffer() -> Result<()> {
        let mut client1 = Client::default();
        client1.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client1.process_transaction(
            2,
            TransactionKindCsv::Withdrawal,
            Some("1"),
        )?;
        client1.process_transaction(
            3,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client1.process_transaction(3, TransactionKindCsv::Dispute, None)?;
        client1.process_transaction(3, TransactionKindCsv::Dispute, None)?;
        client1.process_transaction(3, TransactionKindCsv::Resolve, None)?;

        let mut client2 = Client::default();
        client2.process_transaction(
            5,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client2.process_transaction(
            6,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client2.process_transaction(
            7,
            TransactionKindCsv::Withdrawal,
            Some("1"),
        )?;
        client2.process_transaction(5, TransactionKindCsv::ChargeBack, None)?;
        client2.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        client2.process_transaction(
            8,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client2.process_transaction(8, TransactionKindCsv::Dispute, None)?;
        client2.process_transaction(
            9,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client2.process_transaction(9, TransactionKindCsv::Dispute, None)?;
        client2.process_transaction(9, TransactionKindCsv::ChargeBack, None)?;

        let mut buf = vec![];
        write_clients(
            &mut buf,
            vec![(1, client1), (2, client2), (3, Client::default())]
                .into_iter()
                .collect(),
        )?;

        let csv = String::from_utf8(buf)?;
        let lines: Vec<&str> = csv.lines().collect();
        println!("{:#?}", lines);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "client,available,held,total,locked");
        assert!(lines.contains(&"1,1.0000,0.0000,1.0000,false"));
        assert!(lines.contains(&"2,1.0000,1.0000,2.0000,true"));
        assert!(lines.contains(&"3,0.0000,0.0000,0.0000,false"));

        Ok(())
    }
}
