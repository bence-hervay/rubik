use crate::facelet::Facelet;

use super::FaceletArray;

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

    fn byte_and_shift(index: usize) -> (usize, u8) {
        let byte = index / 2;
        let shift = if index % 2 == 0 { 0 } else { 4 };
        (byte, shift)
    }
}

impl FaceletArray for Nibble {
    fn with_len(len: usize, fill: Facelet) -> Self {
        let mut this = Self {
            len,
            data: vec![0; len.div_ceil(2)],
        };
        this.fill(fill);
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
}
