//! Represents client state by grouping transactions which mutate the state
//! into a data structure [`Client`] which enables to serialized it into CSV
//! according to the spec.

use super::TransactionKindCsv;
use crate::prelude::*;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Default, Debug)]
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

#[derive(Debug)]
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
        amount: Option<String>,
    ) -> Result<()> {
        match kind {
            TransactionKindCsv::ChargeBack => {
                self.flagged
                    .insert(id, FlaggedTransactionState::ChargedBack);
            }
            TransactionKindCsv::Dispute => {
                self.flagged.insert(id, FlaggedTransactionState::Disputed);
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
                let amount = Amount::from_str(
                    &amount.ok_or(anyhow!("no amount for withdrawal tx"))?,
                )?;
                self.withdrawn = self.withdrawn.checked_add(amount)?;
            }
            TransactionKindCsv::Deposit => {
                let amount = Amount::from_str(
                    &amount.ok_or(anyhow!("no amount for deposit tx"))?,
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
