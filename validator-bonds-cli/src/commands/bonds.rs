use crate::commands::common::CommonCollectOptions;
use crate::utils::stake_accounts::get_stake_accounts;
use crate::{dto::ValidatorBondRecord, utils::rpc::get_rpc_client};
use anchor_lang::{AccountDeserialize, Discriminator};
use serde_yaml;
use solana_account_decoder::UiAccountEncoding;
use solana_client::{
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::Memcmp,
};
use solana_sdk::pubkey::Pubkey;
use validator_bonds::constants::PROGRAM_ID;
use validator_bonds::state::bond::Bond;

const BONDS_WITHDRAW_AUTHORITY: &str = "7cgg6KhPd1G8oaoB48RyPDWu7uZs51jUpDYB3eq4VebH";

pub async fn collect_bonds(options: CommonCollectOptions) -> anyhow::Result<()> {
    let withdraw_authority = &BONDS_WITHDRAW_AUTHORITY.to_string();
    let client: &solana_client::nonblocking::rpc_client::RpcClient =
        &get_rpc_client(options.rpc_url, options.commitment.to_string());

    let updated_at = chrono::Utc::now();
    let current_epoch_info = client.get_epoch_info().await?;
    let epoch = current_epoch_info.epoch;

    let mut stake_accounts =
        get_stake_accounts(client, Some(withdraw_authority), Some(withdraw_authority)).await?;

    stake_accounts = stake_accounts
        .iter()
        .filter(|sa| sa.1.stake.lockup().unwrap().custodian == Pubkey::default())
        .filter(|sa| sa.1.stake.lockup().unwrap().epoch.lt(&epoch))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut bonds: Vec<ValidatorBondRecord> = vec![];
    let bonds_program = PROGRAM_ID.try_into()?;
    let bond_accounts = client
        .get_program_accounts_with_config(
            &bonds_program,
            RpcProgramAccountsConfig {
                filters: Some(vec![solana_client::rpc_filter::RpcFilterType::Memcmp(
                    Memcmp::new_base58_encoded(0, &Bond::DISCRIMINATOR),
                )]),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    data_slice: None,
                    commitment: None,
                    min_context_slot: None,
                },
                with_context: None,
            },
        )
        .await?;

    for bond_account in bond_accounts {
        let bond: Bond = AccountDeserialize::try_deserialize(&mut bond_account.1.data.as_slice())?;
        let maybe_funding = stake_accounts.iter().find(|x| {
            x.1.stake.delegation().unwrap().voter_pubkey.to_string()
                == bond.vote_account.to_string()
        });

        let funding = match maybe_funding {
            Some(funding) => funding.1.get_lamports(),
            None => 0,
        };

        bonds.push(ValidatorBondRecord {
            pubkey: bond_account.0.to_string(),
            vote_account: bond.vote_account.to_string(),
            authority: bond.authority.to_string(),
            cpmpe: bond.cpmpe.try_into().unwrap(),
            funds: funding,
            epoch,
            updated_at,
        })
    }

    serde_yaml::to_writer(std::io::stdout(), &bonds)?;

    Ok(())
}
