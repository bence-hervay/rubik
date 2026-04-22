use crate::facelet::Facelet;

pub trait FaceletArray: Clone + core::fmt::Debug + Send {
    fn with_len(len: usize, fill: Facelet) -> Self
    where
        Self: Sized;

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
