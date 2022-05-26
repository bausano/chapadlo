//! Decimal is represented by [`u64`] in this program. There are [`DECIMALS`]
//! decimal places that the amounts are scaled by in the program.

use crate::prelude::*;
use std::fmt;
use std::str::FromStr;

const DECIMALS: usize = 4;
const DECIMAL_MULTIPLIER: u64 = 10_u64.pow(DECIMALS as u32);

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Amount(pub u64);

impl Amount {
    pub fn checked_add(self, other: Amount) -> Result<Amount> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or_else(|| anyhow!("integer overflow"))
    }

    pub fn checked_sub(self, other: Amount) -> Result<Amount> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or_else(|| anyhow!("integer underflow"))
    }
}

impl FromStr for Amount {
    type Err = anyhow::Error;

    /// ```rust
    /// assert_eq!(Amount::from_str("10.85"), Ok(Amount(10_8500)));
    /// ```
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let amount = match input.find('.') {
            // special case for omitting decimal dot
            None => u64::from_str(input)?
                .checked_mul(DECIMAL_MULTIPLIER)
                .ok_or_else(|| anyhow!("integer overflow")),
            Some(decimal_dot_index)
                if decimal_dot_index == 0
                    || decimal_dot_index == input.len() - 1 =>
            {
                Err(anyhow!("not a decimal number"))
            }
            // if more than 4 decimal places "0.1231"
            Some(decimal_dot_index)
                if decimal_dot_index + DECIMALS + 1 < input.len() =>
            {
                Err(anyhow!("at most 4 decimal places allowed"))
            }
            Some(decimal_dot_index) => {
                let integer_part = u64::from_str(&input[..decimal_dot_index])?
                    .checked_mul(DECIMAL_MULTIPLIER)
                    .ok_or_else(|| anyhow!("integer overflow"))?;

                // cases:
                // "0.1" => 4 - (3 - 1 - 1) => 1 * 10^3 => 0_1000
                // "0.15" => 4 - (4 - 1 - 1) => 15 * 10^2 => 0_1500
                // "0.153" => 4 - (5 - 1 - 1) => 153 * 10^1 => 0_1530
                // "0.1535" => 4 - (6 - 1 - 1) => 1535 * 10^0 => 0_1535
                // overflow cannot happen due to a condition above which rejects
                // more than 4 decimal places
                let decimal_multiplier =
                    DECIMALS - (input.len() - 1 - decimal_dot_index);

                // we know that "i" is not the last char in the string due to prev
                // match branch
                let decimal_part =
                    u64::from_str(&input[(decimal_dot_index + 1)..])?
                        .checked_mul(10_u64.pow(decimal_multiplier as u32))
                        .ok_or_else(|| anyhow!("integer overflow"))?;

                integer_part
                    .checked_add(decimal_part)
                    .ok_or_else(|| anyhow!("integer overflow"))
            }
        }?;

        Ok(Self(amount))
    }
}

impl fmt::Display for Amount {
    /// ```rust
    /// assert_eq!(&Amount(10_8500).to_string(), "10.85");
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let decimal_part = self.0.rem_euclid(DECIMAL_MULTIPLIER);
        let integer_part = self.0 / DECIMAL_MULTIPLIER;

        write!(f, "{}.{:04}", integer_part, decimal_part)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_adds() {
        assert_eq!(Amount(1).checked_add(Amount(2)).unwrap(), Amount(3));
        assert_eq!(Amount(0).checked_add(Amount(2)).unwrap(), Amount(2));
        assert_eq!(Amount(0).checked_add(Amount(0)).unwrap(), Amount(0));
        assert_eq!(
            Amount(u64::MAX).checked_add(Amount(0)).unwrap(),
            Amount(u64::MAX)
        );

        assert!(Amount(u64::MAX).checked_add(Amount(1)).is_err());
    }

    #[test]
    fn it_subs() {
        assert_eq!(Amount(2).checked_sub(Amount(2)).unwrap(), Amount(0));
        assert_eq!(Amount(2).checked_sub(Amount(1)).unwrap(), Amount(1));
        assert_eq!(Amount(0).checked_sub(Amount(0)).unwrap(), Amount(0));
        assert_eq!(Amount(1).checked_sub(Amount(0)).unwrap(), Amount(1));
        assert_eq!(
            Amount(u64::MAX).checked_sub(Amount(0)).unwrap(),
            Amount(u64::MAX)
        );
        assert_eq!(
            Amount(u64::MAX).checked_sub(Amount(u64::MAX)).unwrap(),
            Amount(0)
        );

        assert!(Amount(0).checked_sub(Amount(1)).is_err());
    }

    #[test]
    fn it_writes_amount_to_string() {
        assert_eq!(&Amount(10_8500).to_string(), "10.8500");
        assert_eq!(&Amount(0_8500).to_string(), "0.8500");
        assert_eq!(&Amount(0_0000).to_string(), "0.0000");
        assert_eq!(&Amount(42816_0390).to_string(), "42816.0390");
    }

    #[test]
    fn it_parses_amount_from_string() {
        assert_eq!(Amount::from_str("10.0").unwrap(), Amount(10_0000));
        assert_eq!(Amount::from_str("0.0").unwrap(), Amount(0));
        assert_eq!(Amount::from_str("0").unwrap(), Amount(0));
        assert_eq!(Amount::from_str("0.5055").unwrap(), Amount(0_5055));
        assert_eq!(Amount::from_str("0.50").unwrap(), Amount(0_5000));
        assert_eq!(Amount::from_str("12837.502").unwrap(), Amount(12837_5020));
        assert_eq!(Amount::from_str("60").unwrap(), Amount(60_0000));
        assert!(Amount::from_str("0.50012").is_err());
        assert!(Amount::from_str("0.5001023901").is_err());
        assert!(Amount::from_str("asd").is_err());
        assert!(Amount::from_str("asd.").is_err());
        assert!(Amount::from_str("1.").is_err());
        assert!(Amount::from_str(".1").is_err());
        assert!(Amount::from_str(".").is_err());
        assert!(Amount::from_str("").is_err());
    }
}
