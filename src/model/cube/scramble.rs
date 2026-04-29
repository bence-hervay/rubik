use std::sync::OnceLock;

use crate::{
    algorithms::{
        centers::{CenterCommutatorTable, GENERATED_CENTER_SCHEDULE},
        CornerSlot,
    },
    conventions::{home_facelet_for_face, normalize_face_pair},
    cube::{edge_cubie_for_facelet_location, Cube, CubeReachability, EdgeCubieLocation},
    face::{FaceAngle, FaceId},
    geometry,
    moves::{Axis, Move, MoveAngle},
    random::RandomSource,
    simulation::derived::{trace_facelet_location_through_move, FaceletLocation},
    storage::FaceletArray,
};

impl<S: FaceletArray> Cube<S> {
    /// Replaces the cube with a random reachable state by writing facelets
    /// directly inside the piece orbits from the n x n x n first-law
    /// decomposition.
    ///
    /// The method does not apply or record moves. Corners receive a random
    /// twist vector with total twist zero; centers are scrambled with direct
    /// normalized center-commutator updates; odd-cube middle edges are kept
    /// unflipped; wing-edge orientation is determined by the paired a/b
    /// position type described in the first-law constraints.
    pub fn scramble_direct<R: RandomSource>(&mut self, rng: &mut R) {
        self.reset_to_solved_facelets();

        let center_edge_parities = self.scramble_direct_centers(rng);
        let corner_parity = if self.n >= 4 { Some(false) } else { None };
        let corner_odd = self.scramble_direct_corners(corner_parity, rng);
        self.scramble_direct_edges(corner_odd, &center_edge_parities, rng);

        self.reachability = CubeReachability::Reachable;
        self.history.clear();
    }

    fn reset_to_solved_facelets(&mut self) {
        for face in FaceId::ALL {
            let face_ref = &mut self.faces[face.index()];
            face_ref.set_rotation(FaceAngle::default());
            let value = home_facelet_for_face(face);
            for row in 0..self.n {
                for col in 0..self.n {
                    face_ref.set(row, col, value);
                }
            }
        }
    }

    fn scramble_direct_corners<R: RandomSource>(
        &mut self,
        parity: Option<bool>,
        rng: &mut R,
    ) -> bool {
        if self.n < 2 {
            return false;
        }

        let mut permutation = random_permutation(CornerSlot::ALL.len(), rng);
        if let Some(odd) = parity {
            force_permutation_parity(&mut permutation, odd);
        }
        let twists = random_corner_twists(rng);

        for (destination_index, source_index) in permutation.iter().copied().enumerate() {
            let source = CornerSlot::ALL[source_index];
            let destination = CornerSlot::ALL[destination_index];
            let source_faces = source.faces();
            let destination_stickers = destination.stickers(self.n);
            let twist = twists[destination_index] as usize;

            for source_orientation in 0..3 {
                let destination_orientation = (source_orientation + twist) % 3;
                self.write_facelet_location(
                    destination_stickers[destination_orientation],
                    home_facelet_for_face(source_faces[source_orientation]),
                );
            }
        }

        permutation_is_odd(&permutation)
    }

    fn scramble_direct_centers<R: RandomSource>(&mut self, rng: &mut R) -> Vec<bool> {
        let center_edge_parities = vec![false; wing_orbit_count(self.n) + 1];
        if self.n < 4 {
            return Vec::new();
        }

        static CENTER_TABLE: OnceLock<CenterCommutatorTable> = OnceLock::new();
        let table = CENTER_TABLE.get_or_init(CenterCommutatorTable::new);
        let mut applied = 0usize;
        let target_count = center_ring_count(self.n).max(1);

        while applied < target_count {
            let step =
                GENERATED_CENTER_SCHEDULE[random_below(rng, GENERATED_CENTER_SCHEDULE.len())];
            let row = 1 + random_below(rng, self.n - 2);
            let column = 1 + random_below(rng, self.n - 2);

            if row == column {
                continue;
            }

            let Some(commutator) = table.get(step.destination, step.helper, step.angle) else {
                continue;
            };

            for _ in 0..2 {
                let rows = [row];
                let columns = [column];
                let plan = self.normalized_face_commutator_plan(commutator, &rows, &columns);
                self.apply_face_commutator_plan_untracked(plan);
            }
            applied += 1;
        }

        center_edge_parities
    }

