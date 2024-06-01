use anchor_client::{DynSigner, Program};
use anyhow::anyhow;
use clap::Parser;
use log::{debug, error, info};
use settlement_engine::merkle_tree_collection::MerkleTreeCollection;
use settlement_engine::settlement_claims::{SettlementCollection, SettlementFunder};
use settlement_engine::utils::read_from_json_file;
use settlement_pipelines::anchor::add_instruction_to_builder;
use settlement_pipelines::arguments::{
    init_from_opts, InitializedGlobalOpts, PriorityFeePolicyOpts, TipPolicyOpts,
};
use settlement_pipelines::arguments::{load_keypair, GlobalOpts};
use settlement_pipelines::cli_result::{CliError, CliResult};
use settlement_pipelines::executor::{execute_in_sequence, execute_parallel};
use settlement_pipelines::init::{get_executor, init_log};
use settlement_pipelines::json_data::{
    resolve_combined, CombinedMerkleTreeSettlementCollections, MerkleTreeMetaSettlement,
};
use settlement_pipelines::reporting::{with_reporting, PrintReportable, ReportHandler};
use settlement_pipelines::settlements::SETTLEMENT_CLAIM_ACCOUNT_SIZE;
use settlement_pipelines::stake_accounts::{
    get_stake_state_type, prepare_merge_instructions, StakeAccountStateType,
    STAKE_ACCOUNT_RENT_EXEMPTION,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::native_token::lamports_to_sol;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::stake::instruction::create_account as create_stake_account;
use solana_sdk::stake::program::ID as stake_program_id;
use solana_sdk::stake::state::{Authorized, Lockup, StakeStateV2};
use solana_sdk::system_program;
use solana_sdk::sysvar::{
    clock::ID as clock_sysvar_id, rent::ID as rent_sysvar_id,
    stake_history::ID as stake_history_sysvar_id,
};
use solana_transaction_builder::TransactionBuilder;
use solana_transaction_executor::{PriorityFeePolicy, TransactionExecutor};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use validator_bonds::instructions::InitSettlementArgs;
use validator_bonds::state::bond::Bond;
use validator_bonds::state::config::find_bonds_withdrawer_authority;
use validator_bonds::state::settlement::{find_settlement_staker_authority, Settlement};
use validator_bonds::ID as validator_bonds_id;
use validator_bonds_common::config::get_config;
use validator_bonds_common::stake_accounts::{
    collect_stake_accounts, get_clock, get_stake_history, obtain_delegated_stake_accounts,
    obtain_funded_stake_accounts_for_settlement, CollectedStakeAccount, CollectedStakeAccounts,
};
use validator_bonds_common::{
    bonds::get_bonds_for_pubkeys, constants::find_event_authority,
    settlements::get_settlements_for_pubkeys,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[clap(flatten)]
    global_opts: GlobalOpts,

    /// Merkle tree collection file path generated by settlement-engine as of --output-merkle-tree-collection
    #[arg(short = 'm', long)]
    input_merkle_tree_collection: PathBuf,

    /// Settlement collection file path generated by settlement-engine as of --output-settlement-collection
    #[arg(short = 's', long)]
    input_settlement_collection: PathBuf,

    /// forcing epoch, overriding from the settlement collection
    #[arg(long)]
    epoch: Option<u64>,

    #[clap(flatten)]
    priority_fee_policy_opts: PriorityFeePolicyOpts,

    #[clap(flatten)]
    tip_policy_opts: TipPolicyOpts,

    /// Marinade wallet that pays for Marinade type Settlements
    #[clap(long)]
    marinade_wallet: Option<String>,

    /// keypair payer for rent of accounts, if not provided, fee payer keypair is used
    #[arg(long)]
    rent_payer: Option<String>,
}

#[tokio::main]
async fn main() -> CliResult {
    let mut reporting = InitSettlementReport::report_handler();
    let result = real_main(&mut reporting).await;
    with_reporting::<InitSettlementReport>(&reporting, result).await
}

async fn real_main(reporting: &mut ReportHandler<InitSettlementReport>) -> anyhow::Result<()> {
    let args: Args = Args::parse();
    init_log(&args.global_opts);

    let InitializedGlobalOpts {
        fee_payer,
        operator_authority,
        priority_fee_policy,
        tip_policy,
        rpc_client,
        program,
    } = init_from_opts(
        &args.global_opts,
        &args.priority_fee_policy_opts,
        &args.tip_policy_opts,
    )?;

    let rent_payer = if let Some(rent_payer) = args.rent_payer.clone() {
        load_keypair(&rent_payer)?
    } else {
        fee_payer.clone()
    };
    let marinade_wallet = if let Some(marinade_wallet) = args.marinade_wallet.clone() {
        load_keypair(&marinade_wallet)?
    } else {
        fee_payer.clone()
    };

    let json_data = load_json_data(
        &args.input_merkle_tree_collection,
        &args.input_settlement_collection,
    )?;

    let config_address = args.global_opts.config;
    info!(
        "Loading merkle tree at: '{:?}', validator-bonds config: {}",
        args.input_merkle_tree_collection, config_address
    );
    let config = get_config(rpc_client.clone(), config_address)
        .await
        .map_err(CliError::retry_able)?;

    let transaction_executor = get_executor(rpc_client.clone(), tip_policy);

    let minimal_stake_lamports = config.minimum_stake_lamports + STAKE_ACCOUNT_RENT_EXEMPTION;
    let epoch = args.epoch.unwrap_or(json_data.epoch);

    // Load on-chain data for Settlement accounts that we need to create
    let mut settlement_records =
        load_on_chain_data(rpc_client.clone(), &json_data, &config_address, epoch).await?;

    reporting
        .reportable
        .init(rpc_client.clone(), epoch, &settlement_records);

    init_settlements(
        &program,
        rpc_client.clone(),
        transaction_executor.clone(),
        &mut settlement_records,
        &config_address,
        fee_payer.clone(),
        operator_authority.clone(),
        rent_payer.clone(),
        epoch,
        &priority_fee_policy,
        reporting,
    )
    .await?;

    merge_settlement_stake_accounts(
        &program,
        rpc_client.clone(),
        transaction_executor.clone(),
        &mut settlement_records,
        &config_address,
        fee_payer.clone(),
        operator_authority.clone(),
        minimal_stake_lamports,
        &priority_fee_policy,
        reporting,
    )
    .await?;

    fund_settlements(
        &program,
        rpc_client.clone(),
        transaction_executor.clone(),
        &mut settlement_records,
        &config_address,
        fee_payer.clone(),
        operator_authority.clone(),
        marinade_wallet.clone(),
        rent_payer.clone(),
        minimal_stake_lamports,
        &priority_fee_policy,
        reporting,
    )
    .await?;

    Ok(())
}

fn load_json_data(
    merkle_trees_path: &PathBuf,
    settlements_path: &PathBuf,
) -> anyhow::Result<CombinedMerkleTreeSettlementCollections> {
    let merkle_tree_collection: MerkleTreeCollection = read_from_json_file(merkle_trees_path)
        .map_err(|e| {
            anyhow!(
                "Cannot read merkle tree collection from file '{:?}': {:?}",
                merkle_trees_path,
                e
            )
        })?;
    let settlement_collection: SettlementCollection = read_from_json_file(settlements_path)
        .map_err(|e| {
            anyhow!(
                "Cannot read settlement collection from file '{:?}': {:?}",
                settlements_path,
                e
            )
        })?;
    resolve_combined(merkle_tree_collection, settlement_collection)
}

/// Load on-chain data for Settlement accounts that we need to create
async fn load_on_chain_data(
    rpc_client: Arc<RpcClient>,
    json_data: &CombinedMerkleTreeSettlementCollections,
    config_address: &Pubkey,
    epoch: u64,
) -> Result<Vec<SettlementRecord>, CliError> {
    // verify what are the settlement accounts that we need to create
    // (not to pushing many RPC calls to the network, squeezing them to less)
    let mut settlement_records = json_data
        .merkle_tree_settlements
        .iter()
        .filter(|MerkleTreeMetaSettlement { merkle_tree, .. }| {
            if merkle_tree.merkle_root.is_some() {
                true
            } else {
                panic!(
                    "Cannot create settlement for vote account {}, epoch {} without a merkle root",
                    merkle_tree.vote_account, epoch
                );
            }
        })
        .map(
            |MerkleTreeMetaSettlement {
                 merkle_tree,
                 settlement,
             }| {
                let merkle_root = merkle_tree.merkle_root.unwrap();
                let vote_account_address = merkle_tree.vote_account;
                let (bond_address, _) = validator_bonds::state::bond::find_bond_address(
                    config_address,
                    &merkle_tree.vote_account,
                );
                let (settlement_address, _) =
                    validator_bonds::state::settlement::find_settlement_address(
                        &bond_address,
                        &merkle_root.to_bytes(),
                        epoch,
                    );
                SettlementRecord {
                    vote_account_address,
                    bond_address,
                    settlement_address,
                    settlement_staker_authority: find_settlement_staker_authority(
                        &settlement_address,
                    )
                    .0,
                    merkle_root: merkle_root.to_bytes(),
                    max_total_claim: merkle_tree.max_total_claim_sum,
                    max_merkle_nodes: merkle_tree.max_total_claims as u64,
                    funder: SettlementFunderType::new(&settlement.meta.funder),
                    bond_account: None,
                    settlement_account: None,
                    state: SettlementRecordState::InProgress,
                }
            },
        )
        .collect::<Vec<SettlementRecord>>();

    // Loading accounts from on-chain, trying to not pushing many RPC calls to the network
    let settlement_addresses: Vec<Pubkey> = settlement_records
        .iter()
        .map(|d| d.settlement_address)
        .collect();
    let settlements = get_settlements_for_pubkeys(rpc_client.clone(), &settlement_addresses)
        .await
        .map_err(CliError::RetryAble)?;
    for (record, (pubkey, settlement)) in settlement_records.iter_mut().zip(settlements.into_iter())
    {
        assert_eq!(
            record.settlement_address, pubkey,
            "Mismatched settlement address"
        ); // sanity check
        record.settlement_account = settlement;
    }
    let bond_addresses: Vec<Pubkey> = settlement_records.iter().map(|d| d.bond_address).collect();
    let bonds = get_bonds_for_pubkeys(rpc_client.clone(), &bond_addresses)
        .await
        .map_err(CliError::RetryAble)?;
    for (record, (pubkey, bond)) in settlement_records.iter_mut().zip(bonds.into_iter()) {
        assert_eq!(record.bond_address, pubkey, "Mismatched bond address"); // sanity check
        record.bond_account = bond;
    }
    Ok(settlement_records)
}

#[allow(clippy::too_many_arguments)]
async fn init_settlements(
    program: &Program<Arc<DynSigner>>,
    rpc_client: Arc<RpcClient>,
    transaction_executor: Arc<TransactionExecutor>,
    settlement_records: &mut Vec<SettlementRecord>,
    config_address: &Pubkey,
    fee_payer: Arc<Keypair>,
    operator_authority: Arc<Keypair>,
    rent_payer: Arc<Keypair>,
    epoch: u64,
    priority_fee_policy: &PriorityFeePolicy,
    reporting: &mut ReportHandler<InitSettlementReport>,
) -> anyhow::Result<()> {
    let mut transaction_builder = TransactionBuilder::limited(fee_payer.clone());
    transaction_builder.add_signer_checked(&operator_authority);
    transaction_builder.add_signer_checked(&rent_payer);

    for settlement_record in settlement_records {
        if settlement_record.bond_account.is_none() {
            reporting.add_error_string(format!(
                "Cannot find bond account {} for vote account {}, claim amount {}",
                settlement_record.bond_address,
                settlement_record.vote_account_address,
                settlement_record.max_total_claim
            ));
            continue;
        }
        if settlement_record.settlement_account.is_some() {
            debug!(
                "Settlement account {} already exists, skipping initialization",
                settlement_record.settlement_address
            );
        } else {
            let req = program
                .request()
                .accounts(validator_bonds::accounts::InitSettlement {
                    config: *config_address,
                    bond: settlement_record.bond_address,
                    operator_authority: operator_authority.pubkey(),
                    system_program: system_program::ID,
                    rent_payer: rent_payer.pubkey(),
                    program: validator_bonds_id,
                    settlement: settlement_record.settlement_address,
                    event_authority: find_event_authority().0,
                })
                .args(validator_bonds::instruction::InitSettlement {
                    init_settlement_args: InitSettlementArgs {
                        merkle_root: settlement_record.merkle_root,
                        rent_collector: rent_payer.pubkey(),
                        max_total_claim: settlement_record.max_total_claim,
                        max_merkle_nodes: settlement_record.max_merkle_nodes,
                        epoch,
                    },
                });
            add_instruction_to_builder(
                &mut transaction_builder,
                &req,
                format!(
                    "InitSettlement: {} (vote account {})",
                    settlement_record.settlement_address, settlement_record.vote_account_address
                ),
            )?;
            reporting
                .reportable
                .add_created_settlement(settlement_record);
        }
    }

    let (tx_count, ix_count) = execute_parallel(
        rpc_client.clone(),
        transaction_executor.clone(),
        &mut transaction_builder,
        priority_fee_policy,
    )
    .await
    .map_err(CliError::retry_able)?;
    info!(
        "InitSettlement [{}]: txes {tx_count}/ixes {ix_count} executed successfully",
        reporting.reportable.list_created_settlements()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn merge_settlement_stake_accounts(
    program: &Program<Arc<DynSigner>>,
    rpc_client: Arc<RpcClient>,
    transaction_executor: Arc<TransactionExecutor>,
    settlement_records: &mut Vec<SettlementRecord>,
    config_address: &Pubkey,
    fee_payer: Arc<Keypair>,
    operator_authority: Arc<Keypair>,
    minimal_stake_lamports: u64,
    priority_fee_policy: &PriorityFeePolicy,
    reporting: &mut ReportHandler<InitSettlementReport>,
) -> anyhow::Result<()> {
    let mut transaction_builder = TransactionBuilder::limited(fee_payer.clone());
    transaction_builder.add_signer_checked(&operator_authority);
    let (withdrawer_authority, _) = find_bonds_withdrawer_authority(config_address);
    let stake_accounts =
        collect_stake_accounts(rpc_client.clone(), Some(&withdrawer_authority), None).await?;

    let mut fund_bond_stake_accounts = get_on_chain_bond_stake_accounts(
        rpc_client.clone(),
        &stake_accounts,
        &withdrawer_authority,
    )
    .await?;

    let settlement_addresses: Vec<Pubkey> = settlement_records
        .iter()
        .map(|d| d.settlement_address)
        .collect();
    let funded_to_settlement_stakes = obtain_funded_stake_accounts_for_settlement(
        rpc_client.clone(),
        stake_accounts,
        config_address,
        settlement_addresses,
    )
    .await
    .map_err(CliError::retry_able)?;

    let clock = get_clock(rpc_client.clone())
        .await
        .map_err(CliError::retry_able)?;
    let stake_history = get_stake_history(rpc_client.clone())
        .await
        .map_err(CliError::retry_able)?;

    // Merging stake accounts to fit for validator bonds funding
    for settlement_record in settlement_records {
        if settlement_record.bond_account.is_none() {
            // not bond account to work with; already reported
            continue;
        }

        let settlement_amount_funded = funded_to_settlement_stakes
            .get(&settlement_record.settlement_address)
            .map_or(0, |(lamports_in_accounts, _)| *lamports_in_accounts);
        let amount_to_fund = settlement_record.settlement_account.as_ref().map_or(
            settlement_record.max_total_claim,
            |settlement| {
                settlement
                    .max_total_claim
                    .saturating_sub(settlement_amount_funded)
            },
        );

        if amount_to_fund == 0 {
            info!(
                "Settlement {} (vote account {}), funder {:?} already funded by {}, skipping funding",
                settlement_record.settlement_address,
                settlement_record.vote_account_address,
                settlement_record.funder,
                lamports_to_sol(settlement_amount_funded),
            );
            settlement_record.state = SettlementRecordState::AlreadyFunded;
            reporting.reportable.funded_already += settlement_record.max_total_claim;
            continue;
        }

        match &mut settlement_record.funder {
            SettlementFunderType::Marinade(_) => {
                info!(
                    "Settlement {} (vote account {}) is to be funded by Marinade from fee wallet by {} SOLs",
                    settlement_record.settlement_address,
                    settlement_record.vote_account_address,
                    lamports_to_sol(amount_to_fund)
                );
                settlement_record.funder =
                    SettlementFunderType::Marinade(Some(SettlementFunderMarinade {
                        amount_to_fund,
                    }));
                reporting.reportable.funded_overall += amount_to_fund;
            }
            SettlementFunderType::ValidatorBond(validator_bonds_funders) => {
                let mut empty_vec: Vec<FundBondStakeAccount> = vec![];
                let funding_stake_accounts = fund_bond_stake_accounts
                    .get_mut(&settlement_record.vote_account_address)
                    .unwrap_or(&mut empty_vec);
                info!(
                        "Settlement {} (vote account {}) is to be funded by validator by {} SOLs, stake accounts: {} with {} SOLs",
                        settlement_record.settlement_address,
                        settlement_record.vote_account_address,
                        lamports_to_sol(amount_to_fund),
                        funding_stake_accounts.len(),
                        lamports_to_sol(funding_stake_accounts
                        .iter()
                        .map(|s| s.lamports)
                        .sum::<u64>())
                    );
                let mut lamports_available: u64 = 0;
                let mut stake_accounts_to_fund: Vec<FundBondStakeAccount> = vec![];
                funding_stake_accounts.retain(|stake_account| {
                    if lamports_available < amount_to_fund + minimal_stake_lamports {
                        lamports_available += stake_account.lamports;
                        stake_accounts_to_fund.push(stake_account.clone());
                        true // delete from the list, no available anymore, it will be funded
                    } else {
                        false // do not delete, it can be used for other settlement
                    }
                });

                // for the found and fitting stake accounts: taking first one and trying to merge other ones into it
                let stake_account_to_fund: Option<(FundBondStakeAccount, StakeAccountStateType)> =
                    if stake_accounts_to_fund.is_empty() || lamports_available == 0 {
                        None
                    } else {
                        let account = stake_accounts_to_fund.remove(0);
                        let stake_type =
                            get_stake_state_type(&account.state, &clock, &stake_history);
                        Some((account, stake_type))
                    };
                if let Some((
                    FundBondStakeAccount {
                        stake_account: destination_stake,
                        split_stake_account: destination_split_stake,
                        state: destination_stake_state,
                        ..
                    },
                    destination_stake_state_type,
                )) = stake_account_to_fund
                {
                    info!(
                        "Settlement: {} (vote account {}) will be funded with {} stake accounts, possibly merged into {}",
                        settlement_record.settlement_address,
                        settlement_record.vote_account_address,
                        stake_accounts_to_fund.len() + 1,
                        destination_stake
                    );

                    validator_bonds_funders.push(SettlementFunderValidatorBond {
                        stake_account_to_fund: destination_stake,
                    });
                    let possible_to_merge = stake_accounts_to_fund
                        .iter()
                        .map(|f| f.into())
                        .collect::<Vec<CollectedStakeAccount>>();
                    let non_mergeable = prepare_merge_instructions(
                        possible_to_merge.iter().collect(),
                        destination_stake,
                        destination_stake_state_type,
                        &settlement_record.settlement_address,
                        Some(&settlement_record.vote_account_address),
                        program,
                        config_address,
                        &withdrawer_authority,
                        &mut transaction_builder,
                        &clock,
                        &stake_history,
                    )
                    .await?;
                    validator_bonds_funders.extend(non_mergeable.into_iter().map(
                        |stake_account_address| SettlementFunderValidatorBond {
                            stake_account_to_fund: stake_account_address,
                        },
                    ));

                    if lamports_available < amount_to_fund {
                        let err_msg = format!(
                                "Cannot fully fund settlement {} (vote account {}, funder: ValidatorBond) with {} SOLs as only {} SOLs were found in stake accounts",
                                settlement_record.settlement_address,
                                settlement_record.vote_account_address,
                                lamports_to_sol(amount_to_fund),
                                lamports_to_sol(lamports_available)
                            );
                        reporting.add_error_string(err_msg);
                        reporting.reportable.funded_overall += lamports_available;
                    } else if lamports_available > amount_to_fund + minimal_stake_lamports {
                        // we are in situation that the stake account has got (or it will have after merging)
                        // more lamports than needed for funding the settlement in current loop iteration,
                        // the rest of lamports from the stake account can be used for other settlements,
                        // the lamports will be available under split stake account after the stake account is funded (next for-loop section)
                        // let's use the split stake account as a source for funding of next settlement
                        // NOTE: this process is "important" if the same vote account is used for multiple settlements,
                        //       and we want to utilize the maximum funding spreading over multiple settlements
                        // WARN: this REQUIRES that the merge bond transactions are executed in sequence!
                        let lamports_available_after_split = lamports_available
                            .saturating_sub(amount_to_fund)
                            .saturating_sub(minimal_stake_lamports);
                        funding_stake_accounts.push(FundBondStakeAccount {
                            lamports: lamports_available_after_split,
                            stake_account: destination_split_stake.pubkey(),
                            split_stake_account: Arc::new(Keypair::new()),
                            state: destination_stake_state,
                        });
                        reporting.reportable.funded_overall += amount_to_fund;
                    }
                } else {
                    let err_msg = format!(
                            "Cannot find stake account to fund settlement {} (vote account {}, funder: ValidatorBond)",
                            settlement_record.settlement_address,
                            settlement_record.vote_account_address,
                        );
                    reporting.add_error_string(err_msg);
                }
                reporting.reportable.funded_already += settlement_record
                    .max_total_claim
                    .saturating_sub(amount_to_fund);
            }
        }
    }

    let execution_result = execute_in_sequence(
        rpc_client.clone(),
        transaction_executor.clone(),
        &mut transaction_builder,
        priority_fee_policy,
    )
    .await;
    reporting.add_tx_execution_result(execution_result, "Stake accounts management");

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn fund_settlements(
    program: &Program<Arc<DynSigner>>,
    rpc_client: Arc<RpcClient>,
    transaction_executor: Arc<TransactionExecutor>,
    settlement_records: &mut Vec<SettlementRecord>,
    config_address: &Pubkey,
    fee_payer: Arc<Keypair>,
    operator_authority: Arc<Keypair>,
    marinade_wallet: Arc<Keypair>,
    rent_payer: Arc<Keypair>,
    minimal_stake_lamports: u64,
    priority_fee_policy: &PriorityFeePolicy,
    reporting: &mut ReportHandler<InitSettlementReport>,
) -> anyhow::Result<()> {
    let mut transaction_builder = TransactionBuilder::limited(fee_payer.clone());
    transaction_builder.add_signer_checked(&operator_authority);
    transaction_builder.add_signer_checked(&rent_payer);
    transaction_builder.add_signer_checked(&marinade_wallet);

    let (withdrawer_authority, _) = find_bonds_withdrawer_authority(config_address);

    // WARN: the prior processing REQUIRES that the fund bond transactions are executed in sequence
    for settlement_record in settlement_records {
        if !settlement_record.state.is_in_progress(reporting) {
            continue;
        }
        match &settlement_record.funder {
            SettlementFunderType::Marinade(Some(SettlementFunderMarinade { amount_to_fund })) => {
                let new_stake_account_keypair = Arc::new(Keypair::new());
                transaction_builder.add_signer_checked(&new_stake_account_keypair);
                info!(
                    "Settlement:{}, creating marinade stake account {}",
                    settlement_record.settlement_address,
                    new_stake_account_keypair.pubkey()
                );
                let instructions = create_stake_account(
                    &marinade_wallet.pubkey(),
                    &new_stake_account_keypair.pubkey(),
                    &Authorized {
                        withdrawer: withdrawer_authority,
                        staker: settlement_record.settlement_staker_authority,
                    },
                    &Lockup {
                        unix_timestamp: 0,
                        epoch: 0,
                        custodian: withdrawer_authority,
                    },
                    // after claiming the rest has to be still living stake account
                    amount_to_fund + minimal_stake_lamports,
                );
                transaction_builder.add_instructions(instructions)?;
                transaction_builder.finish_instruction_pack();
                reporting
                    .reportable
                    .funded_settlements_overall
                    .insert(settlement_record.settlement_address);
            }
            SettlementFunderType::ValidatorBond(validator_bonds_funders) => {
                for SettlementFunderValidatorBond {
                    stake_account_to_fund,
                    ..
                } in validator_bonds_funders
                {
                    // Settlement funding could be of two types: from validator bond or from operator wallet
                    let split_stake_account_keypair = Arc::new(Keypair::new());
                    let req = program
                        .request()
                        .accounts(validator_bonds::accounts::FundSettlement {
                            config: *config_address,
                            bond: settlement_record.bond_address,
                            stake_account: *stake_account_to_fund,
                            bonds_withdrawer_authority: withdrawer_authority,
                            operator_authority: operator_authority.pubkey(),
                            settlement: settlement_record.settlement_address,
                            system_program: system_program::ID,
                            settlement_staker_authority: settlement_record
                                .settlement_staker_authority,
                            rent: rent_sysvar_id,
                            split_stake_account: split_stake_account_keypair.pubkey(),
                            split_stake_rent_payer: rent_payer.pubkey(),
                            stake_history: stake_history_sysvar_id,
                            clock: clock_sysvar_id,
                            stake_program: stake_program_id,
                            program: validator_bonds_id,
                            event_authority: find_event_authority().0,
                        })
                        .args(validator_bonds::instruction::FundSettlement {});
                    transaction_builder.add_signer_checked(&split_stake_account_keypair);
                    add_instruction_to_builder(
                        &mut transaction_builder,
                        &req,
                        format!(
                            "FundSettlement: {}, stake: {}",
                            settlement_record.settlement_address, stake_account_to_fund,
                        ),
                    )?;
                    reporting
                        .reportable
                        .funded_settlements_overall
                        .insert(settlement_record.settlement_address);
                }
            }
            _ => {
                // reason should be already part of reporting here
                error!(
                    "Not possible to fund settlement {} (vote account {}) with funder type {:?}",
                    settlement_record.settlement_address,
                    settlement_record.vote_account_address,
                    settlement_record.funder
                );
            }
        }
    }

    let execute_result = execute_in_sequence(
        rpc_client.clone(),
        transaction_executor.clone(),
        &mut transaction_builder,
        priority_fee_policy,
    )
    .await;
    reporting.add_tx_execution_result(execute_result, "FundSettlements");

    Ok(())
}

#[derive(Clone)]
struct FundBondStakeAccount {
    lamports: u64,
    stake_account: Pubkey,
    split_stake_account: Arc<Keypair>,
    state: StakeStateV2,
}

impl From<&FundBondStakeAccount> for CollectedStakeAccount {
    fn from(fund_bond_stake_account: &FundBondStakeAccount) -> Self {
        (
            fund_bond_stake_account.stake_account,
            fund_bond_stake_account.lamports,
            fund_bond_stake_account.state,
        )
    }
}

/// Filtering stake accounts and creating a Map of vote account to stake accounts
async fn get_on_chain_bond_stake_accounts(
    rpc_client: Arc<RpcClient>,
    stake_accounts: &CollectedStakeAccounts,
    withdrawer_authority: &Pubkey,
) -> Result<HashMap<Pubkey, Vec<FundBondStakeAccount>>, CliError> {
    let non_funded: CollectedStakeAccounts = stake_accounts
        .clone()
        .into_iter()
        .filter(|(_, _, stake)| {
            if let Some(authorized) = stake.authorized() {
                authorized.staker == *withdrawer_authority
                    && authorized.withdrawer == *withdrawer_authority
            } else {
                false
            }
        })
        .collect();
    let non_funded_delegated_stakes =
        obtain_delegated_stake_accounts(rpc_client.clone(), non_funded)
            .await
            .map_err(CliError::RetryAble)?;

    // creating a map of vote account to stake accounts
    let result_map = non_funded_delegated_stakes
        .into_iter()
        .map(|(vote_account, stake_accounts)| {
            (
                vote_account,
                stake_accounts
                    .into_iter()
                    .map(|(stake_account, lamports, state)| FundBondStakeAccount {
                        lamports,
                        stake_account,
                        split_stake_account: Arc::new(Keypair::new()),
                        state,
                    })
                    .collect(),
            )
        })
        .collect::<HashMap<Pubkey, Vec<FundBondStakeAccount>>>();
    Ok(result_map)
}

#[derive(Debug)]
struct SettlementFunderValidatorBond {
    stake_account_to_fund: Pubkey,
}

#[derive(Debug)]
struct SettlementFunderMarinade {
    amount_to_fund: u64,
}

#[derive(Debug)]
enum SettlementFunderType {
    Marinade(Option<SettlementFunderMarinade>),
    ValidatorBond(Vec<SettlementFunderValidatorBond>),
}

impl SettlementFunderType {
    fn new(settlement_funder: &SettlementFunder) -> Self {
        match settlement_funder {
            SettlementFunder::Marinade => SettlementFunderType::Marinade(None),
            SettlementFunder::ValidatorBond => SettlementFunderType::ValidatorBond(vec![]),
        }
    }
}

#[derive(Debug)]
enum SettlementRecordState {
    InProgress,
    AlreadyFunded,
}

impl SettlementRecordState {
    fn is_in_progress(&self, reporting: &mut ReportHandler<InitSettlementReport>) -> bool {
        match self {
            SettlementRecordState::InProgress => true,
            SettlementRecordState::AlreadyFunded => {
                reporting.reportable.funded_settlements_already += 1;
                false
            }
        }
    }
}

#[derive(Debug)]
struct SettlementRecord {
    vote_account_address: Pubkey,
    bond_address: Pubkey,
    bond_account: Option<Bond>,
    settlement_address: Pubkey,
    settlement_account: Option<Settlement>,
    settlement_staker_authority: Pubkey,
    merkle_root: [u8; 32],
    max_total_claim: u64,
    max_merkle_nodes: u64,
    funder: SettlementFunderType,
    state: SettlementRecordState,
}

struct InitSettlementReport {
    rpc_client: Option<Arc<RpcClient>>,
    json_settlements_count: u64,
    json_settlements_max_claim_sum: u64,
    json_max_merkle_nodes_sum: u64,
    // settlement_address, vote_account_address
    created_settlements: Vec<(Pubkey, Pubkey)>,
    epoch: u64,
    funded_overall: u64,
    funded_already: u64,
    funded_settlements_overall: HashSet<Pubkey>,
    funded_settlements_already: u64,
}

impl PrintReportable for InitSettlementReport {
    fn get_report(&self) -> Pin<Box<dyn Future<Output = Vec<String>> + '_>> {
        Box::pin(async {
            let rpc_client = if let Some(rpc_client) = &self.rpc_client {
                rpc_client
            } else {
                return vec![];
            };
            let settlement_claim_rent = rpc_client
                .get_minimum_balance_for_rent_exemption(SETTLEMENT_CLAIM_ACCOUNT_SIZE)
                .await
                .map_or_else(
                    |e| {
                        error!("Error fetching SettlementClaim account rent: {:?}", e);
                        0_u64
                    },
                    |v| v,
                );

            vec![
                format!(
                    "InitSettlement (epoch: {}): created {}/{} settlements",
                    self.epoch,
                    self.created_settlements.len(),
                    self.json_settlements_count
                ),
                format!("InitSettlement funded {}/{} settlements with {}/{} SOLs (before this already funded {} settlements with {} SOLs)",
                    self.funded_settlements_overall.len(),
                    self.json_settlements_count,
                    lamports_to_sol(self.funded_overall),
                    lamports_to_sol(self.json_settlements_max_claim_sum),
                    self.funded_settlements_already,
                    lamports_to_sol(self.funded_already),
                ),
                format!("InitSettlement number of loaded settlements merkle nodes {}, expected rent for settlement claims {} SOLs",
                    self.json_max_merkle_nodes_sum,
                    lamports_to_sol(self.json_max_merkle_nodes_sum * settlement_claim_rent),
                ),
            ]
        })
    }
}

impl InitSettlementReport {
    fn report_handler() -> ReportHandler<Self> {
        let init_settlement_report = Self {
            rpc_client: None,
            created_settlements: vec![],
            json_settlements_count: 0,
            json_settlements_max_claim_sum: 0,
            json_max_merkle_nodes_sum: 0,
            epoch: 0,
            funded_overall: 0,
            funded_already: 0,
            funded_settlements_overall: HashSet::new(),
            funded_settlements_already: 0,
        };
        ReportHandler::new(init_settlement_report)
    }

    fn init(
        &mut self,
        rpc_client: Arc<RpcClient>,
        epoch: u64,
        json_settlements: &Vec<SettlementRecord>,
    ) {
        self.rpc_client = Some(rpc_client);
        self.json_settlements_count = json_settlements.len() as u64;
        self.json_settlements_max_claim_sum =
            json_settlements.iter().map(|s| s.max_total_claim).sum();
        self.json_max_merkle_nodes_sum = json_settlements.iter().map(|s| s.max_merkle_nodes).sum();
        self.epoch = epoch;
    }

    fn add_created_settlement(&mut self, settlement_record: &SettlementRecord) {
        self.created_settlements.push((
            settlement_record.settlement_address,
            settlement_record.vote_account_address,
        ));
    }

    fn list_created_settlements(&self) -> String {
        self.created_settlements
            .iter()
            .map(|(s, v)| format!("{}/{}", s, v))
            .collect::<Vec<String>>()
            .join(", ")
    }
}
