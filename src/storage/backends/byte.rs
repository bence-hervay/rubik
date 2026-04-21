use crate::facelet::Facelet;
use crate::threading::default_thread_count;

use super::{init, FaceletArray, StoragePtr};

#[derive(Clone, Debug, Default)]
pub struct Byte {
    data: Vec<u8>,
}

impl Byte {
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl FaceletArray for Byte {
    type RawStorage = StoragePtr<u8>;

    fn with_len(len: usize, fill: Facelet) -> Self {
        Self::with_len_with_threads(len, fill, default_thread_count())
    }

    fn with_len_with_threads(len: usize, fill: Facelet, thread_count: usize) -> Self {
        Self {
            data: init::filled_vec(len, fill.as_u8(), thread_count),
        }
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn bits_per_facelet() -> usize {
        8
    }

    fn get(&self, index: usize) -> Facelet {
        Facelet::from_u8(self.data[index])
    }

    fn set(&mut self, index: usize, value: Facelet) {
        self.data[index] = value.as_u8();
    }

    fn fill(&mut self, value: Facelet) {
        self.fill_with_threads(value, default_thread_count());
    }

    fn fill_with_threads(&mut self, value: Facelet, thread_count: usize) {
        init::fill_slice(&mut self.data, value.as_u8(), thread_count);
    }

    fn storage_unit_range(index: usize) -> (usize, usize) {
        (index, index)
    }

    fn raw_storage(&mut self) -> Self::RawStorage {
        StoragePtr::new(self.data.as_mut_ptr())
    }

    #[inline(always)]
    unsafe fn get_unchecked_raw(&self, index: usize) -> u8 {
        *self.data.get_unchecked(index)
    }

    #[inline(always)]
    unsafe fn set_unchecked_raw(&mut self, index: usize, value: u8) {
        *self.data.get_unchecked_mut(index) = value;
    }

    #[inline(always)]
    unsafe fn get_unchecked_raw_from(storage: Self::RawStorage, index: usize) -> u8 {
        *storage.ptr().add(index)
    }

    #[inline(always)]
    unsafe fn set_unchecked_raw_in(storage: Self::RawStorage, index: usize, value: u8) {
        *storage.ptr().add(index) = value;
    }

    #[inline(always)]
    unsafe fn get_unchecked(&self, index: usize) -> Facelet {
        Facelet::from_u8(*self.data.get_unchecked(index))
    }

    #[inline(always)]
    unsafe fn set_unchecked(&mut self, index: usize, value: Facelet) {
        *self.data.get_unchecked_mut(index) = value.as_u8();
    }

    fn read_block(&self, start: usize, out: &mut [Facelet]) {
        assert!(start <= self.data.len());
        assert!(out.len() <= self.data.len() - start);
        let len = out.len();

        for (dst, src) in out.iter_mut().zip(&self.data[start..start + len]) {
            *dst = Facelet::from_u8(*src);
        }
    }

    fn write_block(&mut self, start: usize, src: &[Facelet]) {
        assert!(start <= self.data.len());
        assert!(src.len() <= self.data.len() - start);

        for (dst, value) in self.data[start..start + src.len()]
            .iter_mut()
            .zip(src.iter().copied())
        {
            *dst = value.as_u8();
        }
    }
}
