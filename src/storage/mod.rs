mod byte;
mod nibble;
mod packed3;
mod traits;

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
    fn nibble_array_roundtrips() {
        roundtrip::<NibbleArray>();
    }

    #[test]
    fn packed3_array_roundtrips() {
        roundtrip::<Packed3Array>();
    }
}
