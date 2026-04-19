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
    use crate::Facelet;

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
}
