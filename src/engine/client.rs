//! Represents client state by grouping transactions which mutate the state
//! into a data structure [`Client`] which enables to serialized it into CSV
//! according to the spec.

use super::TransactionKindCsv;
use crate::prelude::*;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Default, Debug, Clone)]
pub struct Client {
    /// Deposit txs are kept in vec instead of a map because it's
    /// write heavy and we don't actually need to read those txs more than
    /// once when we iterate over them.
    deposits: Vec<(TxId, Amount)>,
    /// Withdraw txs are not referenced anymore and hence can be casted into a
    /// single integer.
    withdrawn: Amount,
    /// When we iterate over the deposit tx, we need to retrieve information
    /// about their state.
    ///
    /// Since state change txs are rare, but we want to retrieve state for a
    /// tx readily, we store them in a map.
    flagged: HashMap<TxId, FlaggedTransactionState>,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
enum FlaggedTransactionState {
    /// The amount on a disputed tx will count towards client's held amount.
    Disputed,
    /// The amount of a chargeback tx will not be counted towards client's
    /// state, and it means that the client is marked as frozen.
    ChargedBack,
}

impl Client {
    /// Given a tx info we update the client's state.
    pub(super) fn process_transaction(
        &mut self,
        id: TxId,
        kind: TransactionKindCsv,
        amount: Option<&str>,
    ) -> Result<()> {
        match kind {
            TransactionKindCsv::ChargeBack => {
                self.flagged
                    .insert(id, FlaggedTransactionState::ChargedBack);
            }
            TransactionKindCsv::Dispute => {
                match self.flagged.get(&id) {
                    // cannot go from charged back to disputed
                    Some(FlaggedTransactionState::ChargedBack) => (),
                    _ => {
                        self.flagged
                            .insert(id, FlaggedTransactionState::Disputed);
                    }
                };
            }
            TransactionKindCsv::Resolve => {
                match self.flagged.get(&id) {
                    // from Disputed -> Resolve is a valid transition
                    Some(FlaggedTransactionState::Disputed) => {
                        // we don't care if [`None`], it's provider's feed err
                        self.flagged.remove(&id);
                    }
                    // from ChargedBack to Resolve is not a valid transition,
                    // cannot resolve charged back tx and the account must be
                    // frozen, skip this as a provider mistake
                    //
                    // TODO: log this scenario
                    Some(FlaggedTransactionState::ChargedBack) => (),
                    // cannot resolve what's not disputed, provider is trying
                    // to bamboozle us
                    None => (),
                };
            }
            TransactionKindCsv::Withdrawal => {
                let amount =
                    Amount::from_str(amount.ok_or_else(|| {
                        anyhow!("no amount for withdrawal tx")
                    })?)?;
                self.withdrawn = self.withdrawn.checked_add(amount)?;
            }
            TransactionKindCsv::Deposit => {
                let amount = Amount::from_str(
                    amount
                        .ok_or_else(|| anyhow!("no amount for deposit tx"))?,
                )?;
                self.deposits.push((id, amount));
            }
        };

        Ok(())
    }

