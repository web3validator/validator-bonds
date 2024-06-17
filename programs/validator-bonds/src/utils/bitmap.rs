use crate::error::ErrorCode;
use anchor_lang::prelude::*;

const BITS_PER_BYTE: u8 = 8;

pub struct BitmapProjection(pub u64);

impl BitmapProjection {
    pub fn check_size(max_records: u64, data: &[u8]) -> Result<()> {
        require_gte!(
            data.len(),
            Self::bitmap_size_in_bytes(max_records),
            ErrorCode::BitmapSizeMismatch
        );
        Ok(())
    }

    pub fn is_set(&self, index: u64, data: &[u8]) -> Result<bool> {
        self.verify_index(index)?;
        let (byte_index, bit_mod) = Self::bitmap_byte_index_and_bit_mod(index);
        let bitmap_byte = self.bitmap_byte(byte_index, data);
        Ok(bitmap_byte & (1 << bit_mod) != 0)
    }

    pub fn set(&mut self, index: u64, data: &mut [u8]) -> Result<()> {
        self.verify_index(index)?;
        self.set_inner(index, data)
    }

    pub fn try_to_set(&mut self, index: u64, data: &mut [u8]) -> Result<bool> {
        if self.is_set(index, data)? {
            Ok(false)
        } else {
            // validity of index is checked in is_set
            self.set_inner(index, data)?;
            Ok(true)
        }
    }

    /// calculating number of bits(!) that are set to 1 in the bitmap_bytes slice
    pub fn number_of_bits(&self, data: &[u8]) -> Result<u64> {
        Ok(data
            .iter()
            .map(|byte| byte.count_ones() as u64)
            .sum::<u64>())
    }

    fn set_inner(&mut self, index: u64, data: &mut [u8]) -> Result<()> {
        let (byte_index, bit_mod) = Self::bitmap_byte_index_and_bit_mod(index);
        let old_byte = self.bitmap_byte_mut(byte_index, data);
        let new_byte = *old_byte | 1_u8 << bit_mod;
        *old_byte = new_byte;
        Ok(())
    }

    fn bitmap_byte<'a>(&self, byte_index: usize, data: &'a [u8]) -> &'a u8 {
        &data[byte_index]
    }

    fn bitmap_byte_mut<'a>(&mut self, byte_index: usize, data: &'a mut [u8]) -> &'a mut u8 {
        &mut data[byte_index]
    }

    fn bitmap_byte_index_and_bit_mod(index: u64) -> (usize, u8) {
        let byte_index = index / BITS_PER_BYTE as u64;
        let bit_mod = index % BITS_PER_BYTE as u64;
        (byte_index as usize, bit_mod as u8)
    }

    /// number of bytes required for the bitmap to store the given number of records
    /// every record consumes 1 bit in the bitmap
    pub fn bitmap_size_in_bytes(max_records: u64) -> usize {
        let (byte_index, bit_mod) = Self::bitmap_byte_index_and_bit_mod(max_records);
        if bit_mod == 0 {
            byte_index
        } else {
            byte_index + 1_usize
        }
    }

    pub fn debug_string(&self, data: &[u8]) -> String {
        let (last_byte_index, last_bit) = Self::bitmap_byte_index_and_bit_mod(self.0);
        let mut formatted_data =
                // stripping last byte only to include the bitmap data limited by max_records
                data[..last_byte_index + 1]
                .iter()
                // using reverse bits to format byte as "00000001" but as "10000000"
                .map(|b| format!("{:08b}", b.reverse_bits()))
                .collect::<Vec<String>>();
        formatted_data[last_byte_index] =
            (&formatted_data[last_byte_index][..last_bit as usize]).to_string();
        formatted_data.join(",")
    }

    fn verify_index(&self, index: u64) -> Result<()> {
        require_gt!(self.0, index, ErrorCode::BitmapIndexOutOfBonds);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitmap_size_in_bytes() {
        assert_eq!(BitmapProjection::bitmap_size_in_bytes(0), 0);
        assert_eq!(BitmapProjection::bitmap_size_in_bytes(1), 1);
        assert_eq!(BitmapProjection::bitmap_size_in_bytes(7), 1);
        assert_eq!(BitmapProjection::bitmap_size_in_bytes(8), 1);
        assert_eq!(BitmapProjection::bitmap_size_in_bytes(9), 2);
        assert_eq!(BitmapProjection::bitmap_size_in_bytes(15), 2);
        assert_eq!(BitmapProjection::bitmap_size_in_bytes(16), 2);
        assert_eq!(BitmapProjection::bitmap_size_in_bytes(17), 3);
        assert_eq!(
            BitmapProjection::bitmap_size_in_bytes(u64::MAX),
            (u64::MAX / 8 + 1) as usize
        );
    }
}
