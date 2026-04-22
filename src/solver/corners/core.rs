use std::collections::VecDeque;

use crate::{
    conventions::{face_axis, face_outer_move, home_facelet_for_face},
    cube::{trace_facelet_location_through_move, Cube},
    face::FaceId,
    facelet::Facelet,
    moves::{Move, MoveAngle},
    storage::FaceletArray,
};

#[cfg(test)]
use crate::cube::{
    corner_cubie_for_facelet_location, trace_corner_cubie_through_move, CornerCubieLocation,
};

use super::CornerSlot;

#[cfg(test)]
use super::super::{SolveContext, SolverStage};
#[cfg(test)]
use super::CornerReductionStage;

const CORNER_ORIENTATION_STATE_COUNT: usize = 2_187;
const CORNER_PERMUTATION_STATE_COUNT: usize = 40_320;
const CORNER_MOVE_COUNT: usize = 18;
const FACTORIALS: [usize; 9] = [1, 1, 2, 6, 24, 120, 720, 5_040, 40_320];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) struct CornerState {
    permutation: [u8; 8],
    orientation: [u8; 8],
}

impl CornerState {
    const SOLVED: Self = Self {
        permutation: [0, 1, 2, 3, 4, 5, 6, 7],
        orientation: [0; 8],
    };

