use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, utoipa::ToSchema)]
pub struct ValidatorBondRecord {
    pub pubkey: String,
    pub vote_account: String,
    pub authority: String,
    pub cpmpe: Decimal,
    pub updated_at: DateTime<Utc>,
    pub epoch: u64,
}