    pub fn into_csv_row(self, id: ClientId) -> Result<String> {
        let mut frozen = false;
        let mut available = Amount(0);
        let mut held = Amount(0);
        for (tx_id, amount) in self.deposits {
            // check the state of the deposit tx
            match self.flagged.get(&tx_id) {
                Some(FlaggedTransactionState::Disputed) => {
                    held = held.checked_add(amount)?;
                }
                Some(FlaggedTransactionState::ChargedBack) => {
                    frozen = true;
                }
                None => {
                    available = available.checked_add(amount)?;
                }
            }
        }

        // TBD: should be enable this scenario?
        let available = available
            .checked_sub(self.withdrawn)
            .context(anyhow!("withdrawn more than deposited"))?;

        let total = held.checked_add(available)?;

        Ok(format!(
            "{},{},{},{},{}\n",
            id, available, held, total, frozen
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_processes_chargeback_transaction() -> Result<()> {
        let mut client = Client::default();

        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        assert!(client.deposits.is_empty());
        assert_eq!(client.withdrawn, Amount(0));
        assert_eq!(
            client.flagged,
            vec![(1, FlaggedTransactionState::ChargedBack)]
                .into_iter()
                .collect()
        );

        client.process_transaction(
            1,
            TransactionKindCsv::ChargeBack,
            Some("asd"),
        )?;
        assert!(client.deposits.is_empty());
        assert_eq!(client.withdrawn, Amount(0));
        assert_eq!(
            client.flagged,
            vec![(1, FlaggedTransactionState::ChargedBack)]
                .into_iter()
                .collect()
        );

        client.process_transaction(2, TransactionKindCsv::ChargeBack, None)?;
        assert!(client.deposits.is_empty());
        assert_eq!(client.withdrawn, Amount(0));
        assert_eq!(
            client.flagged,
            vec![
                (1, FlaggedTransactionState::ChargedBack),
                (2, FlaggedTransactionState::ChargedBack)
            ]
            .into_iter()
            .collect()
        );

        Ok(())
    }

    #[test]
    fn it_processes_disputed_transaction() -> Result<()> {
        let mut client = Client::default();

        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;
        assert!(client.deposits.is_empty());
        assert_eq!(client.withdrawn, Amount(0));
        assert_eq!(
            client.flagged,
            vec![(1, FlaggedTransactionState::ChargedBack)]
                .into_iter()
                .collect()
        );

        let mut client = Client::default();
        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;
        assert!(client.deposits.is_empty());
        assert_eq!(client.withdrawn, Amount(0));
        assert_eq!(
            client.flagged,
            vec![(1, FlaggedTransactionState::Disputed)]
                .into_iter()
                .collect()
        );

        Ok(())
    }

    #[test]
    fn it_processes_resolved_transaction() -> Result<()> {
        let mut client = Client::default();

        client.process_transaction(1, TransactionKindCsv::ChargeBack, None)?;
        client.process_transaction(1, TransactionKindCsv::Resolve, None)?;
        assert!(client.deposits.is_empty());
        assert_eq!(client.withdrawn, Amount(0));
        assert_eq!(
            client.flagged,
            vec![(1, FlaggedTransactionState::ChargedBack)]
                .into_iter()
                .collect()
        );

        let mut client = Client::default();
        client.process_transaction(1, TransactionKindCsv::Dispute, None)?;
        client.process_transaction(1, TransactionKindCsv::Resolve, None)?;
        assert!(client.flagged.is_empty());
        client.process_transaction(1, TransactionKindCsv::Resolve, None)?;
        assert!(client.flagged.is_empty());
        assert!(client.deposits.is_empty());
        assert_eq!(client.withdrawn, Amount(0));

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
        assert_eq!(client.withdrawn, Amount(10_0000));
        assert!(client.deposits.is_empty());
        assert!(client.flagged.is_empty());
        client.process_transaction(
            2,
            TransactionKindCsv::Withdrawal,
            Some("0.300"),
        )?;
        assert_eq!(client.withdrawn, Amount(10_3000));
        assert!(client.deposits.is_empty());
        assert!(client.flagged.is_empty());

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
        assert_eq!(client.deposits, vec![(1, Amount(10_0000))]);
        assert_eq!(client.withdrawn, Amount(0));
        assert!(client.flagged.is_empty());
        client.process_transaction(
            2,
            TransactionKindCsv::Deposit,
            Some("0.300"),
        )?;
        assert_eq!(
            client.deposits,
            vec![(1, Amount(10_0000)), (2, Amount(0_3000))]
        );
        assert_eq!(client.withdrawn, Amount(0));
        assert!(client.flagged.is_empty());

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
    fn it_fails_if_more_withdrawals_than_deposits() -> Result<()> {
        let mut client = Client::default();

        client.process_transaction(
            1,
            TransactionKindCsv::Withdrawal,
            Some("10.0"),
        )?;
        client.process_transaction(
            2,
            TransactionKindCsv::Deposit,
            Some("9.9999"),
        )?;

        assert!(client.into_csv_row(1).is_err(),);

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