    fn scramble_direct_edges<R: RandomSource>(
        &mut self,
        corner_odd: bool,
        center_edge_parities: &[bool],
        rng: &mut R,
    ) {
        if self.n < 3 {
            return;
        }

        if self.n % 2 == 1 {
            let positions = middle_edge_positions(self.n);
            self.permute_edge_orbit(
                &positions,
                Some(corner_odd),
                EdgeOrientationMode::Middle,
                rng,
            );
        }

        for row in 1..=wing_orbit_count(self.n) {
            let positions = wing_edge_positions(self.n, row);
            let parity = if self.n % 2 == 1 {
                let center_ring = wing_orbit_count(self.n) + 1 - row;
                let center_edge_odd = center_edge_parities
                    .get(center_ring)
                    .copied()
                    .unwrap_or(false);
                Some(corner_odd ^ center_edge_odd)
            } else {
                None
            };
            self.permute_edge_orbit(&positions, parity, EdgeOrientationMode::Wing, rng);
        }
    }

    fn permute_edge_orbit<R: RandomSource>(
        &mut self,
        positions: &[EdgePosition],
        parity: Option<bool>,
        orientation_mode: EdgeOrientationMode,
        rng: &mut R,
    ) -> bool {
        if positions.is_empty() {
            return false;
        }

        let mut permutation = random_permutation(positions.len(), rng);
        if let Some(odd) = parity {
            force_permutation_parity(&mut permutation, odd);
        }
        let is_odd = permutation_is_odd(&permutation);
        let wing_types = match orientation_mode {
            EdgeOrientationMode::Middle => None,
            EdgeOrientationMode::Wing => Some(wing_position_types(self.n, positions)),
        };

        for (destination_index, source_index) in permutation.into_iter().enumerate() {
            let source = edge_cubie_from_position(self.n, positions[source_index]);
            let destination = edge_cubie_from_position(self.n, positions[destination_index]);
            let reversed = match orientation_mode {
                EdgeOrientationMode::Middle => false,
                EdgeOrientationMode::Wing => {
                    let wing_types = wing_types
                        .as_ref()
                        .expect("wing types must exist in wing orientation mode");
                    wing_types[source_index] != wing_types[destination_index]
                }
            };
            self.write_edge_cubie(source, destination, reversed);
        }

        is_odd
    }

    fn write_edge_cubie(
        &mut self,
        source: EdgeCubieLocation,
        destination: EdgeCubieLocation,
        reversed: bool,
    ) {
        let [source_first, source_second] = source.stickers();
        let [destination_first, destination_second] = destination.stickers();
        let (to_first, to_second) = if reversed {
            (destination_second, destination_first)
        } else {
            (destination_first, destination_second)
        };

        self.write_facelet_location(to_first, home_facelet_for_face(source_first.face));
        self.write_facelet_location(to_second, home_facelet_for_face(source_second.face));
    }

    fn write_facelet_location(&mut self, location: FaceletLocation, value: crate::Facelet) {
        self.faces[location.face.index()].set(location.row, location.col, value);
    }
}

