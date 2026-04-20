use crate::facelet::Facelet;

use super::{init, FaceletArray, StoragePtr, DEFAULT_INITIALIZATION_THREAD_COUNT};

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
    type RawStorage = StoragePtr<u8>;

    fn with_len(len: usize, fill: Facelet) -> Self {
        Self::with_len_with_threads(len, fill, DEFAULT_INITIALIZATION_THREAD_COUNT)
    }

    fn with_len_with_threads(len: usize, fill: Facelet, thread_count: usize) -> Self {
        let mut this = Self {
            len,
            data: init::filled_vec(len.div_ceil(2), Self::packed_byte(fill), thread_count),
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
        self.fill_with_threads(value, DEFAULT_INITIALIZATION_THREAD_COUNT);
    }

    fn fill_with_threads(&mut self, value: Facelet, thread_count: usize) {
        init::fill_slice(&mut self.data, Self::packed_byte(value), thread_count);
        self.clear_unused_slots();
    }

    fn storage_unit_range(index: usize) -> (usize, usize) {
        let (byte, _) = Self::byte_and_shift(index);
        (byte, byte)
    }

    fn raw_storage(&mut self) -> Self::RawStorage {
        StoragePtr::new(self.data.as_mut_ptr())
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
    unsafe fn get_unchecked_raw_from(storage: Self::RawStorage, index: usize) -> u8 {
        let (byte, shift) = Self::byte_and_shift(index);
        (*storage.ptr().add(byte) >> shift) & 0x0F
    }

    #[inline(always)]
    unsafe fn set_unchecked_raw_in(storage: Self::RawStorage, index: usize, value: u8) {
        let (byte, shift) = Self::byte_and_shift(index);
        let slot = storage.ptr().add(byte);
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
