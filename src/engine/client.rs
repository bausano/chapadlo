//! Represents client state by grouping transactions which mutate the state
//! into a data structure [`Client`] which enables to serialized it into CSV
//! according to the spec.

use super::TransactionKindCsv;
use crate::prelude::*;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Client {
    /// Once a client is frozen, all deposits or withdrawals are ignored.
    is_frozen: bool,
    /// This decreases with withdrawal and dispute txs, and increases with
    /// deposit and resolve txs.
    available: Amount,
    /// This decreases with resolve and charge back txs and increases with
    /// dispute tx.
    held: Amount,
    /// Adding repeatedly into a hashmap incurs the cost of rebuilding it.
    /// However, since we need to refer to deposit amount due to disputes, the
    /// cost of searching for a transaction in a vector would be `O(N)`, because
    /// the txs come to us unsorted by id.
    ///
    /// If we know average number of deposits per client, we could default the
    /// size of the map on construction. However, that's an over-optimization
    /// for this program.
    ///
    /// Deposit is deemed as frozen if the amount is zero. A deposit tx with
    /// amount 0 is skipped.
    deposits: HashMap<TxId, Amount>,
    /// Since state change txs are rare, we don't store this information in
    /// the deposits map, as that would grow memory while most of that memory
    /// would be set to "false" disputed flag.
    ///
    /// # Invariants
    /// If an id is in this set, then it must also be in the `deposits` map.
    /// That's because we skip disputes for non-existing deposits and we never
    /// delete from `deposits`.
    disputes: HashSet<TxId>,
}

impl Client {
    /// Given a tx info we update the client's state.
    pub(super) fn process_transaction(
        &mut self,
        id: TxId,
        kind: TransactionKindCsv,
        amount: Option<&str>,
    ) -> Result<()> {
        use TransactionKindCsv::*;

        match kind {
            ChargeBack if self.disputes.contains(&id) => {
                self.is_frozen = true;

                // see the invariant on `disputed` set
                let tx_amount = *self.deposits.get(&id).unwrap();
                self.held = self.held.checked_sub(tx_amount)?;

                // signals that the tx was frozen
                self.deposits.insert(id, Amount(0));
                self.disputes.remove(&id);
            }
            // amount zero means already charged back
            Dispute
                if matches!(self.deposits.get(&id), Some(a) if *a != Amount(0))
                    && !self.disputes.contains(&id) =>
            {
                self.disputes.insert(id);

                // see the invariant on `disputed` set
                let tx_amount = *self.deposits.get(&id).unwrap();
                self.held = self.held.checked_add(tx_amount)?;
                self.available = self.available.checked_sub(tx_amount)?;
            }
            Resolve if self.disputes.contains(&id) => {
                self.disputes.remove(&id);

                // see the invariant on `disputed` set
                let tx_amount = *self.deposits.get(&id).unwrap();
                self.available = self.available.checked_add(tx_amount)?;
                self.held = self.held.checked_sub(tx_amount)?;
            }
            Withdrawal | Deposit if self.is_frozen => (),
            Withdrawal => {
                let amount =
                    Amount::from_str(amount.ok_or_else(|| {
                        anyhow!("no amount for withdrawal tx")
                    })?)?;
                if self.available >= amount {
                    self.available.0 -= amount.0;
                }
            }
            Deposit if !self.deposits.contains_key(&id) => {
                let amount = Amount::from_str(
                    amount
                        .ok_or_else(|| anyhow!("no amount for deposit tx"))?,
                )?;
                self.deposits.insert(id, amount);
                self.available = self.available.checked_add(amount)?;
            }
            // additionally noop if
            // * charge back references non-disputed or non-existing tx
            // * dispute references charged back or non-existing tx
            // * dispute already exist for tx
            // * withdrawal or deposit was done to a frozen client
            // * deposit if deposit with that tx id already exists
            _ => (),
        };

        Ok(())
    }

