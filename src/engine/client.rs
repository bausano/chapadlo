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
                    Some(FlaggedTransactionState::Disputed) => {
                        // we don't care if [`None`], it's provider's feed err
                        self.flagged.remove(&id);
                    }
                    // cannot resolve charged back tx, account frozen
                    Some(FlaggedTransactionState::ChargedBack) => (),
                    // cannot resolve what's not disputed
                    None => (),
                };
            }
            TransactionKindCsv::Withdrawal => {
                let amount = Amount::from_str(
                    &amount
                        .ok_or(anyhow!("missing amount for withdrawal tx"))?,
                )?;
                self.withdrawn = self
                    .withdrawn
                    .checked_add(amount)
                    .ok_or(anyhow!("math overflow"))?;
            }
            TransactionKindCsv::Deposit => {
                let amount = Amount::from_str(
                    &amount.ok_or(anyhow!("missing amount for deposit tx"))?,
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
            match self.flagged.get(&tx_id) {
                Some(FlaggedTransactionState::Disputed) => {
                    held = held
                        .checked_add(amount)
                        .ok_or(anyhow!("math overflow"))?;
                }
                Some(FlaggedTransactionState::ChargedBack) => {
                    frozen = true;
                }
                None => {
                    available = available
                        .checked_add(amount)
                        .ok_or(anyhow!("math overflow"))?;
                }
            }
        }

        // TBD: should be enable this scenario?
        let available = available
            .checked_sub(self.withdrawn)
            .ok_or(anyhow!("withdrawn more than deposited"))?;

        let total = held
            .checked_add(available)
            .ok_or(anyhow!("math overflow"))?;

        Ok(format!(
            "{},{},{},{},{}\n",
            id, available, held, total, frozen
        ))
    }
}
