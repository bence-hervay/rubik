use crate::facelet::Facelet;

use super::FaceletArray;

#[derive(Clone, Debug, Default)]
pub struct ByteArray {
    data: Vec<u8>,
}

impl ByteArray {
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl FaceletArray for ByteArray {
    fn with_len(len: usize, fill: Facelet) -> Self {
        Self {
            data: vec![fill.as_u8(); len],
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
