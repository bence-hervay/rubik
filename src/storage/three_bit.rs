use crate::facelet::Facelet;

use super::FaceletArray;

#[derive(Clone, Debug, Default)]
pub struct ThreeBit {
    len: usize,
    words: Vec<u64>,
}

impl ThreeBit {
    pub fn capacity_words(&self) -> usize {
        self.words.len()
    }

    pub fn as_packed_words(&self) -> &[u64] {
        &self.words
    }

    fn bit_offset(index: usize) -> usize {
        index
            .checked_mul(3)
            .expect("three_bit bit offset overflowed usize")
    }
}

impl FaceletArray for ThreeBit {
    fn with_len(len: usize, fill: Facelet) -> Self {
        let total_bits = len
            .checked_mul(3)
            .expect("three_bit total bit length overflowed usize");
        let word_count = total_bits.div_ceil(64);

        let mut this = Self {
            len,
            words: vec![0; word_count],
        };
        this.fill(fill);
        this
    }

    fn len(&self) -> usize {
        self.len
    }

    fn bits_per_facelet() -> usize {
        3
    }

    fn storage_bytes_for_len(len: usize) -> usize {
        len.checked_mul(3)
            .expect("three_bit total bit length overflowed usize")
            .div_ceil(64)
            .checked_mul(8)
            .expect("three_bit storage byte estimate overflowed usize")
    }

    fn get(&self, index: usize) -> Facelet {
        assert!(index < self.len);

        let bit = Self::bit_offset(index);
        let word = bit / 64;
        let shift = bit % 64;

        let raw = if shift <= 61 {
            (self.words[word] >> shift) & 0b111
        } else {
            let low = self.words[word] >> shift;
            let high_bits = shift + 3 - 64;
            let high = self.words[word + 1] & ((1u64 << high_bits) - 1);
            low | (high << (64 - shift))
        };

        Facelet::from_u8(raw as u8)
    }

    fn set(&mut self, index: usize, value: Facelet) {
        assert!(index < self.len);

        let bit = Self::bit_offset(index);
        let word = bit / 64;
        let shift = bit % 64;
        let raw = (value.as_u8() & 0b111) as u64;

        if shift <= 61 {
            let mask = !(0b111u64 << shift);
            self.words[word] = (self.words[word] & mask) | (raw << shift);
        } else {
            let low_bits = 64 - shift;
            let high_bits = 3 - low_bits;

            let low_part_mask = (1u64 << low_bits) - 1;
            let low_mask = !(low_part_mask << shift);
            self.words[word] = (self.words[word] & low_mask) | ((raw & low_part_mask) << shift);

            let high_part_mask = (1u64 << high_bits) - 1;
            let high_mask = !high_part_mask;
            self.words[word + 1] = (self.words[word + 1] & high_mask) | (raw >> low_bits);
        }
    }
}
