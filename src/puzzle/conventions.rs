use crate::{
    face::FaceId,
    facelet::Facelet,
    moves::{Axis, Move, MoveAngle},
};

pub const fn face_axis(face: FaceId) -> Axis {
    match face {
        FaceId::R | FaceId::L => Axis::X,
        FaceId::U | FaceId::D => Axis::Y,
        FaceId::F | FaceId::B => Axis::Z,
    }
}

pub const fn opposite_face(face: FaceId) -> FaceId {
    match face {
        FaceId::U => FaceId::D,
        FaceId::D => FaceId::U,
        FaceId::R => FaceId::L,
        FaceId::L => FaceId::R,
        FaceId::F => FaceId::B,
        FaceId::B => FaceId::F,
    }
}

pub const fn home_facelet_for_face(face: FaceId) -> Facelet {
    match face {
        FaceId::U => Facelet::White,
        FaceId::D => Facelet::Yellow,
        FaceId::R => Facelet::Red,
        FaceId::L => Facelet::Orange,
        FaceId::F => Facelet::Green,
        FaceId::B => Facelet::Blue,
    }
}

pub const fn normalize_face_pair(first: FaceId, second: FaceId) -> (FaceId, FaceId) {
    if first.index() <= second.index() {
        (first, second)
    } else {
        (second, first)
    }
}

pub fn face_layer_move(
    side_length: usize,
    face: FaceId,
    depth_from_face: usize,
    angle: MoveAngle,
) -> Move {
    assert!(side_length > 0, "cube side length must be > 0");
    assert!(
        depth_from_face < side_length,
        "face-relative move depth out of bounds"
    );

    let last = side_length - 1;
    match face {
        FaceId::U => Move::new(Axis::Y, last - depth_from_face, angle),
        FaceId::D => Move::new(Axis::Y, depth_from_face, angle.inverse()),
        FaceId::R => Move::new(Axis::X, last - depth_from_face, angle),
        FaceId::L => Move::new(Axis::X, depth_from_face, angle.inverse()),
        FaceId::F => Move::new(Axis::Z, last - depth_from_face, angle),
        FaceId::B => Move::new(Axis::Z, depth_from_face, angle.inverse()),
    }
}

pub fn face_outer_move(side_length: usize, face: FaceId, angle: MoveAngle) -> Move {
    face_layer_move(side_length, face, 0, angle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_faces_are_involutive() {
        for face in FaceId::ALL {
            assert_eq!(opposite_face(opposite_face(face)), face);
            assert_ne!(opposite_face(face), face);
        }
    }

    #[test]
    fn home_facelets_follow_canonical_face_order() {
        assert_eq!(home_facelet_for_face(FaceId::U), Facelet::White);
        assert_eq!(home_facelet_for_face(FaceId::D), Facelet::Yellow);
        assert_eq!(home_facelet_for_face(FaceId::R), Facelet::Red);
        assert_eq!(home_facelet_for_face(FaceId::L), Facelet::Orange);
        assert_eq!(home_facelet_for_face(FaceId::F), Facelet::Green);
        assert_eq!(home_facelet_for_face(FaceId::B), Facelet::Blue);
    }

    #[test]
    fn face_axes_match_cube_conventions() {
        assert_eq!(face_axis(FaceId::L), Axis::X);
        assert_eq!(face_axis(FaceId::R), Axis::X);
        assert_eq!(face_axis(FaceId::D), Axis::Y);
        assert_eq!(face_axis(FaceId::U), Axis::Y);
        assert_eq!(face_axis(FaceId::B), Axis::Z);
        assert_eq!(face_axis(FaceId::F), Axis::Z);
    }

    #[test]
    fn normalized_face_pair_is_stable() {
        assert_eq!(
            normalize_face_pair(FaceId::R, FaceId::F),
            (FaceId::R, FaceId::F)
        );
        assert_eq!(
            normalize_face_pair(FaceId::F, FaceId::R),
            (FaceId::R, FaceId::F)
        );
    }

    #[test]
    fn face_relative_moves_match_standard_outer_turn_examples() {
        let last = 4;

        assert_eq!(
            face_outer_move(5, FaceId::U, MoveAngle::Positive),
            Move::new(Axis::Y, last, MoveAngle::Positive)
        );
        assert_eq!(
            face_outer_move(5, FaceId::L, MoveAngle::Positive),
            Move::new(Axis::X, 0, MoveAngle::Negative)
        );
        assert_eq!(
            face_outer_move(5, FaceId::B, MoveAngle::Positive),
            Move::new(Axis::Z, 0, MoveAngle::Negative)
        );
    }

    #[test]
    fn inner_face_relative_depths_map_to_axis_depths() {
        assert_eq!(
            face_layer_move(6, FaceId::R, 2, MoveAngle::Double),
            Move::new(Axis::X, 3, MoveAngle::Double)
        );
        assert_eq!(
            face_layer_move(6, FaceId::D, 2, MoveAngle::Negative),
            Move::new(Axis::Y, 2, MoveAngle::Positive)
        );
    }
}
