mod backends;
mod facelet_array;
mod init;

pub use backends::{Byte, Nibble, ThirdByte, ThreeBit};
pub use facelet_array::FaceletArray;

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

    fn initialization<A: FaceletArray>() {
        for len in [0usize, 1, 2, 3, 4, 5, 7, 21, 22, 63, 64, 65, 129, 20_000] {
            for fill in Facelet::ALL {
                let array = A::with_len(len, fill);

                assert_eq!(array.len(), len);
                for index in 0..len {
                    assert_eq!(
                        array.get(index),
                        fill,
                        "initialization mismatch at index {index}, len {len}"
                    );
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
        initialization::<Byte>();
    }

    #[test]
    fn third_byte_roundtrips() {
        roundtrip::<ThirdByte>();
        initialization::<ThirdByte>();
    }

    #[test]
    fn third_byte_stores_three_facelets_per_byte() {
        for len in 0..16 {
            let array = ThirdByte::with_len(len, Facelet::White);
            assert_eq!(array.capacity_bytes(), len.div_ceil(3));
            assert_eq!(ThirdByte::storage_bytes_for_len(len), len.div_ceil(3));
        }
    }

    #[test]
    fn nibble_roundtrips() {
        roundtrip::<Nibble>();
        initialization::<Nibble>();
    }

    #[test]
    fn three_bit_roundtrips() {
        roundtrip::<ThreeBit>();
        initialization::<ThreeBit>();
    }

    #[test]
    fn packed_initialization_leaves_unused_storage_zeroed() {
        let nibble = Nibble::with_len(3, Facelet::Blue);
        assert_eq!(nibble.as_packed_slice()[1] >> 4, 0);

        let third_byte_one_extra = ThirdByte::with_len(4, Facelet::Blue);
        assert_eq!(
            third_byte_one_extra.as_packed_slice()[1],
            Facelet::Blue.as_u8()
        );

        let third_byte_two_extra = ThirdByte::with_len(5, Facelet::Blue);
        assert_eq!(
            third_byte_two_extra.as_packed_slice()[1],
            Facelet::Blue.as_u8() + Facelet::Blue.as_u8() * 6
        );

        let three_bit = ThreeBit::with_len(1, Facelet::Blue);
        assert_eq!(three_bit.as_packed_words()[0] & !0b111, 0);
    }

    #[test]
    fn storage_backends_agree_after_random_updates() {
        let len = 257;
        let mut rng = XorShift64::new(0x51A7E_F00D);
        let mut reference = vec![Facelet::White; len];
        let mut byte = Byte::with_len(len, Facelet::White);
        let mut third_byte = ThirdByte::with_len(len, Facelet::White);
        let mut nibble = Nibble::with_len(len, Facelet::White);
        let mut three_bit = ThreeBit::with_len(len, Facelet::White);

        for _ in 0..10_000 {
            let index = (rng.next_u64() as usize) % len;
            let value = Facelet::from_u8((rng.next_u64() % 6) as u8);

            reference[index] = value;
            byte.set(index, value);
            third_byte.set(index, value);
            nibble.set(index, value);
            three_bit.set(index, value);
        }

        assert_array_matches_reference(&byte, &reference);
        assert_array_matches_reference(&third_byte, &reference);
        assert_array_matches_reference(&nibble, &reference);
        assert_array_matches_reference(&three_bit, &reference);
    }

    #[test]
    fn storage_byte_estimates_are_exact() {
        for len in 0usize..200 {
            assert_eq!(Byte::storage_bytes_for_len(len), len);
            assert_eq!(ThirdByte::storage_bytes_for_len(len), len.div_ceil(3));
            assert_eq!(Nibble::storage_bytes_for_len(len), len.div_ceil(2));
            assert_eq!(
                ThreeBit::storage_bytes_for_len(len),
                len.checked_mul(3).unwrap().div_ceil(64) * 8
            );

            let byte = Byte::with_len(len, Facelet::White);
            let third_byte = ThirdByte::with_len(len, Facelet::White);
            let nibble = Nibble::with_len(len, Facelet::White);
            let three_bit = ThreeBit::with_len(len, Facelet::White);

            assert_eq!(byte.as_slice().len(), Byte::storage_bytes_for_len(len));
            assert_eq!(
                third_byte.capacity_bytes(),
                ThirdByte::storage_bytes_for_len(len)
            );
            assert_eq!(nibble.capacity_bytes(), Nibble::storage_bytes_for_len(len));
            assert_eq!(
                three_bit.capacity_words() * core::mem::size_of::<u64>(),
                ThreeBit::storage_bytes_for_len(len)
            );
        }
    }
}
