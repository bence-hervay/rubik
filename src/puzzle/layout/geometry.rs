use crate::{
    face::FaceId,
    line::{LineKind, StripSpec},
    moves::Axis,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct Coord3 {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) z: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct Dir3 {
    x: i8,
    y: i8,
    z: i8,
}

impl Dir3 {
    const POS_X: Self = Self { x: 1, y: 0, z: 0 };
    const NEG_X: Self = Self { x: -1, y: 0, z: 0 };
    const POS_Y: Self = Self { x: 0, y: 1, z: 0 };
    const NEG_Y: Self = Self { x: 0, y: -1, z: 0 };
    const POS_Z: Self = Self { x: 0, y: 0, z: 1 };
    const NEG_Z: Self = Self { x: 0, y: 0, z: -1 };
}

pub(crate) fn face_normal(face: FaceId) -> Dir3 {
    match face {
        FaceId::U => Dir3::POS_Y,
        FaceId::D => Dir3::NEG_Y,
        FaceId::R => Dir3::POS_X,
        FaceId::L => Dir3::NEG_X,
        FaceId::F => Dir3::POS_Z,
        FaceId::B => Dir3::NEG_Z,
    }
}

fn normal_axis(face: FaceId) -> Axis {
    match face {
        FaceId::R | FaceId::L => Axis::X,
        FaceId::U | FaceId::D => Axis::Y,
        FaceId::F | FaceId::B => Axis::Z,
    }
}

pub(crate) fn positive_axis_face(axis: Axis) -> FaceId {
    match axis {
        Axis::X => FaceId::R,
        Axis::Y => FaceId::U,
        Axis::Z => FaceId::F,
    }
}

pub(crate) fn negative_axis_face(axis: Axis) -> FaceId {
    match axis {
        Axis::X => FaceId::L,
        Axis::Y => FaceId::D,
        Axis::Z => FaceId::B,
    }
}

pub(crate) fn logical_to_coord(face: FaceId, row: usize, col: usize, n: usize) -> Coord3 {
    debug_assert!(n > 0);
    debug_assert!(row < n);
    debug_assert!(col < n);

    match face {
        FaceId::U => Coord3 {
            x: col,
            y: n - 1,
            z: row,
        },
        FaceId::D => Coord3 {
            x: col,
            y: 0,
            z: n - 1 - row,
        },
        FaceId::R => Coord3 {
            x: n - 1,
            y: n - 1 - row,
            z: n - 1 - col,
        },
        FaceId::L => Coord3 {
            x: 0,
            y: n - 1 - row,
            z: col,
        },
        FaceId::F => Coord3 {
            x: col,
            y: n - 1 - row,
            z: n - 1,
        },
        FaceId::B => Coord3 {
            x: n - 1 - col,
            y: n - 1 - row,
            z: 0,
        },
    }
}

pub(crate) fn coord_to_logical(face: FaceId, coord: Coord3, n: usize) -> (usize, usize) {
    debug_assert!(n > 0);

    match face {
        FaceId::U => {
            debug_assert_eq!(coord.y, n - 1);
            (coord.z, coord.x)
        }
        FaceId::D => {
            debug_assert_eq!(coord.y, 0);
            (n - 1 - coord.z, coord.x)
        }
        FaceId::R => {
            debug_assert_eq!(coord.x, n - 1);
            (n - 1 - coord.y, n - 1 - coord.z)
        }
        FaceId::L => {
            debug_assert_eq!(coord.x, 0);
            (n - 1 - coord.y, coord.z)
        }
        FaceId::F => {
            debug_assert_eq!(coord.z, n - 1);
            (n - 1 - coord.y, coord.x)
        }
        FaceId::B => {
            debug_assert_eq!(coord.z, 0);
            (n - 1 - coord.y, n - 1 - coord.x)
        }
    }
}

fn face_from_normal(normal: Dir3) -> FaceId {
    match normal {
        Dir3 { x: 1, y: 0, z: 0 } => FaceId::R,
        Dir3 { x: -1, y: 0, z: 0 } => FaceId::L,
        Dir3 { x: 0, y: 1, z: 0 } => FaceId::U,
        Dir3 { x: 0, y: -1, z: 0 } => FaceId::D,
        Dir3 { x: 0, y: 0, z: 1 } => FaceId::F,
        Dir3 { x: 0, y: 0, z: -1 } => FaceId::B,
        _ => panic!("invalid face normal: {normal:?}"),
    }
}

fn rotate_coord_cw(axis: Axis, coord: Coord3, n: usize) -> Coord3 {
    match axis {
        Axis::X => Coord3 {
            x: coord.x,
            y: coord.z,
            z: n - 1 - coord.y,
        },
        Axis::Y => Coord3 {
            x: n - 1 - coord.z,
            y: coord.y,
            z: coord.x,
        },
        Axis::Z => Coord3 {
            x: coord.y,
            y: n - 1 - coord.x,
            z: coord.z,
        },
    }
}

fn rotate_normal_cw(axis: Axis, normal: Dir3) -> Dir3 {
    match axis {
        Axis::X => Dir3 {
            x: normal.x,
            y: normal.z,
            z: -normal.y,
        },
        Axis::Y => Dir3 {
            x: -normal.z,
            y: normal.y,
            z: normal.x,
        },
        Axis::Z => Dir3 {
            x: normal.y,
            y: -normal.x,
            z: normal.z,
        },
    }
}

fn line_point(spec: StripSpec, t: usize, n: usize) -> (usize, usize) {
    let p = if spec.reversed { n - 1 - t } else { t };
    match spec.kind {
        LineKind::Row => (spec.index, p),
        LineKind::Col => (p, spec.index),
    }
}

fn strip_base_for_face(face: FaceId, axis: Axis, depth: usize, n: usize) -> Option<StripSpec> {
    if normal_axis(face) == axis {
        return None;
    }

    let spec = match (face, axis) {
        (FaceId::U, Axis::X) => StripSpec::col(face, depth, false),
        (FaceId::U, Axis::Z) => StripSpec::row(face, depth, false),
        (FaceId::D, Axis::X) => StripSpec::col(face, depth, false),
        (FaceId::D, Axis::Z) => StripSpec::row(face, n - 1 - depth, false),
        (FaceId::F, Axis::X) => StripSpec::col(face, depth, false),
        (FaceId::F, Axis::Y) => StripSpec::row(face, n - 1 - depth, false),
        (FaceId::B, Axis::X) => StripSpec::col(face, n - 1 - depth, false),
        (FaceId::B, Axis::Y) => StripSpec::row(face, n - 1 - depth, false),
        (FaceId::R, Axis::Y) => StripSpec::row(face, n - 1 - depth, false),
        (FaceId::R, Axis::Z) => StripSpec::col(face, n - 1 - depth, false),
        (FaceId::L, Axis::Y) => StripSpec::row(face, n - 1 - depth, false),
        (FaceId::L, Axis::Z) => StripSpec::col(face, depth, false),
        _ => unreachable!("perpendicular faces were filtered out"),
    };

    Some(spec)
}

fn mapped_strip_after_cw(axis: Axis, spec: StripSpec, n: usize) -> StripSpec {
    let (row0, col0) = line_point(spec, 0, n);
    let coord0 = logical_to_coord(spec.face, row0, col0, n);
    let normal0 = face_normal(spec.face);
    let dest_coord0 = rotate_coord_cw(axis, coord0, n);
    let dest_normal0 = rotate_normal_cw(axis, normal0);
    let dest_face = face_from_normal(dest_normal0);
    let (dest_row0, dest_col0) = coord_to_logical(dest_face, dest_coord0, n);

    if n == 1 {
        return strip_base_for_face(dest_face, axis, 0, n)
            .expect("mapped 1x1 strip should land on a side face");
    }

    let (row1, col1) = line_point(spec, n - 1, n);
    let coord1 = logical_to_coord(spec.face, row1, col1, n);
    let dest_coord1 = rotate_coord_cw(axis, coord1, n);
    let (dest_row1, dest_col1) = coord_to_logical(dest_face, dest_coord1, n);

    if dest_row0 == dest_row1 {
        let reversed = match (dest_col0, dest_col1) {
            (0, end) if end == n - 1 => false,
            (end, 0) if end == n - 1 => true,
            _ => panic!("mapped row strip is not contiguous"),
        };
        StripSpec::row(dest_face, dest_row0, reversed)
    } else if dest_col0 == dest_col1 {
        let reversed = match (dest_row0, dest_row1) {
            (0, end) if end == n - 1 => false,
            (end, 0) if end == n - 1 => true,
            _ => panic!("mapped col strip is not contiguous"),
        };
        StripSpec::col(dest_face, dest_col0, reversed)
    } else {
        panic!("mapped strip did not land on a row or column")
    }
}

pub(crate) fn plan_positive_quarter_turn(axis: Axis, depth: usize, n: usize) -> [StripSpec; 4] {
    assert!(n > 0, "cube side length must be > 0");
    assert!(depth < n, "move depth out of bounds");

    let mut start = None;
    for face in FaceId::ALL {
        if let Some(spec) = strip_base_for_face(face, axis, depth, n) {
            start = Some(spec);
            break;
        }
    }

    let mut specs = [start.expect("axis should always have four side strips"); 4];
    for i in 1..4 {
        specs[i] = mapped_strip_after_cw(axis, specs[i - 1], n);
    }

    let back_to_start = mapped_strip_after_cw(axis, specs[3], n);
    assert_eq!(
        (back_to_start.face, back_to_start.kind, back_to_start.index),
        (specs[0].face, specs[0].kind, specs[0].index),
        "strip cycle did not close"
    );

    specs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_close_for_small_sizes() {
        for n in 1..8 {
            for depth in 0..n {
                for axis in [Axis::X, Axis::Y, Axis::Z] {
                    let specs = plan_positive_quarter_turn(axis, depth, n);
                    assert_eq!(specs.len(), 4);
                }
            }
        }
    }
}
