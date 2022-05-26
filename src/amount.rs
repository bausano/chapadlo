//! Decimal is represented by [`u64`] in this program. There are [`DECIMALS`]
//! decimal places that the amounts are scaled by in the program.

use std::fmt;
use std::str::FromStr;

const DECIMALS: usize = 4;
const DECIMAL_MULTIPLIER: u64 = 10_u64.pow(DECIMALS as u32);

#[derive(Default, Debug, Copy, Clone)]
pub struct Amount(pub u64);

impl Amount {
    pub fn checked_add(self, other: Amount) -> Option<Amount> {
        self.0.checked_add(other.0).map(Self)
    }

    pub fn checked_sub(self, other: Amount) -> Option<Amount> {
        self.0.checked_sub(other.0).map(Self)
    }
}

impl FromStr for Amount {
    type Err = &'static str;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let amount = match input.find('.') {
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
        }?;

        Ok(Self(amount))
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let decimal_part = self.0.rem_euclid(DECIMAL_MULTIPLIER);
        let integer_part = self.0 / DECIMAL_MULTIPLIER;

        write!(f, "{}.{}", integer_part, decimal_part)
    }
}
