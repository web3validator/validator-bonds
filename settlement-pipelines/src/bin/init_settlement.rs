use anyhow::anyhow;
use clap::Parser;
use log::{debug, error, info};
use settlement_engine::merkle_tree_collection::MerkleTreeCollection;
use settlement_engine::settlement_claims::{SettlementCollection, SettlementFunder};
use settlement_engine::utils::read_from_json_file;
use settlement_pipelines::anchor::add_instructions_to_builder_from_anchor;
use settlement_pipelines::arguments::GlobalOpts;
use settlement_pipelines::arguments::{
    init_from_opts, InitializedGlobalOpts, PriorityFeePolicyOpts, TipPolicyOpts,
};
use settlement_pipelines::init::init_log;
use settlement_pipelines::json_data::{resolve_combined, MerkleTreeMetaSettlement};
use settlement_pipelines::STAKE_ACCOUNT_RENT_EXEMPTION;
use solana_sdk::native_token::lamports_to_sol;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use solana_sdk::stake::instruction::create_account as create_stake_account;
use solana_sdk::stake::program::ID as stake_program_id;
use solana_sdk::stake::state::{Authorized, Lockup};
use solana_sdk::system_program;
use solana_sdk::sysvar::{
    clock::ID as clock_sysvar_id, rent::ID as rent_sysvar_id,
    stake_history::ID as stake_history_sysvar_id,
};
use solana_transaction_builder::TransactionBuilder;
use solana_transaction_builder_executor::{
    builder_to_execution_data, execute_transactions_in_parallel, execute_transactions_in_sequence,
};
use solana_transaction_executor::{
    SendTransactionWithGrowingTipProvider, TransactionExecutorBuilder,
};
use std::collections::HashMap;
use std::io;
use std::rc::Rc;
use std::sync::Arc;
use validator_bonds::instructions::{InitSettlementArgs, MergeStakeArgs};
use validator_bonds::state::bond::Bond;
use validator_bonds::state::config::find_bonds_withdrawer_authority;
use validator_bonds::state::settlement::{find_settlement_staker_authority, Settlement};
use validator_bonds::ID as validator_bonds_id;
use validator_bonds_common::config::get_config;
use validator_bonds_common::stake_accounts::{
    collect_stake_accounts, obtain_delegated_stake_accounts,
    obtain_funded_stake_accounts_for_settlement, CollectedStakeAccounts,
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

    #[arg(
        short = 'm',
        long,
        env,
        help = "Merkle tree collection file path generated by settlement-engine as of --output-merkle-tree-collection"
    )]
    input_merkle_tree_collection: String,

    #[arg(
        short = 's',
        long,
        env,
        help = "Settlement collection file path generated by settlement-engine as of --output-settlement-collection"
    )]
    input_settlement_collection: String,

    /// forcing epoch, overriding from the settlement collection
    #[arg(long)]
    epoch: Option<u64>,

    #[clap(flatten)]
    priority_fee_policy_opts: PriorityFeePolicyOpts,

    #[clap(flatten)]
    tip_policy_opts: TipPolicyOpts,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Args = Args::parse();
    init_log(&args.global_opts);

    let config_address = args.global_opts.config;
    info!(
        "Loading merkle tree at: '{}', validator-bonds config: {}",
        args.input_merkle_tree_collection, config_address
    );
    let combined_collection = {
        let merkle_tree_collection: MerkleTreeCollection =
            read_from_json_file(&args.input_merkle_tree_collection).map_err(|e| {
                anyhow!(
                    "Cannot read merkle tree collection from file '{}': {:?}",
                    args.input_merkle_tree_collection,
                    e
                )
            })?;
        let settlement_collection: SettlementCollection =
            read_from_json_file(&args.input_settlement_collection).map_err(|e| {
                anyhow!(
                    "Cannot read settlement collection from file '{}': {:?}",
                    args.input_settlement_collection,
                    e
                )
            })?;
        resolve_combined(merkle_tree_collection, settlement_collection)
    }?;

    let InitializedGlobalOpts {
        rpc_url,
        fee_payer_keypair,
        fee_payer_pubkey,
        operator_authority_keypair,
        priority_fee_policy,
        tip_policy,
        rpc_client,
        program,
    } = init_from_opts(
        &args.global_opts,
        &args.priority_fee_policy_opts,
        &args.tip_policy_opts,
    )?;

    let epoch = args.epoch.unwrap_or(combined_collection.epoch);

    let mut vote_accounts: Vec<Pubkey> = Vec::new();
    let mut transaction_builder = TransactionBuilder::limited(fee_payer_keypair.clone());
    transaction_builder.add_signer_checked(&operator_authority_keypair.clone());

    let config = get_config(rpc_client.clone(), config_address).await?;
    let minimal_stake_lamports = config.minimum_stake_lamports + STAKE_ACCOUNT_RENT_EXEMPTION;

    let mut init_settlement_errors: Vec<String> = vec![];

    // verify what are the settlement accounts that we need to create
    // (not to pushing many RPC calls to the network, squeezing them to less)
    let mut settlement_records = combined_collection
        .merkle_tree_settlements
        .iter()
        .filter(|MerkleTreeMetaSettlement { merkle_tree, .. }| {
            if merkle_tree.merkle_root.is_some() {
                true
            } else {
                panic!(
                    "Cannot create settlement for vote account {} without a merkle root",
                    merkle_tree.vote_account
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
                    &config_address,
                    &merkle_tree.vote_account,
                );
                let (settlement_address, _) =
                    validator_bonds::state::settlement::find_settlement_address(
                        &bond_address,
                        &merkle_root.to_bytes(),
                        epoch,
                    );
                Ok(SettlementRecord {
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
                })
            },
        )
        .collect::<Result<Vec<SettlementRecord>, anyhow::Error>>()?;

    // loading accounts from on-chain
    let settlement_addresses: Vec<Pubkey> = settlement_records
        .iter()
        .map(|d| d.settlement_address)
        .collect();
    let settlements =
        get_settlements_for_pubkeys(rpc_client.clone(), &settlement_addresses).await?;
    for (record, (pubkey, settlement)) in settlement_records.iter_mut().zip(settlements.into_iter())
    {
        assert_eq!(
            record.settlement_address, pubkey,
            "Mismatched settlement address"
        ); // sanity check
        record.settlement_account = settlement;
    }
    let bond_addresses: Vec<Pubkey> = settlement_records.iter().map(|d| d.bond_address).collect();
    let bonds = get_bonds_for_pubkeys(rpc_client.clone(), &bond_addresses).await?;
    for (record, (pubkey, bond)) in settlement_records.iter_mut().zip(bonds.into_iter()) {
        assert_eq!(record.bond_address, pubkey, "Mismatched bond address"); // sanity check
        record.bond_account = bond;
    }

    for settlement_record in &mut settlement_records {
        if settlement_record.bond_account.is_none() {
            let err_msg = format!(
                "Cannot find bond account {} for vote account {}, claim amount {}",
                settlement_record.bond_address,
                settlement_record.vote_account_address,
                settlement_record.max_total_claim
            );
            error!("{}", err_msg);
            init_settlement_errors.push(err_msg);
            continue;
        }
        if settlement_record.settlement_account.is_some() {
            debug!(
                "Settlement account {} already exists, skipping initialization",
                settlement_record.settlement_address
            );
        } else {
            vote_accounts.push(settlement_record.vote_account_address);
            let req = program
                .request()
                .accounts(validator_bonds::accounts::InitSettlement {
                    config: config_address,
                    bond: settlement_record.bond_address,
                    operator_authority: operator_authority_keypair.pubkey(),
                    system_program: system_program::ID,
                    rent_payer: fee_payer_pubkey,
                    program: validator_bonds_id,
                    settlement: settlement_record.settlement_address,
                    event_authority: find_event_authority().0,
                })
                .args(validator_bonds::instruction::InitSettlement {
                    init_settlement_args: InitSettlementArgs {
                        merkle_root: settlement_record.merkle_root,
                        rent_collector: fee_payer_pubkey,
                        max_total_claim: settlement_record.max_total_claim,
                        max_merkle_nodes: settlement_record.max_merkle_nodes,
                        epoch,
                    },
                });
            add_instructions_to_builder_from_anchor(&mut transaction_builder, &req).map_err(
                |e| {
                    anyhow!(
                        "Cannot add init settlement instruction to transaction builder: {:?}",
                        e
                    )
                },
            )?;
        }
    }

    let transaction_executor_builder = TransactionExecutorBuilder::new()
        .with_default_providers(rpc_client.clone())
        .with_send_transaction_provider(SendTransactionWithGrowingTipProvider {
            rpc_url: rpc_url.clone(),
            query_param: "tip".into(),
            tip_policy,
        });
    let transaction_executor = Arc::new(transaction_executor_builder.build());

    let init_execution_count = transaction_builder.instructions().len();
    let execution_data = builder_to_execution_data(
        rpc_url.clone(),
        &mut transaction_builder,
        Some(priority_fee_policy.clone()),
    );
    execute_transactions_in_parallel(
        transaction_executor.clone(),
        execution_data,
        Some(100_usize),
    )
    .await?;
    info!(
        "InitSettlement instructions {} executed successfully of settlement/vote_account [{}]",
        init_execution_count,
        settlement_records
            .iter()
            .map(|v| format!("{}/{}", v.settlement_address, v.vote_account_address))
            .collect::<Vec<String>>()
            .join(", ")
    );
    assert_eq!(
        transaction_builder.instructions().len(),
        0,
        "Expected to get all instructions from builder processed"
    );

    let mut transaction_builder = TransactionBuilder::limited(fee_payer_keypair.clone());
    transaction_builder.add_signer_checked(&operator_authority_keypair.clone());

    // let's check how we are about settlement funding
    let (withdrawer_authority, _) = find_bonds_withdrawer_authority(&config_address);
    let stake_accounts =
        collect_stake_accounts(rpc_client.clone(), Some(&withdrawer_authority), None).await?;
    let non_funded: CollectedStakeAccounts = stake_accounts
        .clone()
        .into_iter()
        .filter(|(_, _, stake)| {
            if let Some(authorized) = stake.authorized() {
                authorized.staker == withdrawer_authority
                    && authorized.withdrawer == withdrawer_authority
            } else {
                false
            }
        })
        .collect();
    let non_funded_delegated_stakes =
        obtain_delegated_stake_accounts(non_funded, rpc_client.clone()).await?;
    let funded_to_settlement_stakes = obtain_funded_stake_accounts_for_settlement(
        stake_accounts,
        &config_address,
        settlement_addresses,
        rpc_client.clone(),
    )
    .await?;
    // creating a map of vote account to stake accounts3
    #[derive(Clone)]
    struct FundBondStakeAccount {
        lamports: u64,
        stake_account: Pubkey,
        split_stake_account: Rc<Keypair>,
    }
    let mut fund_bond_stake_accounts: HashMap<Pubkey, Vec<FundBondStakeAccount>> =
        non_funded_delegated_stakes
            .into_iter()
            .map(|(vote_account, stake_accounts)| {
                (
                    vote_account,
                    stake_accounts
                        .into_iter()
                        .map(|(stake_account, lamports, _)| FundBondStakeAccount {
                            lamports,
                            stake_account,
                            split_stake_account: Rc::new(Keypair::new()),
                        })
                        .collect(),
                )
            })
            .collect();

    // to print to std out at the end
    let mut funded_lamports_overall: u64 = 0;

    // Merging stake accounts to fit for validator bonds funding
    for settlement_record in &mut settlement_records {
        if settlement_record.bond_account.is_none() {
            // not bond account to work with
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
                "Settlement {} (vote account {}), funter {:?} already funded by {}, skipping funding",
                settlement_record.settlement_address,
                settlement_record.vote_account_address,
                settlement_record.funder,
                settlement_amount_funded
            );
            settlement_record.state = SettlementRecordState::AlreadyFunded;
            continue;
        }

        match settlement_record.funder {
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
                funded_lamports_overall += amount_to_fund;
            }
            SettlementFunderType::ValidatorBond(_) => {
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

                let stake_account_to_fund =
                    if stake_accounts_to_fund.is_empty() || lamports_available == 0 {
                        None
                    } else {
                        Some(stake_accounts_to_fund.remove(0))
                    };
                if let Some(FundBondStakeAccount {
                    stake_account: destination_stake,
                    split_stake_account: destination_split_stake,
                    ..
                }) = stake_account_to_fund
                {
                    info!(
                        "Settlement: {} (vote account {}) will be funded with {} stake accounts merged into {}",
                        settlement_record.settlement_address,
                        settlement_record.vote_account_address,
                        stake_accounts_to_fund.len() + 1,
                        destination_stake
                    );
                    for merge_stake_account in stake_accounts_to_fund {
                        let req = program
                            .request()
                            .accounts(validator_bonds::accounts::MergeStake {
                                config: config_address,
                                stake_history: stake_history_sysvar_id,
                                clock: clock_sysvar_id,
                                source_stake: merge_stake_account.stake_account,
                                destination_stake,
                                staker_authority: withdrawer_authority,
                                stake_program: stake_program_id,
                                program: validator_bonds_id,
                                event_authority: find_event_authority().0,
                            })
                            .args(validator_bonds::instruction::MergeStake {
                                merge_args: MergeStakeArgs {
                                    settlement: settlement_record.settlement_address,
                                },
                            });
                        add_instructions_to_builder_from_anchor(&mut transaction_builder, &req)
                            .map_err(|e| {
                                anyhow!(
                                    "Cannot add merge stake instruction to transaction builder: {:?}",
                                    e
                                )
                            })?;
                    }
                    if lamports_available < amount_to_fund {
                        let err_msg = format!(
                            "Cannot fully fund settlement {} (vote account {}) with {} SOLs as only {} SOLs were found in stake accounts",
                            settlement_record.settlement_address,
                            settlement_record.vote_account_address,
                            lamports_to_sol(amount_to_fund),
                            lamports_to_sol(lamports_available)
                        );
                        error!("{}", err_msg);
                        init_settlement_errors.push(err_msg);
                        funded_lamports_overall += lamports_available;
                    } else if lamports_available > amount_to_fund + minimal_stake_lamports {
                        // we are in situation that the stake account has got (or it will have after merging)
                        // more lamports than needed for funding the settlement in current loop iteration,
                        // the rest of lamports from the stake account can be used for other settlements,
                        // the lamports will be available under split stake account after the stake account is funded (next for-loop section)
                        // let's use the split stake account as a source for funding of next settlement
                        // NOTE: this process is "important" if the same vote account is used for multiple settlements,
                        //       and we want to utilize the maximum funding spreading over multiple settlements
                        // WARN: this REQUIRES that the fund bond transactions are executed in sequence!
                        let lamports_available_after_split =
                            lamports_available - amount_to_fund - minimal_stake_lamports;
                        funding_stake_accounts.push(FundBondStakeAccount {
                            lamports: lamports_available_after_split,
                            stake_account: destination_split_stake.pubkey(),
                            split_stake_account: Rc::new(Keypair::new()),
                        });
                        funded_lamports_overall += amount_to_fund;
                    }
                    settlement_record.funder =
                        SettlementFunderType::ValidatorBond(Some(SettlementFunderValidatorBond {
                            stake_account_to_fund: destination_stake,
                        }))
                } else {
                    let err_msg = format!(
                        "Cannot find stake account to fund settlement {} of vote account {}",
                        settlement_record.settlement_address,
                        settlement_record.vote_account_address
                    );
                    error!("{}", err_msg);
                    init_settlement_errors.push(err_msg);
                }
            }
        }
    }

    let merge_execution_count = transaction_builder.instructions().len();
    let execution_data = builder_to_execution_data(
        rpc_url.clone(),
        &mut transaction_builder,
        Some(priority_fee_policy.clone()),
    );
    execute_transactions_in_sequence(transaction_executor.clone(), execution_data).await?;
    info!(
        "Stake accounts management instructions {} executed successfully",
        merge_execution_count
    );
    assert_eq!(
        transaction_builder.instructions().len(),
        0,
        "Expected to get all instructions from builder processed"
    );

    let mut transaction_builder = TransactionBuilder::limited(fee_payer_keypair.clone());
    transaction_builder.add_signer_checked(&operator_authority_keypair.clone());

    // Funding settlements
    let mut funded_settlements_overall = 0u32;
    // WARN: the prior processing REQUIRES that the fund bond transactions are executed in sequence
    for settlement_record in &settlement_records {
        if !settlement_record.state.is_in_progress() {
            continue;
        }
        match settlement_record.funder {
            SettlementFunderType::Marinade(Some(SettlementFunderMarinade { amount_to_fund })) => {
                let new_stake_account_keypair = Rc::new(Keypair::new());
                transaction_builder.add_signer_checked(&new_stake_account_keypair);
                info!(
                    "Settlement:{}, creating marinade stake account {}",
                    settlement_record.settlement_address,
                    new_stake_account_keypair.pubkey()
                );
                let instructions = create_stake_account(
                    &fee_payer_keypair.pubkey(),
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
                funded_settlements_overall += 1;
            }
            SettlementFunderType::ValidatorBond(Some(SettlementFunderValidatorBond {
                stake_account_to_fund,
                ..
            })) => {
                // Settlement funding could be of two types: from validator bond or from operator wallet
                let split_stake_account_keypair = Rc::new(Keypair::new());
                let req = program
                    .request()
                    .accounts(validator_bonds::accounts::FundSettlement {
                        config: config_address,
                        bond: settlement_record.bond_address,
                        stake_account: stake_account_to_fund,
                        bonds_withdrawer_authority: withdrawer_authority,
                        operator_authority: operator_authority_keypair.pubkey(),
                        settlement: settlement_record.settlement_address,
                        system_program: system_program::ID,
                        settlement_staker_authority: settlement_record.settlement_staker_authority,
                        rent: rent_sysvar_id,
                        split_stake_account: split_stake_account_keypair.pubkey(),
                        split_stake_rent_payer: fee_payer_pubkey,
                        stake_history: stake_history_sysvar_id,
                        clock: clock_sysvar_id,
                        stake_program: stake_program_id,
                        program: validator_bonds_id,
                        event_authority: find_event_authority().0,
                    })
                    .args(validator_bonds::instruction::FundSettlement {});
                transaction_builder.add_signer_checked(&split_stake_account_keypair);
                add_instructions_to_builder_from_anchor(&mut transaction_builder, &req).map_err(
                    |e| {
                        anyhow!(
                            "Cannot add merge stake instruction to transaction builder: {:?}",
                            e
                        )
                    },
                )?;
                funded_settlements_overall += 1;
            }
            _ => {
                error!(
                    "Not possible to fund settlement {} (vote account {}) with funder type {:?}",
                    settlement_record.settlement_address,
                    settlement_record.vote_account_address,
                    settlement_record.funder
                );
            }
        }
    }

    let fund_settlement_execution_count = transaction_builder.instructions().len();
    let execution_data = builder_to_execution_data(
        rpc_url.clone(),
        &mut transaction_builder,
        Some(priority_fee_policy.clone()),
    );
    execute_transactions_in_sequence(transaction_executor.clone(), execution_data).await?;
    info!(
        "FundSettlement instructions {} executed successfully",
        fund_settlement_execution_count
    );
    assert_eq!(
        transaction_builder.instructions().len(),
        0,
        "Expected to get all instructions from builder processed"
    );

    println!("For epoch {epoch}: JSON loaded {} settlements with {} SOLs, this run funded {} settlements with {} SOLs; {} errors",
        settlement_records.len(),
        lamports_to_sol(settlement_records.iter().map(|s| s.max_total_claim).sum::<u64>()),
        funded_settlements_overall,
        lamports_to_sol(funded_lamports_overall),
        init_settlement_errors.len(),
    );

    if !init_settlement_errors.is_empty() {
        serde_json::to_writer(io::stdout(), &init_settlement_errors)?;
        return Err(anyhow!(
            "{} errors during initialization of settlements",
            init_settlement_errors.len(),
        ));
    }

    Ok(())
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
    ValidatorBond(Option<SettlementFunderValidatorBond>),
}

impl SettlementFunderType {
    fn new(settlement_funder: &SettlementFunder) -> Self {
        match settlement_funder {
            SettlementFunder::Marinade => SettlementFunderType::Marinade(None),
            SettlementFunder::ValidatorBond => SettlementFunderType::ValidatorBond(None),
        }
    }
}

#[derive(Debug)]
enum SettlementRecordState {
    InProgress,
    AlreadyFunded,
}

impl SettlementRecordState {
    fn is_in_progress(&self) -> bool {
        matches!(self, SettlementRecordState::InProgress)
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
