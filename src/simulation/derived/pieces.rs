use crate::{
    face::FaceId,
    geometry,
    line::StripSpec,
    moves::{Move, MoveAngle},
    simulation::derived::{CornerCubieLocation, EdgeCubieLocation, FaceletLocation},
};

impl EdgeCubieLocation {
    pub const fn stickers(self) -> [FaceletLocation; 2] {
        self.stickers
    }
}

impl CornerCubieLocation {
    pub const fn stickers(self) -> [FaceletLocation; 3] {
        self.stickers
    }
}

pub(crate) fn edge_cubie_for_facelet_location(
    side_length: usize,
    location: FaceletLocation,
) -> Option<EdgeCubieLocation> {
    edge_cubie_location(side_length, location)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn corner_cubie_for_facelet_location(
    side_length: usize,
    location: FaceletLocation,
) -> Option<CornerCubieLocation> {
    corner_cubie_location(side_length, location)
}

pub(crate) fn edge_cubie_orbit_index(
    side_length: usize,
    cubie: EdgeCubieLocation,
) -> Option<usize> {
    let [first, second] = cubie.stickers();
    let first = edge_facelet_orbit_index(side_length, first)?;
    let second = edge_facelet_orbit_index(side_length, second)?;
    if first == second {
        Some(first)
    } else {
        None
    }
}

pub(crate) fn trace_edge_cubie_through_move(
    side_length: usize,
    cubie: EdgeCubieLocation,
    mv: Move,
) -> EdgeCubieLocation {
    let [first, second] = cubie.stickers();
    let first = trace_position_through_move(
        side_length,
        FacePosition {
            face: first.face,
            row: first.row,
            col: first.col,
        },
        mv,
    );
    let second = trace_position_through_move(
        side_length,
        FacePosition {
            face: second.face,
            row: second.row,
            col: second.col,
        },
        mv,
    );
    let first_location = facelet_location(first);
    let second_location = facelet_location(second);
    let turning_face = if mv.depth == 0 {
        Some(geometry::negative_axis_face(mv.axis))
    } else if mv.depth + 1 == side_length {
        Some(geometry::positive_axis_face(mv.axis))
    } else {
        None
    };
    let anchor = if turning_face == Some(cubie.stickers()[0].face) {
        first_location
    } else if turning_face == Some(cubie.stickers()[1].face) {
        second_location
    } else {
        first_location
    };

    edge_cubie_location(side_length, anchor)
        .expect("traced edge sticker must stay on an edge cubie")
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn trace_facelet_location_through_moves(
    side_length: usize,
    location: FaceletLocation,
    moves: &[Move],
) -> FaceletLocation {
    let position = trace_position(
        side_length,
        FacePosition {
            face: location.face,
            row: location.row,
            col: location.col,
        },
        moves.iter().copied(),
    );
    facelet_location(position)
}

pub(crate) fn trace_facelet_location_through_move(
    side_length: usize,
    location: FaceletLocation,
    mv: Move,
) -> FaceletLocation {
    let position = trace_position_through_move(
        side_length,
        FacePosition {
            face: location.face,
            row: location.row,
            col: location.col,
        },
        mv,
    );
    facelet_location(position)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn trace_corner_cubie_through_move(
    side_length: usize,
    cubie: CornerCubieLocation,
    mv: Move,
) -> CornerCubieLocation {
    let traced = trace_facelet_location_through_move(side_length, cubie.stickers()[0], mv);
    corner_cubie_location(side_length, traced).expect("traced facelet must stay on a corner cubie")
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct FacePosition {
    pub(crate) face: FaceId,
    pub(crate) row: usize,
    pub(crate) col: usize,
}

pub(crate) fn facelet_location(position: FacePosition) -> FaceletLocation {
    FaceletLocation {
        face: position.face,
        row: position.row,
        col: position.col,
    }
}

pub(crate) fn trace_position(
    n: usize,
    position: FacePosition,
    moves: impl IntoIterator<Item = Move>,
) -> FacePosition {
    moves.into_iter().fold(position, |position, mv| {
        trace_position_through_move(n, position, mv)
    })
}

pub(crate) fn trace_position_through_move(
    n: usize,
    mut position: FacePosition,
    mv: Move,
) -> FacePosition {
    if mv.depth == n - 1 && position.face == geometry::positive_axis_face(mv.axis) {
        position = rotate_face_position(position, n, mv.angle);
    } else if mv.depth == 0 && position.face == geometry::negative_axis_face(mv.axis) {
        position = rotate_face_position(position, n, mv.angle.inverse());
    }

    let specs = geometry::plan_positive_quarter_turn(mv.axis, mv.depth, n);
    for (index, spec) in specs.iter().copied().enumerate() {
        if let Some(offset) = strip_offset(position, spec, n) {
            let destination_index = (index + usize::from(mv.angle.as_u8())) % specs.len();
            return strip_position(specs[destination_index], offset, n);
        }
    }

    position
}

fn rotate_face_position(mut position: FacePosition, n: usize, angle: MoveAngle) -> FacePosition {
    for _ in 0..angle.as_u8() {
        position = FacePosition {
            face: position.face,
            row: position.col,
            col: n - 1 - position.row,
        };
    }
    position
}

fn strip_offset(position: FacePosition, spec: StripSpec, n: usize) -> Option<usize> {
    if position.face != spec.face {
        return None;
    }

    match spec.kind {
        crate::line::LineKind::Row if position.row == spec.index => Some(if spec.reversed {
            n - 1 - position.col
        } else {
            position.col
        }),
        crate::line::LineKind::Col if position.col == spec.index => Some(if spec.reversed {
            n - 1 - position.row
        } else {
            position.row
        }),
        _ => None,
    }
}

fn strip_position(spec: StripSpec, offset: usize, n: usize) -> FacePosition {
    let coordinate = if spec.reversed {
        n - 1 - offset
    } else {
        offset
    };
    match spec.kind {
        crate::line::LineKind::Row => FacePosition {
            face: spec.face,
            row: spec.index,
            col: coordinate,
        },
        crate::line::LineKind::Col => FacePosition {
            face: spec.face,
            row: coordinate,
            col: spec.index,
        },
    }
}

pub(crate) fn unique_edge_cubies(
    n: usize,
    locations: impl IntoIterator<Item = FaceletLocation>,
) -> Option<[EdgeCubieLocation; 3]> {
    let mut cubies = [None; 3];
    let mut len = 0;

    for location in locations {
        let cubie = edge_cubie_location(n, location)?;
        if cubies[..len].contains(&Some(cubie)) {
            continue;
        }
        if len == cubies.len() {
            return None;
        }
        cubies[len] = Some(cubie);
        len += 1;
    }

    if len != cubies.len() {
        return None;
    }

    Some(cubies.map(|cubie| cubie.expect("edge cubie entry must be initialized")))
}

fn edge_facelet_orbit_index(n: usize, location: FaceletLocation) -> Option<usize> {
    if n < 3 || location.row >= n || location.col >= n {
        return None;
    }

    let offset = match (location.row, location.col) {
        (0, col) if col > 0 && col + 1 < n => col,
        (last_row, col) if last_row + 1 == n && col > 0 && col + 1 < n => col,
        (row, 0) if row > 0 && row + 1 < n => row,
        (row, last_col) if last_col + 1 == n && row > 0 && row + 1 < n => row,
        _ => return None,
    };

    Some(offset.min(n - 1 - offset))
}

pub(crate) fn edge_cubie_location(
    n: usize,
    location: FaceletLocation,
) -> Option<EdgeCubieLocation> {
    if n < 3 || location.row >= n || location.col >= n {
        return None;
    }

    let coord = geometry::logical_to_coord(location.face, location.row, location.col, n);
    let mut boundary_faces = [None; 3];
    let mut len = 0;

    if coord.x == 0 {
        boundary_faces[len] = Some(FaceId::L);
        len += 1;
    } else if coord.x + 1 == n {
        boundary_faces[len] = Some(FaceId::R);
        len += 1;
    }

    if coord.y == 0 {
        boundary_faces[len] = Some(FaceId::D);
        len += 1;
    } else if coord.y + 1 == n {
        boundary_faces[len] = Some(FaceId::U);
        len += 1;
    }

    if coord.z == 0 {
        boundary_faces[len] = Some(FaceId::B);
        len += 1;
    } else if coord.z + 1 == n {
        boundary_faces[len] = Some(FaceId::F);
        len += 1;
    }

    if len != 2 {
        return None;
    }

    let first_face = boundary_faces[0]?;
    let second_face = boundary_faces[1]?;
    let other_face = if location.face == first_face {
        second_face
    } else if location.face == second_face {
        first_face
    } else {
        return None;
    };
    let (other_row, other_col) = geometry::coord_to_logical(other_face, coord, n);
    let other = FaceletLocation {
        face: other_face,
        row: other_row,
        col: other_col,
    };

    Some(canonical_edge_cubie(location, other))
}

fn canonical_edge_cubie(first: FaceletLocation, second: FaceletLocation) -> EdgeCubieLocation {
    if facelet_location_key(second) < facelet_location_key(first) {
        EdgeCubieLocation {
            stickers: [second, first],
        }
    } else {
        EdgeCubieLocation {
            stickers: [first, second],
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn corner_cubie_location(n: usize, location: FaceletLocation) -> Option<CornerCubieLocation> {
    if n < 2 || location.row >= n || location.col >= n {
        return None;
    }

    let coord = geometry::logical_to_coord(location.face, location.row, location.col, n);
    let mut boundary_faces = [None; 3];
    let mut len = 0;

    if coord.x == 0 {
        boundary_faces[len] = Some(FaceId::L);
        len += 1;
    } else if coord.x + 1 == n {
        boundary_faces[len] = Some(FaceId::R);
        len += 1;
    }

    if coord.y == 0 {
        boundary_faces[len] = Some(FaceId::D);
        len += 1;
    } else if coord.y + 1 == n {
        boundary_faces[len] = Some(FaceId::U);
        len += 1;
    }

    if coord.z == 0 {
        boundary_faces[len] = Some(FaceId::B);
        len += 1;
    } else if coord.z + 1 == n {
        boundary_faces[len] = Some(FaceId::F);
        len += 1;
    }

    if len != 3 {
        return None;
    }

    let mut stickers = [location; 3];
    let mut sticker_index = 1;

    for face in boundary_faces.into_iter().flatten() {
        if face == location.face {
            continue;
        }

        let (row, col) = geometry::coord_to_logical(face, coord, n);
        stickers[sticker_index] = FaceletLocation { face, row, col };
        sticker_index += 1;
    }

    debug_assert_eq!(sticker_index, 3);
    Some(canonical_corner_cubie(stickers))
}

#[cfg_attr(not(test), allow(dead_code))]
fn canonical_corner_cubie(mut stickers: [FaceletLocation; 3]) -> CornerCubieLocation {
    stickers.sort_by_key(|location| facelet_location_key(*location));
    CornerCubieLocation { stickers }
}

fn facelet_location_key(location: FaceletLocation) -> (usize, usize, usize) {
    (location.face.index(), location.row, location.col)
}

pub(crate) fn facelet_locations_are_unique(
    locations: impl IntoIterator<Item = FaceletLocation>,
) -> bool {
    let mut seen = [None; 6];
    let mut len = 0;

    for location in locations {
        if seen[..len].contains(&Some(location)) {
            return false;
        }
        if len == seen.len() {
            return false;
        }
        seen[len] = Some(location);
        len += 1;
    }

    true
}

pub(crate) fn edge_cubie_sets_match(
    left: [EdgeCubieLocation; 3],
    right: [EdgeCubieLocation; 3],
) -> bool {
    left.iter().all(|cubie| right.contains(cubie)) && right.iter().all(|cubie| left.contains(cubie))
}

pub(crate) fn edge_cubie_index(
    cubies: [EdgeCubieLocation; 3],
    target: EdgeCubieLocation,
) -> Option<usize> {
    cubies.iter().position(|cubie| *cubie == target)
}
