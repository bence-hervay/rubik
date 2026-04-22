use crate::facelet::Facelet;

use super::{init, FaceletArray};

#[derive(Clone, Debug, Default)]
pub struct Nibble {
    len: usize,
    data: Vec<u8>,
}

impl Nibble {
    pub fn capacity_bytes(&self) -> usize {
        self.data.len()
    }

    pub fn as_packed_slice(&self) -> &[u8] {
        &self.data
    }

    #[inline(always)]
    fn byte_and_shift(index: usize) -> (usize, u8) {
        let byte = index / 2;
        let shift = if index % 2 == 0 { 0 } else { 4 };
        (byte, shift)
    }

    #[inline(always)]
    fn packed_byte(fill: Facelet) -> u8 {
        let raw = fill.as_u8();
        raw | (raw << 4)
    }

    fn clear_unused_slots(&mut self) {
        if self.len % 2 == 1 {
            let last = self
                .data
                .last_mut()
                .expect("odd non-zero length must have a storage byte");
            *last &= 0x0F;
        }
    }
}

impl FaceletArray for Nibble {
    fn with_len(len: usize, fill: Facelet) -> Self {
        let mut this = Self {
            len,
            data: init::filled_vec(len.div_ceil(2), Self::packed_byte(fill)),
        };
        this.clear_unused_slots();
        this
    }

    fn len(&self) -> usize {
        self.len
    }

    fn bits_per_facelet() -> usize {
        4
    }

    fn get(&self, index: usize) -> Facelet {
        assert!(index < self.len);

        let (byte, shift) = Self::byte_and_shift(index);
        let raw = (self.data[byte] >> shift) & 0x0F;
        Facelet::from_u8(raw)
    }

    fn set(&mut self, index: usize, value: Facelet) {
        assert!(index < self.len);

        let (byte, shift) = Self::byte_and_shift(index);
        let clear_mask = !(0x0Fu8 << shift);
        self.data[byte] = (self.data[byte] & clear_mask) | (value.as_u8() << shift);
    }

    fn fill(&mut self, value: Facelet) {
        init::fill_slice(&mut self.data, Self::packed_byte(value));
        self.clear_unused_slots();
    }

    #[inline(always)]
    unsafe fn get_unchecked_raw(&self, index: usize) -> u8 {
        let (byte, shift) = Self::byte_and_shift(index);
        (*self.data.get_unchecked(byte) >> shift) & 0x0F
    }

    #[inline(always)]
    unsafe fn set_unchecked_raw(&mut self, index: usize, value: u8) {
        let (byte, shift) = Self::byte_and_shift(index);
        let slot = self.data.get_unchecked_mut(byte);
        let clear_mask = !(0x0Fu8 << shift);
        *slot = (*slot & clear_mask) | (value << shift);
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
