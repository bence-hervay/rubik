use crate::facelet::Facelet;

use super::FaceletArray;

#[derive(Clone, Debug, Default)]
pub struct Base6Array {
    len: usize,
    data: Vec<u8>,
}

impl Base6Array {
    const FACELETS_PER_BYTE: usize = 3;
    const BASE: u8 = 6;

    pub fn capacity_bytes(&self) -> usize {
        self.data.len()
    }

    pub fn as_packed_slice(&self) -> &[u8] {
        &self.data
    }

    fn byte_and_slot(index: usize) -> (usize, usize) {
        (
            index / Self::FACELETS_PER_BYTE,
            index % Self::FACELETS_PER_BYTE,
        )
    }

    fn place_value(slot: usize) -> u8 {
        match slot {
            0 => 1,
            1 => Self::BASE,
            2 => Self::BASE * Self::BASE,
            _ => unreachable!("base6 slot must be 0, 1, or 2"),
        }
    }
}

impl FaceletArray for Base6Array {
    fn with_len(len: usize, fill: Facelet) -> Self {
        let mut this = Self {
            len,
            data: vec![0; len.div_ceil(Self::FACELETS_PER_BYTE)],
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
        len.div_ceil(Self::FACELETS_PER_BYTE)
    }

    fn get(&self, index: usize) -> Facelet {
        assert!(index < self.len);

        let (byte, slot) = Self::byte_and_slot(index);
        let place = Self::place_value(slot);
        let raw = (self.data[byte] / place) % Self::BASE;
        Facelet::from_u8(raw)
    }

    fn set(&mut self, index: usize, value: Facelet) {
        assert!(index < self.len);

        let (byte, slot) = Self::byte_and_slot(index);
        let place = Self::place_value(slot);
        let old = (self.data[byte] / place) % Self::BASE;
        self.data[byte] = self.data[byte] - old * place + value.as_u8() * place;
    }
}
