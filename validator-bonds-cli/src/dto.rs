use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use solana_sdk::{account::Account, pubkey::Pubkey, stake::state::StakeState};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ValidatorBondRecord {
    pub pubkey: String,
    pub vote_account: String,
    pub authority: String,
    pub cpmpe: Decimal,
    pub funds: u64,
    pub epoch: u64,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct StakeAccount {
    pub stake: StakeState,
    pub raw: Account,
    pub pubkey: Pubkey,
}

impl StakeAccount {
    fn new(pubkey: Pubkey, raw: Account, stake: StakeState) -> Self {
        Self { raw, stake, pubkey }
    }

    pub fn get_lamports(&self) -> u64 {
        self.raw.lamports
    }
}

impl From<(Pubkey, Account)> for StakeAccount {
    fn from((pubkey, account): (Pubkey, Account)) -> Self {
        StakeAccount::new(
            pubkey,
            account.clone(),
            bincode::deserialize(&account.data).unwrap(),
        )
    }
}
