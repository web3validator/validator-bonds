use anchor_client::anchor_lang::AccountDeserialize;
use anyhow::anyhow;
use solana_sdk::account::Account;
use validator_bonds::state::settlement_claims::{SettlementClaims, SettlementClaimsWithData};

pub struct SettlementClaimsBitmap {
    settlement_claims: SettlementClaims,
    data: Vec<u8>,
}

impl SettlementClaimsBitmap {
    pub fn new(account: Account) -> anyhow::Result<Self> {
        let vec_data = account.data.to_vec();
        let settlement_claims = SettlementClaims::try_deserialize(&mut vec_data.as_slice())
            .map_or_else(
                |e| Err(anyhow!("Cannot deserialize SettlementClaims data: {}", e)),
                Ok,
            )?;
        Ok(Self {
            settlement_claims,
            data: vec_data,
        })
    }

    pub fn is_set(&mut self, index: u64) -> bool {
        self.settlement_claims_with_data()
            .is_set(index)
            .expect("SettlementClaimsBitmap should be initialized, checked in new()")
    }

    pub fn number_of_set_bits(&mut self) -> u64 {
        self.settlement_claims_with_data()
            .number_of_set_bits()
            .expect("SettlementClaimsBitmap should be initialized, checked in new()")
    }

    fn settlement_claims_with_data(&mut self) -> SettlementClaimsWithData {
        SettlementClaimsWithData::new(self.settlement_claims.max_records, &mut self.data)
    }
}
