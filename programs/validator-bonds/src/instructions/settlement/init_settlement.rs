use crate::error::ErrorCode;
use crate::events::settlement::InitSettlementEvent;
use crate::state::bond::Bond;
use crate::state::config::Config;
use crate::state::settlement::{find_settlement_staker_authority, Bumps, Settlement};
use crate::state::settlement_claims::{
    account_size as settlement_claims_account_size, SettlementClaims,
};
use anchor_lang::prelude::*;

#[derive(AnchorDeserialize, AnchorSerialize)]
pub struct InitSettlementArgs {
    /// merkle root for this settlement, multiple settlements can be created with the same merkle root,
    /// settlements will be distinguished by the vote_account
    pub merkle_root: [u8; 32],
    /// maximal number of lamports that can be claimed from this settlement
    pub max_total_claim: u64,
    /// maximal number of merkle tree nodes that can be claimed from this settlement
    pub max_merkle_nodes: u64,
    /// collects the rent exempt from the settlement account when closed
    pub rent_collector: Pubkey,
    /// epoch that the settlement is created for
    pub epoch: u64,
}

/// Creates settlement account for the bond.
/// Permission-ed for operator authority.
#[event_cpi]
#[derive(Accounts)]
#[instruction(params: InitSettlementArgs)]
pub struct InitSettlement<'info> {
    #[account(
        has_one = operator_authority @ ErrorCode::InvalidOperatorAuthority,
    )]
    pub config: Account<'info, Config>,

    #[account(
        has_one = config @ ErrorCode::ConfigAccountMismatch,
        seeds = [
            b"bond_account",
            config.key().as_ref(),
            bond.vote_account.as_ref()
        ],
        bump = bond.bump,
    )]
    pub bond: Account<'info, Bond>,

    #[account(
        init,
        payer = rent_payer,
        space = 8 + std::mem::size_of::<Settlement>(),
        seeds = [
            b"settlement_account",
            bond.key().as_ref(),
            params.merkle_root.as_ref(),
            params.epoch.to_le_bytes().as_ref(),
        ],
        bump,
    )]
    pub settlement: Account<'info, Settlement>,

    // TODO: it could be needed to have a chance to increase size of the account
    //       Solana maximum allocation size in one instruction is 10KB (~80K records)
    #[account(
        init,
        payer = rent_payer,
        space = settlement_claims_account_size(params.max_merkle_nodes),
        seeds = [
            b"claims_account",
            settlement.key().as_ref(),
        ],
        bump,
    )]
    pub settlement_claims: Account<'info, SettlementClaims>,

    /// operator signer authority that is allowed to create the settlement account
    pub operator_authority: Signer<'info>,

    /// rent exempt payer of account creation
    #[account(
        mut,
        owner = system_program.key(),
    )]
    pub rent_payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

impl<'info> InitSettlement<'info> {
    pub fn process(
        ctx: Context<InitSettlement>,
        InitSettlementArgs {
            merkle_root,
            rent_collector,
            max_total_claim,
            max_merkle_nodes,
            epoch,
        }: InitSettlementArgs,
    ) -> Result<()> {
        require!(!ctx.accounts.config.paused, ErrorCode::ProgramIsPaused);

        if max_total_claim == 0 || max_merkle_nodes == 0 {
            return Err(error!(ErrorCode::EmptySettlementMerkleTree).with_values((
                "max_total_claim, max_merkle_nodes",
                format!("{}, {}", max_total_claim, max_merkle_nodes),
            )));
        }

        let clock = Clock::get()?;

        let (authority, authority_bump) =
            find_settlement_staker_authority(&ctx.accounts.settlement.key());
        ctx.accounts.settlement.set_inner(Settlement {
            bond: ctx.accounts.bond.key(),
            staker_authority: authority,
            merkle_root,
            max_total_claim,
            max_merkle_nodes,
            lamports_funded: 0,
            lamports_claimed: 0,
            merkle_nodes_claimed: 0,
            epoch_created_for: epoch,
            slot_created_at: clock.slot,
            rent_collector,
            split_rent_collector: None,
            split_rent_amount: 0,
            bumps: Bumps {
                pda: ctx.bumps.settlement,
                staker_authority: authority_bump,
                settlement_claims: ctx.bumps.settlement_claims,
            },
            reserved: [0; 90],
        });
        ctx.accounts.settlement_claims.set_inner(SettlementClaims {
            settlement: ctx.accounts.settlement.key(),
            version: 0,
            max_records: max_merkle_nodes,
        });
        emit_cpi!(InitSettlementEvent {
            settlement: ctx.accounts.settlement.key(),
            bond: ctx.accounts.settlement.bond,
            vote_account: ctx.accounts.bond.vote_account,
            staker_authority: ctx.accounts.settlement.staker_authority,
            merkle_root: ctx.accounts.settlement.merkle_root,
            max_total_claim: ctx.accounts.settlement.max_total_claim,
            max_merkle_nodes: ctx.accounts.settlement.max_merkle_nodes,
            epoch_created_for: ctx.accounts.settlement.epoch_created_for,
            slot_created_at: ctx.accounts.settlement.slot_created_at,
            rent_collector: ctx.accounts.settlement.rent_collector,
        });

        Ok(())
    }
}
