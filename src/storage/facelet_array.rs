use crate::facelet::Facelet;

pub trait FaceletArray: Clone + core::fmt::Debug + Send {
    /// Backend-specific raw storage handle used by chunked threaded moves.
    type RawStorage: Copy + Send;

    fn with_len(len: usize, fill: Facelet) -> Self
    where
        Self: Sized;

    fn with_len_with_threads(len: usize, fill: Facelet, thread_count: usize) -> Self
    where
        Self: Sized,
    {
        assert!(thread_count > 0, "thread count must be greater than zero");
        Self::with_len(len, fill)
    }

    fn len(&self) -> usize;

    fn bits_per_facelet() -> usize
    where
        Self: Sized;

    fn storage_bytes_for_len(len: usize) -> usize
    where
        Self: Sized,
    {
        len.checked_mul(Self::bits_per_facelet())
            .and_then(|bits| bits.checked_add(7))
            .map(|bits| bits / 8)
            .expect("facelet storage byte estimate overflowed usize")
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(&self, index: usize) -> Facelet;

    fn set(&mut self, index: usize, value: Facelet);

    /// Inclusive range of raw storage units touched by reading or writing one
    /// facelet. Packed backends return the packed byte/word containing `index`.
    fn storage_unit_range(index: usize) -> (usize, usize)
    where
        Self: Sized;

    /// Raw handle to the backing storage. Threaded moves use this only after
    /// splitting line chunks on storage-unit boundaries.
    fn raw_storage(&mut self) -> Self::RawStorage;

    /// # Safety
    ///
    /// `index` must be in bounds for this storage.
    #[inline]
    unsafe fn get_unchecked_raw(&self, index: usize) -> u8 {
        self.get(index).as_u8()
    }

    /// # Safety
    ///
    /// `index` must be in bounds for this storage and `value` must encode a valid facelet.
    #[inline]
    unsafe fn set_unchecked_raw(&mut self, index: usize, value: u8) {
        self.set(index, Facelet::from_u8(value));
    }

    /// # Safety
    ///
    /// `index` must be in bounds for the storage used to create `storage`.
    unsafe fn get_unchecked_raw_from(storage: Self::RawStorage, index: usize) -> u8
    where
        Self: Sized;

    /// # Safety
    ///
    /// `index` must be in bounds for the storage used to create `storage`, and
    /// `value` must encode a valid facelet. Concurrent callers must operate on
    /// storage-safe disjoint chunks.
    unsafe fn set_unchecked_raw_in(storage: Self::RawStorage, index: usize, value: u8)
    where
        Self: Sized;

    /// # Safety
    ///
    /// `index` must be in bounds for this storage.
    #[inline]
    unsafe fn get_unchecked(&self, index: usize) -> Facelet {
        Facelet::from_u8(self.get_unchecked_raw(index))
    }

    /// # Safety
    ///
    /// `index` must be in bounds for this storage.
    #[inline]
    unsafe fn set_unchecked(&mut self, index: usize, value: Facelet) {
        self.set_unchecked_raw(index, value.as_u8());
    }

    fn fill(&mut self, value: Facelet) {
        for i in 0..self.len() {
            self.set(i, value);
        }
    }

    fn fill_with_threads(&mut self, value: Facelet, thread_count: usize) {
        assert!(thread_count > 0, "thread count must be greater than zero");
        self.fill(value);
    }

    fn swap(&mut self, a: usize, b: usize) {
        let av = self.get(a);
        let bv = self.get(b);
        self.set(a, bv);
        self.set(b, av);
    }

    fn read_block(&self, start: usize, out: &mut [Facelet]) {
        assert!(start <= self.len());
        assert!(out.len() <= self.len() - start);

        for (offset, slot) in out.iter_mut().enumerate() {
            *slot = self.get(start + offset);
        }
    }

    fn write_block(&mut self, start: usize, src: &[Facelet]) {
        assert!(start <= self.len());
        assert!(src.len() <= self.len() - start);

        for (offset, value) in src.iter().copied().enumerate() {
            self.set(start + offset, value);
        }
    }
}

#[derive(Debug)]
pub struct StoragePtr<T> {
    ptr: *mut T,
}

impl<T> Copy for StoragePtr<T> {}

impl<T> Clone for StoragePtr<T> {
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<T> Send for StoragePtr<T> {}

impl<T> StoragePtr<T> {
    pub(crate) fn new(ptr: *mut T) -> Self {
        Self { ptr }
    }

    pub(crate) fn ptr(self) -> *mut T {
        self.ptr
    }
}