    pub fn into_csv_row(self, id: ClientId) -> Result<String> {
        let total = self.available.checked_add(self.held)?;

        Ok(format!(
            "{},{},{},{},{}\n",
            id, self.available, self.held, total, self.is_frozen
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_processes_chargeback_transaction() -> Result<()> {
        let mut client = Client::default();

        let client_before = client.clone();
        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        assert_eq!(client, client_before);

        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        assert_eq!(client, client_before);

        client.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("10"),
        )?;
        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        assert_eq!(client.available, Amount(10_0000));
        assert_eq!(client.held, Amount(0));
        assert!(!client.disputes.contains(&1));
        assert_eq!(client.deposits.get(&1), Some(&Amount(10_0000)));

        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;
        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        assert_eq!(client.available, Amount(0));
        assert_eq!(client.held, Amount(0));
        assert!(!client.disputes.contains(&1));
        assert_eq!(client.deposits.get(&1), Some(&Amount(0)));
        assert!(client.is_frozen);

        Ok(())
    }

    #[test]
    fn it_processes_disputed_transaction() -> Result<()> {
        let mut client = Client::default();

        let client_before = client.clone();
        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;
        assert_eq!(client, client_before);

        let mut client = Client::default();
        client.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;
        assert_eq!(client.deposits.get(&1), Some(&Amount(1_0000)));
        assert!(client.disputes.contains(&1));
        assert_eq!(client.available, Amount(0));
        assert_eq!(client.held, Amount(1_0000));
        assert!(!client.is_frozen);

        Ok(())
    }

    #[test]
    fn it_processes_resolved_transaction() -> Result<()> {
        let mut client = Client::default();

        client.process_transaction(1, TransactionKindCsv::Resolve, None)?;
        assert!(client.deposits.is_empty());
        assert!(client.disputes.is_empty());
        assert_eq!(client.available, Amount(0));
        assert_eq!(client.held, Amount(0));
        assert!(!client.is_frozen);

        let mut client = Client::default();
        client.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;
        client.process_transaction(1, TransactionKindCsv::Resolve, None)?;
        assert_eq!(client.available, Amount(1_0000));
        assert!(client.disputes.is_empty());
        assert_eq!(client.held, Amount(0));

        Ok(())
    }

    #[test]
    fn it_processes_withdrawal_transaction() -> Result<()> {
        let mut client = Client::default();

        assert!(client
            .process_transaction(1, TransactionKindCsv::Withdrawal, None)
            .is_err());
        assert!(client
            .process_transaction(1, TransactionKindCsv::Withdrawal, Some("asd"))
            .is_err());

        client.process_transaction(
            1,
            TransactionKindCsv::Withdrawal,
            Some("10.0"),
        )?;
        assert_eq!(client.available, Amount(0));
        assert_eq!(client.held, Amount(0));
        assert!(client.deposits.is_empty());
        assert!(client.disputes.is_empty());

        client.process_transaction(
            2,
            TransactionKindCsv::Deposit,
            Some("2"),
        )?;
        client.process_transaction(
            2,
            TransactionKindCsv::Withdrawal,
            Some("0.300"),
        )?;
        assert_eq!(client.available, Amount(1_7000));
        client.process_transaction(
            3,
            TransactionKindCsv::Withdrawal,
            Some("10"),
        )?;
        assert_eq!(client.available, Amount(1_7000));

        Ok(())
    }

    #[test]
    fn it_processes_deposit_transaction() -> Result<()> {
        let mut client = Client::default();

        assert!(client
            .process_transaction(1, TransactionKindCsv::Deposit, None)
            .is_err());
        assert!(client
            .process_transaction(1, TransactionKindCsv::Deposit, Some("asd"))
            .is_err());

        client.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("10.0"),
        )?;
        assert_eq!(
            client.deposits,
            vec![(1, Amount(10_0000))].into_iter().collect()
        );
        assert_eq!(client.available, Amount(10_0000));
        assert_eq!(client.held, Amount(0));
        assert!(client.disputes.is_empty());

        client.process_transaction(
            2,
            TransactionKindCsv::Deposit,
            Some("0.300"),
        )?;
        assert_eq!(
            client.deposits,
            vec![(1, Amount(10_0000)), (2, Amount(0_3000))]
                .into_iter()
                .collect()
        );
        assert_eq!(client.available, Amount(10_3000));
        assert_eq!(client.held, Amount(0));
        assert!(client.disputes.is_empty());

        client.process_transaction(
            2, // duplicate id
            TransactionKindCsv::Deposit,
            Some("0.300"),
        )?;
        assert_eq!(
            client.deposits,
            vec![(1, Amount(10_0000)), (2, Amount(0_3000))]
                .into_iter()
                .collect()
        );
        assert_eq!(client.available, Amount(10_3000));
        assert_eq!(client.held, Amount(0));
        assert!(client.disputes.is_empty());

        Ok(())
    }

    #[test]
    fn it_serializes_client_as_empty_csv_row() -> Result<()> {
        assert_eq!(
            Client::default().into_csv_row(1)?,
            "1,0.0000,0.0000,0.0000,false\n"
        );

        Ok(())
    }

    #[test]
    fn it_can_end_up_even() -> Result<()> {
        let mut client = Client::default();

        client.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client.process_transaction(
            2,
            TransactionKindCsv::Withdrawal,
            Some("1"),
        )?;

        assert_eq!(client.into_csv_row(1)?, "1,0.0000,0.0000,0.0000,false\n");

        Ok(())
    }

    #[test]
    fn it_marks_funds_as_held_if_disputed() -> Result<()> {
        let mut client = Client::default();

        client.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client.process_transaction(
            2,
            TransactionKindCsv::Deposit,
            Some("3"),
        )?;
        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;

        assert_eq!(client.into_csv_row(1)?, "1,3.0000,1.0000,4.0000,false\n");

        Ok(())
    }

    #[test]
    fn it_freezes_client_if_charged_back() -> Result<()> {
        let mut client = Client::default();

        client.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("1"),
        )?;
        client.process_transaction(
            2,
            TransactionKindCsv::Deposit,
            Some("3"),
        )?;
        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;
        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;

        assert_eq!(client.into_csv_row(1)?, "1,3.0000,0.0000,3.0000,true\n");

        Ok(())
    }

    #[test]
    fn it_doesnt_mark_client_as_frozen_if_no_chargeback_on_valid_deposit(
    ) -> Result<()> {
        let mut client = Client::default();
        client.process_transaction(
            1,
            TransactionKindCsv::Deposit,
            Some("2.0"),
        )?;
        client.process_transaction(
            1,
            TransactionKindCsv::Withdrawal,
            Some("1.0"),
        )?;
        assert_eq!(
            client.clone().into_csv_row(1).unwrap(),
            "1,1.0000,0.0000,1.0000,false\n"
        );

        client.process_transaction(
            2, // this deposit doesn't exist
            TransactionKindCsv::ChargeBack,
            None,
        )?;
        assert_eq!(
            client.clone().into_csv_row(1).unwrap(),
            "1,1.0000,0.0000,1.0000,false\n"
        );

        client.process_transaction(
            3,
            TransactionKindCsv::Withdrawal,
            Some("0.0"),
        )?;
        client.process_transaction(
            3, // doesn't work on withdrawal
            TransactionKindCsv::ChargeBack,
            None,
        )?;
        assert_eq!(
            client.clone().into_csv_row(1).unwrap(),
            "1,1.0000,0.0000,1.0000,false\n"
        );

        Ok(())
    }
}