#[derive(Copy, Clone, Debug)]
enum EdgeOrientationMode {
    Middle,
    Wing,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct EdgePosition {
    coord: geometry::Coord3,
    faces: (FaceId, FaceId),
}

fn random_corner_twists<R: RandomSource>(rng: &mut R) -> [u8; 8] {
    let mut twists = [0u8; 8];
    let mut sum = 0usize;

    for twist in twists.iter_mut().take(7) {
        *twist = random_below(rng, 3) as u8;
        sum += *twist as usize;
    }
    twists[7] = ((3 - (sum % 3)) % 3) as u8;

    twists
}

fn center_ring_count(side_length: usize) -> usize {
    side_length.saturating_sub(2) / 2
}

fn wing_orbit_count(side_length: usize) -> usize {
    side_length.saturating_sub(2) / 2
}

fn middle_edge_positions(side_length: usize) -> Vec<EdgePosition> {
    assert!(
        side_length >= 3 && side_length % 2 == 1,
        "middle edge orbit requires an odd side length of at least 3"
    );

    let middle = side_length / 2;
    let last = side_length - 1;

    vec![
        edge_position(coord3(middle, last, last), FaceId::U, FaceId::F),
        edge_position(coord3(last, last, middle), FaceId::U, FaceId::R),
        edge_position(coord3(middle, last, 0), FaceId::U, FaceId::B),
        edge_position(coord3(0, last, middle), FaceId::U, FaceId::L),
        edge_position(coord3(last, middle, last), FaceId::F, FaceId::R),
        edge_position(coord3(0, middle, last), FaceId::F, FaceId::L),
        edge_position(coord3(last, middle, 0), FaceId::B, FaceId::R),
        edge_position(coord3(0, middle, 0), FaceId::B, FaceId::L),
        edge_position(coord3(middle, 0, last), FaceId::D, FaceId::F),
        edge_position(coord3(last, 0, middle), FaceId::D, FaceId::R),
        edge_position(coord3(middle, 0, 0), FaceId::D, FaceId::B),
        edge_position(coord3(0, 0, middle), FaceId::D, FaceId::L),
    ]
}

fn wing_edge_positions(side_length: usize, row: usize) -> Vec<EdgePosition> {
    assert!(
        side_length >= 4,
        "wing edge orbit requires side length at least 4"
    );
    assert!(
        row > 0 && row + 1 < side_length,
        "wing row must be an inner row"
    );
    if side_length % 2 == 1 {
        assert_ne!(
            row,
            side_length / 2,
            "wing row cannot be the odd middle layer"
        );
    }

    let last = side_length - 1;
    let mirror = last - row;

    vec![
        edge_position(coord3(row, last, last), FaceId::U, FaceId::F),
        edge_position(coord3(mirror, last, last), FaceId::U, FaceId::F),
        edge_position(coord3(last, last, row), FaceId::U, FaceId::R),
        edge_position(coord3(last, last, mirror), FaceId::U, FaceId::R),
        edge_position(coord3(row, last, 0), FaceId::U, FaceId::B),
        edge_position(coord3(mirror, last, 0), FaceId::U, FaceId::B),
        edge_position(coord3(0, last, row), FaceId::U, FaceId::L),
        edge_position(coord3(0, last, mirror), FaceId::U, FaceId::L),
        edge_position(coord3(last, row, last), FaceId::F, FaceId::R),
        edge_position(coord3(last, mirror, last), FaceId::F, FaceId::R),
        edge_position(coord3(0, row, last), FaceId::F, FaceId::L),
        edge_position(coord3(0, mirror, last), FaceId::F, FaceId::L),
        edge_position(coord3(last, row, 0), FaceId::B, FaceId::R),
        edge_position(coord3(last, mirror, 0), FaceId::B, FaceId::R),
        edge_position(coord3(0, row, 0), FaceId::B, FaceId::L),
        edge_position(coord3(0, mirror, 0), FaceId::B, FaceId::L),
        edge_position(coord3(row, 0, last), FaceId::D, FaceId::F),
        edge_position(coord3(mirror, 0, last), FaceId::D, FaceId::F),
        edge_position(coord3(last, 0, row), FaceId::D, FaceId::R),
        edge_position(coord3(last, 0, mirror), FaceId::D, FaceId::R),
        edge_position(coord3(row, 0, 0), FaceId::D, FaceId::B),
        edge_position(coord3(mirror, 0, 0), FaceId::D, FaceId::B),
        edge_position(coord3(0, 0, row), FaceId::D, FaceId::L),
        edge_position(coord3(0, 0, mirror), FaceId::D, FaceId::L),
    ]
}

fn edge_position(coord: geometry::Coord3, first: FaceId, second: FaceId) -> EdgePosition {
    EdgePosition {
        coord,
        faces: normalize_face_pair(first, second),
    }
}

fn coord3(x: usize, y: usize, z: usize) -> geometry::Coord3 {
    geometry::Coord3 { x, y, z }
}

fn edge_cubie_from_position(side_length: usize, position: EdgePosition) -> EdgeCubieLocation {
    let (face, _) = position.faces;
    let (row, col) = geometry::coord_to_logical(face, position.coord, side_length);
    edge_cubie_for_facelet_location(side_length, FaceletLocation { face, row, col })
        .expect("edge position must decode to an edge cubie")
}

fn edge_position_from_cubie(side_length: usize, cubie: EdgeCubieLocation) -> EdgePosition {
    let [first, second] = cubie.stickers();
    EdgePosition {
        coord: geometry::logical_to_coord(first.face, first.row, first.col, side_length),
        faces: normalize_face_pair(first.face, second.face),
    }
}

fn wing_position_types(side_length: usize, positions: &[EdgePosition]) -> Vec<bool> {
    let constraints = wing_orientation_constraints(side_length, positions);
    let mut types = vec![None; positions.len()];
    types[0] = Some(false);

    let mut changed = true;
    while changed {
        changed = false;
        for &(source, destination, reversed) in &constraints {
            if let Some(source_type) = types[source] {
                let destination_type = source_type ^ reversed;
                match types[destination] {
                    Some(existing) => {
                        assert_eq!(existing, destination_type, "inconsistent wing type graph");
                    }
                    None => {
                        types[destination] = Some(destination_type);
                        changed = true;
                    }
                }
            }

            if let Some(destination_type) = types[destination] {
                let source_type = destination_type ^ reversed;
                match types[source] {
                    Some(existing) => {
                        assert_eq!(existing, source_type, "inconsistent wing type graph");
                    }
                    None => {
                        types[source] = Some(source_type);
                        changed = true;
                    }
                }
            }
        }
    }

    types
        .into_iter()
        .map(|entry| entry.expect("wing type graph must connect every position"))
        .collect()
}

fn wing_orientation_constraints(
    side_length: usize,
    positions: &[EdgePosition],
) -> Vec<(usize, usize, bool)> {
    let mut constraints = Vec::new();

    for axis in [Axis::X, Axis::Y, Axis::Z] {
        for depth in 0..side_length {
            let mv = Move::new(axis, depth, MoveAngle::Positive);
            for (source_index, source_position) in positions.iter().copied().enumerate() {
                let source = edge_cubie_from_position(side_length, source_position);
                let [source_first, source_second] = source.stickers();
                let traced_first =
                    trace_facelet_location_through_move(side_length, source_first, mv);
                let traced_second =
                    trace_facelet_location_through_move(side_length, source_second, mv);
                let destination = edge_cubie_for_facelet_location(side_length, traced_first)
                    .expect("traced wing sticker must remain on an edge cubie");
                let destination_position = edge_position_from_cubie(side_length, destination);
                let destination_index = positions
                    .iter()
                    .position(|position| *position == destination_position)
                    .expect("traced wing cubie must remain in the same wing orbit");
                let [destination_first, destination_second] = destination.stickers();

                let reversed = if traced_first == destination_first
                    && traced_second == destination_second
                {
                    false
                } else if traced_first == destination_second && traced_second == destination_first {
                    true
                } else {
                    panic!("traced wing stickers did not map onto one destination cubie");
                };

                constraints.push((source_index, destination_index, reversed));
            }
        }
    }

    constraints
}

fn random_permutation<R: RandomSource>(len: usize, rng: &mut R) -> Vec<usize> {
    let mut values = (0..len).collect::<Vec<_>>();

    for i in (1..len).rev() {
        let j = random_below(rng, i + 1);
        values.swap(i, j);
    }

    values
}

fn random_below<R: RandomSource>(rng: &mut R, upper: usize) -> usize {
    assert!(upper > 0, "random upper bound must be positive");
    let upper = upper as u64;
    let zone = u64::MAX - (u64::MAX % upper);

    loop {
        let value = rng.next_u64();
        if value < zone {
            return (value % upper) as usize;
        }
    }
}

fn force_permutation_parity(permutation: &mut [usize], odd: bool) {
    if permutation_is_odd(permutation) == odd {
        return;
    }
    assert!(
        permutation.len() >= 2,
        "cannot force odd parity on a singleton permutation"
    );
    permutation.swap(0, 1);
}

fn permutation_is_odd(permutation: &[usize]) -> bool {
    let mut odd = false;
    for i in 0..permutation.len() {
        for j in i + 1..permutation.len() {
            if permutation[i] > permutation[j] {
                odd = !odd;
            }
        }
    }
    odd
}
