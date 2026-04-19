mod base6;
mod byte;
mod nibble;
mod packed3;
mod traits;

pub use base6::Base6Array;
pub use byte::ByteArray;
pub use nibble::NibbleArray;
pub use packed3::Packed3Array;
pub use traits::FaceletArray;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Facelet, RandomSource, XorShift64};

    fn roundtrip<A: FaceletArray>() {
        for len in 0..130 {
            let mut array = A::with_len(len, Facelet::White);

            for i in 0..len {
                array.set(i, Facelet::from_u8((i % 6) as u8));
            }

            for i in 0..len {
                assert_eq!(array.get(i), Facelet::from_u8((i % 6) as u8));
            }

            array.fill(Facelet::Blue);
            for i in 0..len {
                assert_eq!(array.get(i), Facelet::Blue);
            }
        }
    }

    fn assert_array_matches_reference<A: FaceletArray>(array: &A, reference: &[Facelet]) {
        assert_eq!(array.len(), reference.len());

        for (index, expected) in reference.iter().copied().enumerate() {
            assert_eq!(
                array.get(index),
                expected,
                "storage mismatch at index {index}"
            );
        }
    }

    #[test]
    fn byte_array_roundtrips() {
        roundtrip::<ByteArray>();
    }

    #[test]
    fn base6_array_roundtrips() {
        roundtrip::<Base6Array>();
    }

    #[test]
    fn base6_array_stores_three_facelets_per_byte() {
        for len in 0..16 {
            let array = Base6Array::with_len(len, Facelet::White);
            assert_eq!(array.capacity_bytes(), len.div_ceil(3));
            assert_eq!(Base6Array::storage_bytes_for_len(len), len.div_ceil(3));
        }
    }

    #[test]
    fn nibble_array_roundtrips() {
        roundtrip::<NibbleArray>();
    }

    #[test]
    fn packed3_array_roundtrips() {
        roundtrip::<Packed3Array>();
    }

    #[test]
    fn storage_backends_agree_after_random_updates() {
        let len = 257;
        let mut rng = XorShift64::new(0x51A7E_F00D);
        let mut reference = vec![Facelet::White; len];
        let mut byte = ByteArray::with_len(len, Facelet::White);
        let mut base6 = Base6Array::with_len(len, Facelet::White);
        let mut nibble = NibbleArray::with_len(len, Facelet::White);
        let mut packed3 = Packed3Array::with_len(len, Facelet::White);

        for _ in 0..10_000 {
            let index = (rng.next_u64() as usize) % len;
            let value = Facelet::from_u8((rng.next_u64() % 6) as u8);

            reference[index] = value;
            byte.set(index, value);
            base6.set(index, value);
            nibble.set(index, value);
            packed3.set(index, value);
        }

        assert_array_matches_reference(&byte, &reference);
        assert_array_matches_reference(&base6, &reference);
        assert_array_matches_reference(&nibble, &reference);
        assert_array_matches_reference(&packed3, &reference);
    }

    #[test]
    fn storage_byte_estimates_are_exact() {
        for len in 0usize..200 {
            assert_eq!(ByteArray::storage_bytes_for_len(len), len);
            assert_eq!(Base6Array::storage_bytes_for_len(len), len.div_ceil(3));
            assert_eq!(NibbleArray::storage_bytes_for_len(len), len.div_ceil(2));
            assert_eq!(
                Packed3Array::storage_bytes_for_len(len),
                len.checked_mul(3).unwrap().div_ceil(64) * 8
            );

            let byte = ByteArray::with_len(len, Facelet::White);
            let base6 = Base6Array::with_len(len, Facelet::White);
            let nibble = NibbleArray::with_len(len, Facelet::White);
            let packed3 = Packed3Array::with_len(len, Facelet::White);

            assert_eq!(byte.as_slice().len(), ByteArray::storage_bytes_for_len(len));
            assert_eq!(
                base6.capacity_bytes(),
                Base6Array::storage_bytes_for_len(len)
            );
            assert_eq!(
                nibble.capacity_bytes(),
                NibbleArray::storage_bytes_for_len(len)
            );
            assert_eq!(
                packed3.capacity_words() * core::mem::size_of::<u64>(),
                Packed3Array::storage_bytes_for_len(len)
            );
        }
    }
}
