use crate::facelet::Facelet;

use super::{init, FaceletArray};

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

    #[inline(always)]
    fn bit_offset(index: usize) -> usize {
        index
            .checked_mul(3)
            .expect("three_bit bit offset overflowed usize")
    }

    #[inline(always)]
    fn bit_offset_unchecked(index: usize) -> usize {
        index * 3
    }

    fn filled_word(word_index: usize, fill: Facelet) -> u64 {
        let raw = fill.as_u8() & 0b111;
        let mut word = 0u64;
        let mut raw_bit = word_index % 3;

        for bit in 0..64 {
            if (raw >> raw_bit) & 1 == 1 {
                word |= 1u64 << bit;
            }

            raw_bit += 1;
            if raw_bit == 3 {
                raw_bit = 0;
            }
        }

        word
    }

    fn clear_unused_bits(&mut self) {
        let used_bits = self
            .len
            .checked_mul(3)
            .expect("three_bit total bit length overflowed usize")
            % 64;

        if used_bits == 0 {
            return;
        }

        let mask = (1u64 << used_bits) - 1;
        let last = self
            .words
            .last_mut()
            .expect("non-zero bit remainder must have a storage word");
        *last &= mask;
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
            words: init::initialized_vec(word_count, |index| Self::filled_word(index, fill)),
        };
        this.clear_unused_bits();
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

    fn fill(&mut self, value: Facelet) {
        init::initialize_slice(&mut self.words, |index| Self::filled_word(index, value));
        self.clear_unused_bits();
    }

    #[inline(always)]
    unsafe fn get_unchecked_raw(&self, index: usize) -> u8 {
        let bit = Self::bit_offset_unchecked(index);
        let word = bit / 64;
        let shift = bit % 64;

        let raw = if shift <= 61 {
            (*self.words.get_unchecked(word) >> shift) & 0b111
        } else {
            let low = *self.words.get_unchecked(word) >> shift;
            let high_bits = shift + 3 - 64;
            let high = *self.words.get_unchecked(word + 1) & ((1u64 << high_bits) - 1);
            low | (high << (64 - shift))
        };

        raw as u8
    }

    #[inline(always)]
    unsafe fn set_unchecked_raw(&mut self, index: usize, value: u8) {
        let bit = Self::bit_offset_unchecked(index);
        let word = bit / 64;
        let shift = bit % 64;
        let raw = (value & 0b111) as u64;

        if shift <= 61 {
            let slot = self.words.get_unchecked_mut(word);
            let mask = !(0b111u64 << shift);
            *slot = (*slot & mask) | (raw << shift);
        } else {
            let low_bits = 64 - shift;
            let high_bits = 3 - low_bits;

            let low_part_mask = (1u64 << low_bits) - 1;
            let low_mask = !(low_part_mask << shift);
            let low_slot = self.words.get_unchecked_mut(word);
            *low_slot = (*low_slot & low_mask) | ((raw & low_part_mask) << shift);

            let high_part_mask = (1u64 << high_bits) - 1;
            let high_mask = !high_part_mask;
            let high_slot = self.words.get_unchecked_mut(word + 1);
            *high_slot = (*high_slot & high_mask) | (raw >> low_bits);
        }
    }

    #[inline(always)]
    unsafe fn get_unchecked(&self, index: usize) -> Facelet {
        Facelet::from_u8(self.get_unchecked_raw(index))
    }

    #[inline(always)]
    unsafe fn set_unchecked(&mut self, index: usize, value: Facelet) {
        self.set_unchecked_raw(index, value.as_u8());
    }
}
