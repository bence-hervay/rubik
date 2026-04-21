mod backends;
mod facelet_array;
mod init;

pub use backends::{Byte, Byte3, Nibble, ThreeBit};
pub use facelet_array::{FaceletArray, StoragePtr};

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

    fn threaded_initialization<A: FaceletArray>() {
        for len in [0usize, 1, 2, 3, 4, 5, 7, 21, 22, 63, 64, 65, 129, 20_000] {
            for fill in Facelet::ALL {
                for thread_count in [1usize, 2, 16] {
                    let array = A::with_len_with_threads(len, fill, thread_count);

                    assert_eq!(array.len(), len);
                    for index in 0..len {
                        assert_eq!(
                            array.get(index),
                            fill,
                            "initialization mismatch at index {index}, len {len}, threads {thread_count}"
                        );
                    }
                }
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
    fn byte_roundtrips() {
        roundtrip::<Byte>();
        threaded_initialization::<Byte>();
    }

    #[test]
    fn byte3_roundtrips() {
        roundtrip::<Byte3>();
        threaded_initialization::<Byte3>();
    }

    #[test]
    fn byte3_stores_three_facelets_per_byte() {
        for len in 0..16 {
            let array = Byte3::with_len(len, Facelet::White);
            assert_eq!(array.capacity_bytes(), len.div_ceil(3));
            assert_eq!(Byte3::storage_bytes_for_len(len), len.div_ceil(3));
        }
    }

    #[test]
    fn nibble_roundtrips() {
        roundtrip::<Nibble>();
        threaded_initialization::<Nibble>();
    }

    #[test]
    fn three_bit_roundtrips() {
        roundtrip::<ThreeBit>();
        threaded_initialization::<ThreeBit>();
    }

    #[test]
    fn packed_initialization_leaves_unused_storage_zeroed() {
        let nibble = Nibble::with_len_with_threads(3, Facelet::Blue, 16);
        assert_eq!(nibble.as_packed_slice()[1] >> 4, 0);

        let byte3_one_extra = Byte3::with_len_with_threads(4, Facelet::Blue, 16);
        assert_eq!(byte3_one_extra.as_packed_slice()[1], Facelet::Blue.as_u8());

        let byte3_two_extra = Byte3::with_len_with_threads(5, Facelet::Blue, 16);
        assert_eq!(
            byte3_two_extra.as_packed_slice()[1],
            Facelet::Blue.as_u8() + Facelet::Blue.as_u8() * 6
        );

        let three_bit = ThreeBit::with_len_with_threads(1, Facelet::Blue, 16);
        assert_eq!(three_bit.as_packed_words()[0] & !0b111, 0);
    }

    #[test]
    fn storage_backends_agree_after_random_updates() {
        let len = 257;
        let mut rng = XorShift64::new(0x51A7E_F00D);
        let mut reference = vec![Facelet::White; len];
        let mut byte = Byte::with_len(len, Facelet::White);
        let mut byte3 = Byte3::with_len(len, Facelet::White);
        let mut nibble = Nibble::with_len(len, Facelet::White);
        let mut three_bit = ThreeBit::with_len(len, Facelet::White);

        for _ in 0..10_000 {
            let index = (rng.next_u64() as usize) % len;
            let value = Facelet::from_u8((rng.next_u64() % 6) as u8);

            reference[index] = value;
            byte.set(index, value);
            byte3.set(index, value);
            nibble.set(index, value);
            three_bit.set(index, value);
        }

        assert_array_matches_reference(&byte, &reference);
        assert_array_matches_reference(&byte3, &reference);
        assert_array_matches_reference(&nibble, &reference);
        assert_array_matches_reference(&three_bit, &reference);
    }

    #[test]
    fn storage_byte_estimates_are_exact() {
        for len in 0usize..200 {
            assert_eq!(Byte::storage_bytes_for_len(len), len);
            assert_eq!(Byte3::storage_bytes_for_len(len), len.div_ceil(3));
            assert_eq!(Nibble::storage_bytes_for_len(len), len.div_ceil(2));
            assert_eq!(
                ThreeBit::storage_bytes_for_len(len),
                len.checked_mul(3).unwrap().div_ceil(64) * 8
            );

            let byte = Byte::with_len(len, Facelet::White);
            let byte3 = Byte3::with_len(len, Facelet::White);
            let nibble = Nibble::with_len(len, Facelet::White);
            let three_bit = ThreeBit::with_len(len, Facelet::White);

            assert_eq!(byte.as_slice().len(), Byte::storage_bytes_for_len(len));
            assert_eq!(byte3.capacity_bytes(), Byte3::storage_bytes_for_len(len));
            assert_eq!(nibble.capacity_bytes(), Nibble::storage_bytes_for_len(len));
            assert_eq!(
                three_bit.capacity_words() * core::mem::size_of::<u64>(),
                ThreeBit::storage_bytes_for_len(len)
            );
        }
    }
}
