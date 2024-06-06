use crate::error::ErrorCode as BondErrorCode;
use anchor_lang::prelude::Pubkey;
use anchor_lang::prelude::*;
use anchor_lang::Discriminator;
use std::fmt::Debug;

const BITS_PER_BYTE: u8 = 8;

// 8 + mem::size_of::<SettlementClaims>(): 8 + 32 + 1 + 8 = 49 bytes
// Anchor aligns to 8 bytes, so the size is 56 bytes
// TODO: place into constants
pub const SETTLEMENT_CLAIMS_HEADER_SIZE: usize = 56;

// TODO: need to verify that using #account does not break the data management
// hoping it does anything with the data after the account part
// #[derive(Default, Clone, AnchorSerialize, AnchorDeserialize, BorshSchema, Debug)]

/// Simple bitmap to deduplicate settlement claims
/// The bitmap structure stores the first index (0) in the most left bit of the most left (first) byte.
#[account()]
#[derive(Debug)]
pub struct SettlementClaims {
    pub settlement: Pubkey,
    pub version: u8,
    pub max_records: u64,
    // data are remaining space in Account, not touched by Anchor to not exceed 32KB on heap data
    // https://github.com/solana-developers/anchor-zero-copy-example/tree/main?tab=readme-ov-file#explanation-of-solana-memory-and-zero-copy
    // data: &mut [u8],
}

pub struct SettlementClaimsWithData<'a> {
    pub max_records: u64,
    pub data: &'a mut [u8],
}

impl<'a> SettlementClaimsWithData<'a> {
    pub fn new(max_records: u64, data: &mut [u8]) -> SettlementClaimsWithData {
        SettlementClaimsWithData { max_records, data }
    }

    /// account has got correct discriminator
    /// and has enough space for bitmap (no more Solana account space allocation is needed)
    pub fn is_initialized(&self) -> bool {
        msg!(
            "self.data.len(): {}, self.account_size(): {}",
            self.data.len(),
            self.account_size()
        );
        self.data[0..8] == SettlementClaims::DISCRIMINATOR && self.data.len() >= self.account_size()
    }

    pub fn assert_initialized(&self) -> Result<()> {
        require!(self.is_initialized(), ErrorCode::AccountNotInitialized);
        Ok(())
    }

    pub fn is_set(&self, index: u64) -> Result<bool> {
        self.verify_index(index)?;

        let (byte_index, bit_index) = Self::bitmap_byte_and_bit(index);
        let bitmap_byte = self.bitmap_byte(byte_index);
        Ok(bitmap_byte & (1 << bit_index) != 0)
    }

    pub fn set(&mut self, index: u64) -> Result<()> {
        self.verify_index(index)?;
        self.set_inner(index)
    }

    pub fn try_to_set(&mut self, index: u64) -> Result<bool> {
        if self.is_set(index)? {
            Ok(false)
        } else {
            // validity of index is checked in is_set
            self.set_inner(index)?;
            Ok(true)
        }
    }

    // TODO: maybe better to print only part of last byte not whole one
    pub fn debug_string(&self) -> String {
        if !self.is_initialized() {
            return "SettlementClaims: not initialized".to_string();
        }
        let data_to_format: &[u8] = if self.data.len() < self.account_size() {
            msg!(
                "max records: {:?}, data size {} smaller to expected account size {}",
                self.max_records,
                self.data.len(),
                self.account_size()
            );
            &self.data[SETTLEMENT_CLAIMS_HEADER_SIZE..]
        } else {
            &self.data[SETTLEMENT_CLAIMS_HEADER_SIZE..self.account_size()]
        };

        let formatted_data = data_to_format
            .iter()
            // using reverse bits to not format byte as "00000001" but as "10000000"
            .map(|b| format!("{:08b},", b.reverse_bits()))
            .collect::<Vec<String>>();
        format!(
            "max records: {}, data: {}",
            self.max_records,
            formatted_data.join(",")
        )
    }

    pub fn number_of_set_bits(&self) -> Result<u64> {
        self.assert_initialized()?;
        let bitmap_bytes = &self.data[SETTLEMENT_CLAIMS_HEADER_SIZE..self.account_size()];
        // calculating number of bits(!) that are set to 1 in the bitmap_bytes slice
        // TODO: would be possible to somehow work with u8 and then only for summing things together works with u64?
        Ok(bitmap_bytes
            .iter()
            .map(|byte| byte.count_ones() as u64)
            .sum::<u64>())
    }

    /// required size of bytes of Solana account with bitmap of particular size
    fn account_size(&self) -> usize {
        settlement_claims_account_size(self.max_records)
    }

    fn set_inner(&mut self, index: u64) -> Result<()> {
        let (byte_index, bit_index) = Self::bitmap_byte_and_bit(index);
        msg!(
            "set inner, index: {}, byte_index: {}, bit_index: {}",
            index,
            byte_index,
            bit_index
        );
        let old_byte = self.bitmap_byte_mut(byte_index);
        let new_byte = *old_byte | 1_u8 << bit_index;
        *old_byte = new_byte;

        msg!(
            "set inner, new value: {}, new byte: {}",
            self.debug_string(),
            format!("{:08b},", new_byte.reverse_bits())
        );
        Ok(())
    }

    fn bitmap_byte(&self, byte_index: usize) -> &u8 {
        &self.data[SETTLEMENT_CLAIMS_HEADER_SIZE + byte_index]
    }

    fn bitmap_byte_mut(&mut self, byte_index: usize) -> &mut u8 {
        &mut self.data[SETTLEMENT_CLAIMS_HEADER_SIZE + byte_index]
    }

    fn bitmap_byte_and_bit(index: u64) -> (usize, u8) {
        let byte_index = index / BITS_PER_BYTE as u64;
        let bit_index = index % BITS_PER_BYTE as u64;
        (byte_index as usize, bit_index as u8)
    }

    fn verify_index(&self, index: u64) -> Result<()> {
        self.assert_initialized()?;
        require_gt!(
            self.max_records,
            index,
            BondErrorCode::ClaimingIndexOutOfBounds
        );
        Ok(())
    }
}

/// required size of bytes of Solana account with bitmap of particular size
pub fn settlement_claims_account_size(max_records: u64) -> usize {
    SETTLEMENT_CLAIMS_HEADER_SIZE + settlement_claims_bitmap_size(max_records)
}

/// number of bytes required for the bitmap to store the given number of records
fn settlement_claims_bitmap_size(max_records: u64) -> usize {
    let byte_number = max_records / BITS_PER_BYTE as u64;
    let byte_modulo = max_records % BITS_PER_BYTE as u64;
    if byte_modulo == 0 {
        // msg!("bitmap size records: {}, modulo 0: {}", max_records, byte_number);
        byte_number as usize
    } else {
        // msg!("bitmap size records: {}, modulo >0: {}", max_records, byte_number + 1);
        byte_number as usize + 1
    }
}

// TODO: add some tests on bitmap operations