    fn is_solved(self) -> bool {
        self == Self::SOLVED
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(super) struct CornerMoveSpec {
    face: FaceId,
    angle: MoveAngle,
}

impl CornerMoveSpec {
    const fn new(face: FaceId, angle: MoveAngle) -> Self {
        Self { face, angle }
    }

    pub(super) fn move_for_side_length(self, side_length: usize) -> Move {
        face_outer_move(side_length, self.face, self.angle)
    }
}

const CORNER_MOVE_SPECS: [CornerMoveSpec; CORNER_MOVE_COUNT] = [
    CornerMoveSpec::new(FaceId::U, MoveAngle::Positive),
    CornerMoveSpec::new(FaceId::U, MoveAngle::Double),
    CornerMoveSpec::new(FaceId::U, MoveAngle::Negative),
    CornerMoveSpec::new(FaceId::D, MoveAngle::Positive),
    CornerMoveSpec::new(FaceId::D, MoveAngle::Double),
    CornerMoveSpec::new(FaceId::D, MoveAngle::Negative),
    CornerMoveSpec::new(FaceId::R, MoveAngle::Positive),
    CornerMoveSpec::new(FaceId::R, MoveAngle::Double),
    CornerMoveSpec::new(FaceId::R, MoveAngle::Negative),
    CornerMoveSpec::new(FaceId::L, MoveAngle::Positive),
    CornerMoveSpec::new(FaceId::L, MoveAngle::Double),
    CornerMoveSpec::new(FaceId::L, MoveAngle::Negative),
    CornerMoveSpec::new(FaceId::F, MoveAngle::Positive),
    CornerMoveSpec::new(FaceId::F, MoveAngle::Double),
    CornerMoveSpec::new(FaceId::F, MoveAngle::Negative),
    CornerMoveSpec::new(FaceId::B, MoveAngle::Positive),
    CornerMoveSpec::new(FaceId::B, MoveAngle::Double),
    CornerMoveSpec::new(FaceId::B, MoveAngle::Negative),
];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct CornerMoveEffect {
    destination: [u8; 8],
    orientation: [[u8; 3]; 8],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CornerMoveTables {
    permutation_move: Vec<[u16; CORNER_MOVE_COUNT]>,
    orientation_move: Vec<[u16; CORNER_MOVE_COUNT]>,
    permutation_distance: Vec<u8>,
    orientation_distance: Vec<u8>,
}

impl CornerMoveTables {
    pub(super) fn new() -> Self {
        let effects = CORNER_MOVE_SPECS.map(build_corner_move_effect);

        let mut permutation_move = vec![[0u16; CORNER_MOVE_COUNT]; CORNER_PERMUTATION_STATE_COUNT];
        for index in 0..CORNER_PERMUTATION_STATE_COUNT {
            let permutation = decode_corner_permutation(index);
            for (move_index, effect) in effects.iter().copied().enumerate() {
                permutation_move[index][move_index] =
                    encode_corner_permutation(apply_permutation_move(permutation, effect)) as u16;
            }
        }

        let mut orientation_move = vec![[0u16; CORNER_MOVE_COUNT]; CORNER_ORIENTATION_STATE_COUNT];
        for index in 0..CORNER_ORIENTATION_STATE_COUNT {
            let orientation = decode_corner_orientation(index);
            for (move_index, effect) in effects.iter().copied().enumerate() {
                orientation_move[index][move_index] =
                    encode_corner_orientation(apply_orientation_move(orientation, effect)) as u16;
            }
        }

        let permutation_distance = build_distance_table(&permutation_move);
        let orientation_distance = build_distance_table(&orientation_move);

        Self {
            permutation_move,
            orientation_move,
            permutation_distance,
            orientation_distance,
        }
    }

    pub(super) fn solve(&self, state: CornerState) -> Option<Vec<CornerMoveSpec>> {
        if state.is_solved() {
            return Some(Vec::new());
        }

        let permutation_index = encode_corner_permutation(state.permutation);
        let orientation_index = encode_corner_orientation(state.orientation);
        let mut depth = self.heuristic(permutation_index, orientation_index) as usize;
        let mut path = Vec::new();

        while depth <= 14 {
            if self.search(permutation_index, orientation_index, depth, None, &mut path) {
                return Some(
                    path.into_iter()
                        .map(|move_index| CORNER_MOVE_SPECS[move_index])
                        .collect(),
                );
            }
            depth += 1;
        }

        None
    }

    fn search(
        &self,
        permutation_index: usize,
        orientation_index: usize,
        depth_remaining: usize,
        previous_face: Option<FaceId>,
        path: &mut Vec<usize>,
    ) -> bool {
        let heuristic = self.heuristic(permutation_index, orientation_index) as usize;
        if heuristic > depth_remaining {
            return false;
        }

        if depth_remaining == 0 {
            return permutation_index == 0 && orientation_index == 0;
        }

        for (move_index, spec) in CORNER_MOVE_SPECS.iter().copied().enumerate() {
            if move_is_redundant(previous_face, spec.face) {
                continue;
            }

            path.push(move_index);

            let next_permutation = self.permutation_move[permutation_index][move_index] as usize;
            let next_orientation = self.orientation_move[orientation_index][move_index] as usize;
            if self.search(
                next_permutation,
                next_orientation,
                depth_remaining - 1,
                Some(spec.face),
                path,
            ) {
                return true;
            }

            path.pop();
        }

        false
    }

    fn heuristic(&self, permutation_index: usize, orientation_index: usize) -> u8 {
        self.permutation_distance[permutation_index]
            .max(self.orientation_distance[orientation_index])
    }
}

fn build_corner_move_effect(spec: CornerMoveSpec) -> CornerMoveEffect {
    let mv = spec.move_for_side_length(3);
    let mut destination = [0u8; 8];
    let mut orientation = [[0u8; 3]; 8];

    for slot in CornerSlot::ALL {
        let stickers = slot.stickers(3);
        let traced = stickers.map(|location| trace_facelet_location_through_move(3, location, mv));
        let destination_slot =
            CornerSlot::from_faces(traced[0].face, traced[1].face, traced[2].face)
                .expect("traced corner must land on a corner slot");
        let destination_faces = destination_slot.faces();

        destination[slot.index()] = destination_slot.index() as u8;
        for source_orientation in 0..3 {
            orientation[slot.index()][source_orientation] = destination_faces
                .iter()
                .position(|face| *face == traced[source_orientation].face)
                .expect("traced corner sticker must land on a destination-slot face")
                as u8;
        }
    }

    CornerMoveEffect {
        destination,
        orientation,
    }
}

fn apply_permutation_move(permutation: [u8; 8], effect: CornerMoveEffect) -> [u8; 8] {
    let mut next = [0u8; 8];

    for source_slot in 0..8 {
        let destination_slot = effect.destination[source_slot] as usize;
        next[destination_slot] = permutation[source_slot];
    }

    next
}

fn apply_orientation_move(orientation: [u8; 8], effect: CornerMoveEffect) -> [u8; 8] {
    let mut next = [0u8; 8];

    for source_slot in 0..8 {
        let destination_slot = effect.destination[source_slot] as usize;
        next[destination_slot] = effect.orientation[source_slot][orientation[source_slot] as usize];
    }

    next
}

fn build_distance_table(move_table: &[[u16; CORNER_MOVE_COUNT]]) -> Vec<u8> {
    let mut distance = vec![u8::MAX; move_table.len()];
    let mut queue = VecDeque::new();

    distance[0] = 0;
    queue.push_back(0usize);

    while let Some(state) = queue.pop_front() {
        let next_distance = distance[state] + 1;
        for &next in &move_table[state] {
            let next = next as usize;
            if distance[next] != u8::MAX {
                continue;
            }
            distance[next] = next_distance;
            queue.push_back(next);
        }
    }

    distance
}

fn move_is_redundant(previous_face: Option<FaceId>, next_face: FaceId) -> bool {
    let Some(previous_face) = previous_face else {
        return false;
    };

    if previous_face == next_face {
        return true;
    }

    face_axis(previous_face) == face_axis(next_face) && previous_face.index() > next_face.index()
}

fn encode_corner_orientation(orientation: [u8; 8]) -> usize {
    let mut index = 0usize;

    for value in orientation.iter().take(7).copied() {
        index = index * 3 + value as usize;
    }

    index
}

fn decode_corner_orientation(mut index: usize) -> [u8; 8] {
    let mut orientation = [0u8; 8];
    let mut sum = 0usize;

    for slot in (0..7).rev() {
        orientation[slot] = (index % 3) as u8;
        sum += orientation[slot] as usize;
        index /= 3;
    }

    orientation[7] = ((3 - (sum % 3)) % 3) as u8;
    orientation
}

fn encode_corner_permutation(permutation: [u8; 8]) -> usize {
    let mut index = 0usize;

    for slot in 0..8 {
        let mut smaller = 0usize;
        for other in slot + 1..8 {
            if permutation[other] < permutation[slot] {
                smaller += 1;
            }
        }
        index += smaller * FACTORIALS[7 - slot];
    }

    index
}

fn decode_corner_permutation(mut index: usize) -> [u8; 8] {
    let mut permutation = [0u8; 8];
    let mut available = Vec::from([0u8, 1, 2, 3, 4, 5, 6, 7]);

    for slot in 0..8 {
        let factor = FACTORIALS[7 - slot];
        let pick = index / factor;
        index %= factor;
        permutation[slot] = available.remove(pick);
    }

    permutation
}

pub(super) fn read_corner_state<S: FaceletArray>(cube: &Cube<S>) -> Option<CornerState> {
    if cube.side_len() < 2 {
        return Some(CornerState::SOLVED);
    }

    let mut permutation = [0u8; 8];
    let mut orientation = [0u8; 8];
    let mut seen = [false; 8];

    for slot in CornerSlot::ALL {
        let colors = slot
            .stickers(cube.side_len())
            .map(|location| cube.face(location.face).get(location.row, location.col));
        let cubie = corner_cubie_index_from_colors(colors)?;
        if seen[cubie] {
            return None;
        }
        seen[cubie] = true;

        let ud_color = home_facelet_for_face(CornerSlot::ALL[cubie].faces()[0]);
        let twist = colors.iter().position(|color| *color == ud_color)? as u8;

        permutation[slot.index()] = cubie as u8;
        orientation[slot.index()] = twist;
    }

    if !seen.into_iter().all(|entry| entry) {
        return None;
    }
    if orientation.iter().copied().map(usize::from).sum::<usize>() % 3 != 0 {
        return None;
    }
    Some(CornerState {
        permutation,
        orientation,
    })
}

fn corner_cubie_index_from_colors(colors: [Facelet; 3]) -> Option<usize> {
    let key = corner_color_key(colors);
    CornerSlot::ALL
        .iter()
        .position(|slot| corner_color_key(slot.faces().map(home_facelet_for_face)) == key)
}

fn corner_color_key(colors: [Facelet; 3]) -> [u8; 3] {
    let mut key = colors.map(Facelet::as_u8);
    key.sort_unstable();
    key
}

pub(super) fn all_corner_facelets_solved<S: FaceletArray>(cube: &Cube<S>) -> bool {
    let side_length = cube.side_len();
    if side_length < 2 {
        return true;
    }

    for slot in CornerSlot::ALL {
        let faces = slot.faces();
        let stickers = slot.stickers(side_length);

        for (face, location) in faces.into_iter().zip(stickers) {
            if cube.face(location.face).get(location.row, location.col)
                != home_facelet_for_face(face)
            {
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
fn read_corner_cubies(side_length: usize) -> [CornerCubieLocation; 8] {
    CornerSlot::ALL.map(|slot| {
        let anchor = slot.stickers(side_length)[0];
        corner_cubie_for_facelet_location(side_length, anchor)
            .expect("corner slot anchor must decode to a valid corner cubie")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        solver::{CenterReductionStage, EdgePairingStage, ReductionSolver},
        Byte, SolveOptions, Solver, XorShift64,
    };

    #[test]
    fn corner_move_tables_match_single_outer_moves() {
        let solved = CornerState::SOLVED;

        for side_length in 2..=8 {
            for (move_index, spec) in CORNER_MOVE_SPECS.iter().copied().enumerate() {
                let mut cube = Cube::<Byte>::new_solved(side_length);
                let mv = spec.move_for_side_length(side_length);
                cube.apply_move_untracked(mv);

                let actual =
                    read_corner_state(&cube).expect("outer move must preserve corner state");
                let expected = CornerState {
                    permutation: apply_permutation_move(
                        solved.permutation,
                        build_corner_move_effect(spec),
                    ),
                    orientation: apply_orientation_move(
                        solved.orientation,
                        build_corner_move_effect(spec),
                    ),
                };

                assert_eq!(
                    actual, expected,
                    "corner move table mismatch for n={side_length}, move_index={move_index}, move={mv}"
                );
            }
        }
    }

    #[test]
    fn corner_cubie_tracing_matches_outer_move_positions() {
        for side_length in 2..=8 {
            let before = read_corner_cubies(side_length);

            for spec in CORNER_MOVE_SPECS {
                let mv = spec.move_for_side_length(side_length);
                let mut cube = Cube::<Byte>::new_solved(side_length);
                cube.apply_move_untracked(mv);
                let after = read_corner_cubies(side_length);

                for cubie in before {
                    let traced = trace_corner_cubie_through_move(side_length, cubie, mv);
                    assert!(
                        after.contains(&traced),
                        "traced corner cubie must exist after applying {mv} on n={side_length}"
                    );
                }
            }
        }
    }

    #[test]
    fn corner_stage_recorded_moves_replay_to_same_full_cube_state() {
        for side_length in 2..=8 {
            for seed in [0xC0A2_EE11u64, 0xC0A2_EE22u64] {
                let mut cube = Cube::<Byte>::new_solved(side_length);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                cube.scramble(&mut rng);
                let initial = cube.clone();
                let history_before = cube.history().len();
                let history_before_moves = initial.history().as_slice().to_vec();

                let mut stage = CornerReductionStage::default();
                let mut context = SolveContext::new(SolveOptions { record_moves: true });
                <CornerReductionStage as SolverStage<Byte>>::run(
                    &mut stage,
                    &mut cube,
                    &mut context,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "corner stage failed for n={side_length}, seed={seed:#x}: {error}\n{}",
                        cube.net_string(),
                    )
                });

                let mut replay = initial;
                replay.apply_moves_untracked(context.moves().iter().copied());

                assert_cubes_match(&cube, &replay);
                assert!(all_corner_facelets_solved(&cube));
                assert_eq!(cube.history().len(), history_before);
                assert_eq!(cube.history().as_slice(), history_before_moves.as_slice());
            }
        }
    }

    #[test]
    fn corner_stage_solves_scrambled_corners_for_sizes_two_to_eight() {
        for side_length in 2..=8 {
            for seed in [0xC0A2_51DEu64, 0xC0A2_5EEDu64] {
                let mut cube = Cube::<Byte>::new_solved(side_length);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                cube.scramble(&mut rng);

                let mut stage = CornerReductionStage::default();
                let mut context = SolveContext::new(SolveOptions {
                    record_moves: false,
                });
                <CornerReductionStage as SolverStage<Byte>>::run(
                    &mut stage,
                    &mut cube,
                    &mut context,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "corner stage failed for n={side_length}, seed={seed:#x}: {error}\n{}",
                        cube.net_string(),
                    )
                });

                assert!(all_corner_facelets_solved(&cube));
            }
        }
    }

    #[test]
    fn full_default_pipeline_solves_scrambled_cubes_from_one_to_eight() {
        for side_length in 1..=8 {
            let mut cube = Cube::<Byte>::new_solved(side_length);
            let mut rng = XorShift64::new(0x5017_C0DE ^ side_length as u64);
            cube.scramble(&mut rng);

            let mut solver = ReductionSolver::<Byte>::new(SolveOptions {
                record_moves: false,
            })
            .with_stage(CenterReductionStage::western_default())
            .with_stage(CornerReductionStage::default())
            .with_stage(EdgePairingStage::default());

            solver.solve(&mut cube).unwrap_or_else(|error| {
                panic!(
                    "full pipeline failed for n={side_length}: {error}\n{}",
                    cube.net_string(),
                )
            });

            assert!(
                cube.is_solved(),
                "full solver did not solve n={side_length}"
            );
        }
    }

    fn assert_cubes_match<A: FaceletArray, B: FaceletArray>(actual: &Cube<A>, expected: &Cube<B>) {
        assert_eq!(actual.side_len(), expected.side_len());

        for face in FaceId::ALL {
            assert_eq!(
                actual.face(face).rotation(),
                expected.face(face).rotation(),
                "face rotation mismatch on {face}"
            );

            for row in 0..actual.side_len() {
                for col in 0..actual.side_len() {
                    assert_eq!(
                        actual.face(face).get(row, col),
                        expected.face(face).get(row, col),
                        "facelet mismatch on {face} at ({row}, {col})"
                    );
                }
            }
        }
    }
}
