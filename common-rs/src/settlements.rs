use crate::bonds::get_bonds_for_pubkeys;
use crate::get_validator_bonds_program;

use crate::utils::{get_account_infos_for_pubkeys, get_accounts_for_pubkeys};

use crate::settlement_claims::SettlementClaimsBitmap;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::sync::Arc;
use validator_bonds::state::bond::Bond;
use validator_bonds::state::settlement::{find_settlement_claims_address, Settlement};

pub async fn get_settlements(
    rpc_client: Arc<RpcClient>,
) -> anyhow::Result<Vec<(Pubkey, Settlement)>> {
    let program = get_validator_bonds_program(rpc_client, None)?;
    Ok(program.accounts(Default::default()).await?)
}

pub async fn get_settlements_for_pubkeys(
    rpc_client: Arc<RpcClient>,
    pubkeys: &[Pubkey],
) -> anyhow::Result<Vec<(Pubkey, Option<Settlement>)>> {
    get_accounts_for_pubkeys(rpc_client, pubkeys).await
}

pub async fn get_settlement_claims_for_settlement<'a>(
    settlement_claims: Vec<(Pubkey, Option<SettlementClaimsBitmap>)>,
    settlement_pubkey: &Pubkey,
) -> anyhow::Result<SettlementClaimsBitmap> {
    if settlement_claims.is_empty() {
        Err(anyhow::anyhow!(
            "No settlement claims found for settlement pubkey: {}",
            settlement_pubkey
        ))
    } else {
        let a = settlement_claims
            .into_iter()
            .find(|(pubkey, _)| pubkey == settlement_pubkey);
        if let Some((_, Some(settlement_claims))) = a {
            Ok(settlement_claims)
        } else {
            Err(anyhow::anyhow!(
                "No settlement claims found for settlement pubkey: {}",
                settlement_pubkey
            ))
        }
    }
}

pub async fn get_settlement_claims_for_settlement_pubkeys(
    rpc_client: Arc<RpcClient>,
    settlement_pubkeys: &[Pubkey],
) -> anyhow::Result<Vec<(Pubkey, Option<SettlementClaimsBitmap>)>> {
    let settlement_claims_pubkeys = settlement_pubkeys
        .iter()
        .map(|settlement_pubkey| find_settlement_claims_address(settlement_pubkey).0)
        .collect::<Vec<Pubkey>>();
    get_account_infos_for_pubkeys(rpc_client, &settlement_claims_pubkeys)
        .await?
        .into_iter()
        .map(|(pubkey, account)| {
            if let Some(account) = account {
                Ok((pubkey, Some(SettlementClaimsBitmap::new(account)?)))
            } else {
                Ok((pubkey, None))
            }
        })
        .collect()
}

pub async fn get_bonds_for_settlements(
    rpc_client: Arc<RpcClient>,
    settlements: &[(Pubkey, Settlement)],
) -> anyhow::Result<Vec<(Pubkey, Option<Bond>)>> {
    let bond_pubkeys = settlements
        .iter()
        .map(|(_, settlement)| settlement.bond)
        .collect::<HashSet<_>>() // be unique
        .into_iter()
        .collect::<Vec<Pubkey>>();

    let bonds = get_bonds_for_pubkeys(rpc_client, &bond_pubkeys).await?;

    let settlements_bonds = settlements
        .iter()
        .map(|(pubkey, settlement)| {
            bonds
                .iter()
                .find(|(bond_pubkey, bond)| bond_pubkey == pubkey && bond.is_some())
                .map_or_else(
                    || (settlement.bond, None),
                    |(_, bond)| {
                        if let Some(bond) = bond {
                            (settlement.bond, Some(bond.clone()))
                        } else {
                            (settlement.bond, None)
                        }
                    },
                )
        })
        .collect();

    Ok(settlements_bonds)
}
