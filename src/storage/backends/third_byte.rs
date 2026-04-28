use crate::facelet::Facelet;

use super::{init, FaceletArray};

#[derive(Clone, Debug, Default)]
pub struct ThirdByte {
    len: usize,
    data: Vec<u8>,
}

impl ThirdByte {
    const FACELETS_PER_BYTE: usize = 3;
    const BASE: u8 = 6;

    pub fn capacity_bytes(&self) -> usize {
        self.data.len()
    }

    pub fn as_packed_slice(&self) -> &[u8] {
        &self.data
    }

    #[inline(always)]
    fn byte_and_slot(index: usize) -> (usize, usize) {
        (
            index / Self::FACELETS_PER_BYTE,
            index % Self::FACELETS_PER_BYTE,
        )
    }

    #[inline(always)]
    fn replace_slot(byte: &mut u8, slot: usize, value: u8) {
        match slot {
            0 => {
                let old = *byte % Self::BASE;
                *byte = *byte - old + value;
            }
            1 => {
                let old = (*byte / Self::BASE) % Self::BASE;
                *byte = *byte - old * Self::BASE + value * Self::BASE;
            }
            2 => {
                let place = Self::BASE * Self::BASE;
                let old = *byte / place;
                *byte = *byte - old * place + value * place;
            }
            _ => unreachable!("third_byte slot must be 0, 1, or 2"),
        }
    }

    #[inline(always)]
    fn packed_byte(fill: Facelet) -> u8 {
        let raw = fill.as_u8();
        raw + raw * Self::BASE + raw * Self::BASE * Self::BASE
    }

    fn clear_unused_slots(&mut self) {
        let Some(last) = self.data.last_mut() else {
            return;
        };

        let raw = *last;
        *last = match self.len % Self::FACELETS_PER_BYTE {
            0 => raw,
            1 => raw % Self::BASE,
            2 => raw % (Self::BASE * Self::BASE),
            _ => unreachable!("third_byte remainder must be 0, 1, or 2"),
        };
    }
}

impl FaceletArray for ThirdByte {
    fn with_len(len: usize, fill: Facelet) -> Self {
        let mut this = Self {
            len,
            data: init::filled_vec(
                len.div_ceil(Self::FACELETS_PER_BYTE),
                Self::packed_byte(fill),
            ),
        };
        this.clear_unused_slots();
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
        let packed = self.data[byte];
        let raw = match slot {
            0 => packed % Self::BASE,
            1 => (packed / Self::BASE) % Self::BASE,
            2 => packed / (Self::BASE * Self::BASE),
            _ => unreachable!("third_byte slot must be 0, 1, or 2"),
        };
        Facelet::from_u8(raw)
    }

    fn set(&mut self, index: usize, value: Facelet) {
        assert!(index < self.len);

        let (byte, slot) = Self::byte_and_slot(index);
        Self::replace_slot(&mut self.data[byte], slot, value.as_u8());
    }

    fn fill(&mut self, value: Facelet) {
        init::fill_slice(&mut self.data, Self::packed_byte(value));
        self.clear_unused_slots();
    }

    #[inline(always)]
    unsafe fn get_unchecked_raw(&self, index: usize) -> u8 {
        let (byte, slot) = Self::byte_and_slot(index);
        let packed = *self.data.get_unchecked(byte);
        match slot {
            0 => packed % Self::BASE,
            1 => (packed / Self::BASE) % Self::BASE,
            2 => packed / (Self::BASE * Self::BASE),
            _ => unreachable!("third_byte slot must be 0, 1, or 2"),
        }
    }

    #[inline(always)]
    unsafe fn set_unchecked_raw(&mut self, index: usize, value: u8) {
        let (byte, slot) = Self::byte_and_slot(index);
        let slot_byte = self.data.get_unchecked_mut(byte);
        Self::replace_slot(slot_byte, slot, value);
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
