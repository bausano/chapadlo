use super::TransactionKindCsv;
use crate::prelude::*;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Default, Debug)]
pub struct Client {
    deposits: Vec<(TxId, Amount)>,
    withdrawn: Amount,
    flagged: HashMap<TxId, FlaggedTransactionState>,
}

#[derive(Debug)]
enum FlaggedTransactionState {
    Disputed,
    ChargedBack,
}

impl Client {
    pub(super) fn process_transaction(
        &mut self,
        id: TxId,
        kind: TransactionKindCsv,
        amount: Option<String>,
    ) -> Result<(), &'static str> {
        match kind {
            TransactionKindCsv::ChargeBack => {
                self.flagged
                    .insert(id, FlaggedTransactionState::ChargedBack);
            }
            TransactionKindCsv::Dispute => {
                self.flagged.insert(id, FlaggedTransactionState::Disputed);
            }
            TransactionKindCsv::Resolve => {
                match self
                    .flagged
                    .get(&id)
                    .ok_or("Cannot resolve what's not disputed")?
                {
                    FlaggedTransactionState::Disputed => {
                        // we don't care if [`None`], it's provider's feed err
                        self.flagged.remove(&id);
                    }
                    // cannot resolve charged back tx, account frozen
                    FlaggedTransactionState::ChargedBack => (),
                };
            }
            TransactionKindCsv::Withdrawal => {
                let amount = Amount::from_str(
                    &amount.ok_or("missing amount for withdrawal")?,
                )?;
                self.withdrawn = self
                    .withdrawn
                    .checked_add(amount)
                    .ok_or("withdrawal amount too large")?;
            }
            TransactionKindCsv::Deposit => {
                let amount = Amount::from_str(
                    &amount.ok_or("missing amount for deposit")?,
                )?;
                self.deposits.push((id, amount));
            }
        };

        Ok(())
    }

    pub fn into_csv_row(self, id: ClientId) -> Result<String, &'static str> {
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
