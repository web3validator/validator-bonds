use std::fmt::Debug;
use anchor_lang::prelude::*;
use crate::error::ErrorCode;

const BITS_PER_BYTE: u8 = 8;

pub struct Bitmap<'a> {
    pub max_records: u64,
    pub data: &'a mut [u8],
}

impl<'a> Bitmap<'a> {
    pub fn new(max_records: u64, data: &mut [u8]) -> Self {
        Self::check_size(max_records, data)?;
        Self { max_records, data }
    }

    pub fn check_size(max_records: u64, data: &[u8]) -> Result<()> {
        require_gte!(
            data.len(),
            Self::bitmap_size_in_bytes(max_records),
            ErrorCode::BitmapSizeMismatch
        );
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

    /// number of bytes required for the bitmap to store the given number of records
    /// every record consumes 1 bit in the bitmap
    fn bitmap_size_in_bytes(max_records: u64) -> usize {
        let(byte_index, bit_index) = Self::bitmap_byte_and_bit(max_records);
        if byte_index == 0 {
            byte_index
        } else {
            byte_index + 1_usize
        }
    }

    fn verify_index(&self, index: u64) -> Result<()> {
        require_gt!(
            self.max_records,
            index,
            ErrorCode::BitmapIndexOutOfBonds
        );
        Ok(())
    }
}

impl Debug for Bitmap<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let formatted_data = self.data
            .iter()
            // using reverse bits to not format byte as "00000001" but as "10000000"
            .map(|b| format!("{:08b},", b.reverse_bits()))
            .collect::<Vec<String>>();
        // stripping last byte only to include the bitmap data limited by max_records
        let (last_byte_index, last_bit_index) = Self::bitmap_byte_and_bit(self.max_records);
        formatted_data.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_size_in_bytes() {
        assert_eq!(Bitmap::bitmap_size_in_bytes(0), 0);
        assert_eq!(Bitmap::bitmap_size_in_bytes(1), 1);
        assert_eq!(Bitmap::bitmap_size_in_bytes(7), 1);
        assert_eq!(Bitmap::bitmap_size_in_bytes(8), 1);
        assert_eq!(Bitmap::bitmap_size_in_bytes(9), 2);
        assert_eq!(Bitmap::bitmap_size_in_bytes(15), 2);
        assert_eq!(Bitmap::bitmap_size_in_bytes(16), 2);
        assert_eq!(Bitmap::bitmap_size_in_bytes(17), 3);
        assert_eq!(Bitmap::bitmap_size_in_bytes(u64::MAX), u64::MAX / 8 + 1);
    }
}
