use crate::dto::StakeAccount;
use solana_account_decoder::*;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
    rpc_filter::{Memcmp, RpcFilterType},
};
use solana_program::{pubkey::Pubkey, stake};
use std::{collections::HashMap, str::FromStr};

pub async fn get_stake_accounts(
    rpc_client: &RpcClient,
    stake_authority: Option<&String>,
    withdraw_authority: Option<&String>,
) -> anyhow::Result<HashMap<String, StakeAccount>> {
    let mut filters: Vec<RpcFilterType> = Default::default();
    if let Some(stake_authority) = stake_authority {
        let stake_authority = Pubkey::from_str(&stake_authority)?;
        filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            4 + 8,
            stake_authority.to_bytes().to_vec(),
        )));
    }
    if let Some(withdraw_authority) = withdraw_authority {
        let withdraw_authority = Pubkey::from_str(&withdraw_authority)?;
        filters.push(RpcFilterType::Memcmp(Memcmp::new_raw_bytes(
            4 + 8 + 32,
            withdraw_authority.to_bytes().to_vec(),
        )));
    }

    let accounts = rpc_client
        .get_program_accounts_with_config(
            &stake::program::ID,
            RpcProgramAccountsConfig {
                filters: Some(filters),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    commitment: Some(rpc_client.commitment()),
                    data_slice: None,
                    min_context_slot: None,
                },
                with_context: None,
            },
        )
        .await?;
    Ok(accounts
        .into_iter()
        .map(|(pubkey, account)| (pubkey.to_string(), (pubkey, account).into()))
        .collect())
}
