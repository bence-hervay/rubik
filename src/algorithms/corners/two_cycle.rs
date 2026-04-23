use std::{collections::VecDeque, sync::OnceLock};

use crate::{
    algorithms::{
        AlgorithmContract, AlgorithmExecutionSupport, AlgorithmSideLengthSupport,
        AlgorithmStepSpec, MoveSequenceOperation, SolveAlgorithm,
    },
    cube::Cube,
    face::FaceId,
    moves::MoveAngle,
    solver::{SolveContext, SolveError, SolvePhase, SolveResult},
    storage::FaceletArray,
};

use super::{
    search::{
        all_corner_facelets_solved, apply_corner_effect, corner_move_destinations,
        corner_move_effects, read_corner_state, CornerMoveEffect, CornerMoveSpec, CornerState,
        CORNER_MOVE_SPECS,
    },
    CornerSlot,
};

const CANONICAL_FIRST_SLOT: CornerSlot = CornerSlot::UFR;
const CANONICAL_SECOND_SLOT: CornerSlot = CornerSlot::ULF;
const ORDERED_SLOT_PAIR_COUNT: usize = CornerSlot::ALL.len() * CornerSlot::ALL.len();

const fn corner_move(face: FaceId, angle: MoveAngle) -> CornerMoveSpec {
    CornerMoveSpec::new(face, angle)
}

const CORNER_SWAP_RECIPE: [CornerMoveSpec; 10] = [
    corner_move(FaceId::R, MoveAngle::Negative),
    corner_move(FaceId::F, MoveAngle::Positive),
    corner_move(FaceId::R, MoveAngle::Negative),
    corner_move(FaceId::F, MoveAngle::Double),
    corner_move(FaceId::R, MoveAngle::Positive),
    corner_move(FaceId::U, MoveAngle::Negative),
    corner_move(FaceId::R, MoveAngle::Negative),
    corner_move(FaceId::F, MoveAngle::Double),
    corner_move(FaceId::R, MoveAngle::Double),
    corner_move(FaceId::U, MoveAngle::Negative),
];

const CORNER_TWIST_RECIPE: [CornerMoveSpec; 10] = [
    corner_move(FaceId::R, MoveAngle::Positive),
    corner_move(FaceId::F, MoveAngle::Negative),
    corner_move(FaceId::R, MoveAngle::Positive),
    corner_move(FaceId::F, MoveAngle::Negative),
    corner_move(FaceId::U, MoveAngle::Positive),
    corner_move(FaceId::R, MoveAngle::Positive),
    corner_move(FaceId::U, MoveAngle::Double),
    corner_move(FaceId::F, MoveAngle::Double),
    corner_move(FaceId::R, MoveAngle::Negative),
    corner_move(FaceId::U, MoveAngle::Negative),
];

#[derive(Clone, Debug, Eq, PartialEq)]
struct CornerTwoCycleSetupTable {
    ordered_pair_setups: Vec<Vec<CornerMoveSpec>>,
    swap_sequences: Vec<Vec<CornerMoveSpec>>,
    twist_positive_sequences: Vec<Vec<CornerMoveSpec>>,
    twist_negative_sequences: Vec<Vec<CornerMoveSpec>>,
}

impl CornerTwoCycleSetupTable {
    fn new() -> Self {
        let ordered_pair_setups = build_ordered_pair_setups();
        let swap_inverse = invert_specs(&CORNER_SWAP_RECIPE);
        let twist_inverse = invert_specs(&CORNER_TWIST_RECIPE);
        let mut swap_sequences = vec![Vec::new(); ORDERED_SLOT_PAIR_COUNT];
        let mut twist_positive_sequences = vec![Vec::new(); ORDERED_SLOT_PAIR_COUNT];
        let mut twist_negative_sequences = vec![Vec::new(); ORDERED_SLOT_PAIR_COUNT];

        for first in CornerSlot::ALL {
            for second in CornerSlot::ALL {
                if first == second {
                    continue;
                }

                let key = ordered_slot_pair_key(first, second);
                let forward_setup = &ordered_pair_setups[key];
                let reverse_setup = &ordered_pair_setups[ordered_slot_pair_key(second, first)];

                swap_sequences[key] = choose_shortest_sequence([
                    conjugated_sequence(forward_setup, &CORNER_SWAP_RECIPE),
                    conjugated_sequence(forward_setup, &swap_inverse),
                    conjugated_sequence(reverse_setup, &CORNER_SWAP_RECIPE),
                    conjugated_sequence(reverse_setup, &swap_inverse),
                ]);
                twist_positive_sequences[key] = choose_shortest_sequence([
                    conjugated_sequence(forward_setup, &CORNER_TWIST_RECIPE),
                    conjugated_sequence(reverse_setup, &twist_inverse),
                ]);
                twist_negative_sequences[key] = choose_shortest_sequence([
                    conjugated_sequence(forward_setup, &twist_inverse),
                    conjugated_sequence(reverse_setup, &CORNER_TWIST_RECIPE),
                ]);
            }
        }

        Self {
            ordered_pair_setups,
            swap_sequences,
            twist_positive_sequences,
            twist_negative_sequences,
        }
    }

    #[cfg(test)]
    fn setup_for_ordered_pair(&self, first: CornerSlot, second: CornerSlot) -> &[CornerMoveSpec] {
        &self.ordered_pair_setups[ordered_slot_pair_key(first, second)]
    }

    fn swap_sequence(&self, first: CornerSlot, second: CornerSlot) -> &[CornerMoveSpec] {
        &self.swap_sequences[ordered_slot_pair_key(first, second)]
    }

    fn twist_sequence(
        &self,
        first: CornerSlot,
        second: CornerSlot,
        first_delta: u8,
    ) -> &[CornerMoveSpec] {
        let key = ordered_slot_pair_key(first, second);
        match first_delta {
            1 => &self.twist_positive_sequences[key],
            2 => &self.twist_negative_sequences[key],
            _ => panic!("corner twist delta must be either 1 or 2"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CornerTwoCycleReductionAlgorithm {
    steps: [AlgorithmStepSpec; 5],
}

pub type CornerTwoCycleReductionStage = CornerTwoCycleReductionAlgorithm;

const CORNER_TWO_CYCLE_STANDARD_PRECONDITIONS: &[&str] =
    &["none; the two-cycle corner stage may start from any cube state"];
const CORNER_TWO_CYCLE_STANDARD_POSTCONDITIONS: &[&str] =
    &["all corner facelets are solved when the stage returns success"];
const CORNER_TWO_CYCLE_ALGORITHM_CONTRACT: AlgorithmContract = AlgorithmContract::new(
    AlgorithmSideLengthSupport::all(),
    false,
    CORNER_TWO_CYCLE_STANDARD_PRECONDITIONS,
    CORNER_TWO_CYCLE_STANDARD_POSTCONDITIONS,
    AlgorithmExecutionSupport::StandardAndOptimized,
);

impl Default for CornerTwoCycleReductionAlgorithm {
    fn default() -> Self {
        Self {
            steps: [
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner state extraction",
                    "read corner permutation and orientation from the current cube state",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner setup table",
                    "reuse shortest ordered-pair setup sequences for the canonical working pair",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner transpositions",
                    "place one corner cubie into its home slot per exact two-cycle iteration",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner twists",
                    "solve the remaining corner orientations with paired opposite twists",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner validation",
                    "verify that every corner facelet matches its home face color",
                ),
            ],
        }
    }
}

impl<S: FaceletArray> SolveAlgorithm<S> for CornerTwoCycleReductionAlgorithm {
    fn phase(&self) -> SolvePhase {
        SolvePhase::Corners
    }

    fn name(&self) -> &'static str {
        "corner reduction"
    }

    fn contract(&self) -> AlgorithmContract {
        CORNER_TWO_CYCLE_ALGORITHM_CONTRACT
    }

    fn steps(&self) -> &[AlgorithmStepSpec] {
        &self.steps
    }

    fn run(&mut self, cube: &mut Cube<S>, context: &mut SolveContext) -> SolveResult<()> {
        if cube.side_len() < 2 || all_corner_facelets_solved(cube) {
            return Ok(());
        }

        let mut state = read_corner_state(cube).ok_or(SolveError::StageFailed {
            stage: "corner reduction",
            reason: "could not read a valid reduced corner state",
        })?;
        let table = corner_two_cycle_setup_table();
        let mut solution = Vec::new();

        solve_corner_permutation(&mut state, table, &mut solution);
        solve_corner_orientation(&mut state, table, &mut solution);
        debug_assert_eq!(state.permutation, [0, 1, 2, 3, 4, 5, 6, 7]);
        debug_assert_eq!(state.orientation, [0; 8]);

        let side_length = cube.side_len();
        let moves = solution
            .into_iter()
            .map(|spec| spec.move_for_side_length(side_length))
            .collect::<Vec<_>>();
        let operation = MoveSequenceOperation::new(side_length, &moves);
        context.apply_operation(cube, &operation);

        if all_corner_facelets_solved(cube) {
            Ok(())
        } else {
            Err(SolveError::StageFailed {
                stage: "corner reduction",
                reason: "corner solving left some corner facelets unsolved",
            })
        }
    }
}

fn solve_corner_permutation(
    state: &mut CornerState,
    table: &CornerTwoCycleSetupTable,
    solution: &mut Vec<CornerMoveSpec>,
) {
    for slot in CornerSlot::ALL {
        if state.cubie_at(slot) == slot.index() {
            continue;
        }

        let source = state.slot_for_cubie(slot.index());
        let sequence = table.swap_sequence(slot, source);
        solution.extend_from_slice(sequence);
        *state = apply_sequence_to_state(*state, sequence);
    }
}

fn solve_corner_orientation(
    state: &mut CornerState,
    table: &CornerTwoCycleSetupTable,
    solution: &mut Vec<CornerMoveSpec>,
) {
    while let Some(first) = CornerSlot::ALL
        .into_iter()
        .find(|slot| state.orientation_at(*slot) != 0)
    {
        let first_orientation = state.orientation_at(first);
        let first_delta = (3 - first_orientation) % 3;
        debug_assert!(matches!(first_delta, 1 | 2));

        let complement = opposite_twist_delta(first_delta);
        let second = CornerSlot::ALL
            .into_iter()
            .find(|slot| *slot != first && state.orientation_at(*slot) == complement)
            .or_else(|| {
                CornerSlot::ALL
                    .into_iter()
                    .find(|slot| *slot != first && state.orientation_at(*slot) != 0)
            })
            .expect("unsolved corner orientation must have a partner");

        let sequence = table.twist_sequence(first, second, first_delta);
        solution.extend_from_slice(sequence);
        *state = apply_sequence_to_state(*state, sequence);
    }
}

fn corner_two_cycle_setup_table() -> &'static CornerTwoCycleSetupTable {
    static TABLE: OnceLock<CornerTwoCycleSetupTable> = OnceLock::new();
    TABLE.get_or_init(CornerTwoCycleSetupTable::new)
}

fn build_ordered_pair_setups() -> Vec<Vec<CornerMoveSpec>> {
    let destinations = corner_move_destinations();
    let start = ordered_slot_pair_key(CANONICAL_FIRST_SLOT, CANONICAL_SECOND_SLOT);
    let mut visited = [false; ORDERED_SLOT_PAIR_COUNT];
    let mut predecessor = [usize::MAX; ORDERED_SLOT_PAIR_COUNT];
    let mut predecessor_move = [usize::MAX; ORDERED_SLOT_PAIR_COUNT];
    let mut queue = VecDeque::new();

    visited[start] = true;
    queue.push_back((CANONICAL_FIRST_SLOT, CANONICAL_SECOND_SLOT));

    while let Some((first, second)) = queue.pop_front() {
        let current = ordered_slot_pair_key(first, second);

        for (move_index, destination) in destinations.iter().enumerate() {
            let next_first = CornerSlot::ALL[destination[first.index()] as usize];
            let next_second = CornerSlot::ALL[destination[second.index()] as usize];
            if next_first == next_second {
                continue;
            }

            let next = ordered_slot_pair_key(next_first, next_second);
            if visited[next] {
                continue;
            }

            visited[next] = true;
            predecessor[next] = current;
            predecessor_move[next] = move_index;
            queue.push_back((next_first, next_second));
        }
    }

    let mut setups = vec![Vec::new(); ORDERED_SLOT_PAIR_COUNT];
    for first in CornerSlot::ALL {
        for second in CornerSlot::ALL {
            if first == second {
                continue;
            }

            let key = ordered_slot_pair_key(first, second);
            assert!(visited[key], "ordered corner-slot pair must be reachable");

            let mut path = Vec::new();
            let mut cursor = key;
            while cursor != start {
                let move_index = predecessor_move[cursor];
                path.push(CORNER_MOVE_SPECS[move_index]);
                cursor = predecessor[cursor];
            }
            path.reverse();
            setups[key] = path;
        }
    }

    setups
}

fn conjugated_sequence(setup: &[CornerMoveSpec], body: &[CornerMoveSpec]) -> Vec<CornerMoveSpec> {
    let mut sequence = Vec::with_capacity(setup.len() * 2 + body.len());
    sequence.extend(invert_specs(setup));
    sequence.extend_from_slice(body);
    sequence.extend_from_slice(setup);
    simplify_specs(sequence)
}

fn choose_shortest_sequence<const N: usize>(
    candidates: [Vec<CornerMoveSpec>; N],
) -> Vec<CornerMoveSpec> {
    let mut best: Option<Vec<CornerMoveSpec>> = None;

    for candidate in candidates {
        match &best {
            Some(current) if current.len() <= candidate.len() => {}
            _ => best = Some(candidate),
        }
    }

    best.expect("candidate set must not be empty")
}

fn invert_specs(specs: &[CornerMoveSpec]) -> Vec<CornerMoveSpec> {
    specs
        .iter()
        .rev()
        .copied()
        .map(|spec| CornerMoveSpec::new(spec.face(), spec.angle().inverse()))
        .collect()
}

fn apply_sequence_to_state(mut state: CornerState, specs: &[CornerMoveSpec]) -> CornerState {
    let effects = canonical_corner_move_effects();
    for spec in specs {
        state = apply_corner_effect(state, effects[corner_move_spec_index(*spec)]);
    }
    state
}

fn canonical_corner_move_effects() -> &'static [CornerMoveEffect; CORNER_MOVE_SPECS.len()] {
    static EFFECTS: OnceLock<[CornerMoveEffect; CORNER_MOVE_SPECS.len()]> = OnceLock::new();
    EFFECTS.get_or_init(corner_move_effects)
}

fn corner_move_spec_index(spec: CornerMoveSpec) -> usize {
    CORNER_MOVE_SPECS
        .iter()
        .position(|candidate| *candidate == spec)
        .expect("corner move spec must exist in the canonical move list")
}

fn simplify_specs(specs: Vec<CornerMoveSpec>) -> Vec<CornerMoveSpec> {
    let mut simplified: Vec<CornerMoveSpec> = Vec::with_capacity(specs.len());

    for spec in specs {
        if let Some(previous) = simplified.last_mut() {
            if previous.face() == spec.face() {
                match combine_same_face_turns(previous.angle(), spec.angle()) {
                    Some(angle) => *previous = CornerMoveSpec::new(previous.face(), angle),
                    None => {
                        simplified.pop();
                    }
                }
                continue;
            }
        }

        simplified.push(spec);
    }

    simplified
}

fn combine_same_face_turns(first: MoveAngle, second: MoveAngle) -> Option<MoveAngle> {
    match (u16::from(first.as_u8()) + u16::from(second.as_u8())) % 4 {
        0 => None,
        1 => Some(MoveAngle::Positive),
        2 => Some(MoveAngle::Double),
        3 => Some(MoveAngle::Negative),
        _ => unreachable!("modulo four sum must stay in range"),
    }
}

const fn ordered_slot_pair_key(first: CornerSlot, second: CornerSlot) -> usize {
    first.index() * CornerSlot::ALL.len() + second.index()
}

const fn opposite_twist_delta(delta: u8) -> u8 {
    match delta {
        1 => 2,
        2 => 1,
        _ => panic!("corner twist delta must be either 1 or 2"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        solver::{CenterReductionStage, EdgePairingStage, ReductionSolver, Solver, SolverStage},
        Byte, Cube, SolveOptions, XorShift64,
    };

    use super::super::search::CornerMoveTables;

    #[test]
    fn canonical_swap_recipe_matches_the_adjacent_corner_two_cycle_target() {
        let actual = apply_specs(CORNER_SWAP_RECIPE);
        let expected = CornerState {
            permutation: [1, 0, 2, 3, 4, 5, 6, 7],
            orientation: [0; 8],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn canonical_twist_recipe_matches_the_adjacent_corner_twist_target() {
        let actual = apply_specs(CORNER_TWIST_RECIPE);
        let expected = CornerState {
            permutation: [0, 1, 2, 3, 4, 5, 6, 7],
            orientation: [1, 2, 0, 0, 0, 0, 0, 0],
        };

        assert_eq!(actual, expected);
    }

    #[test]
    fn canonical_recipes_match_the_search_oracle_minima() {
        let tables = CornerMoveTables::new();
        let swap_target = CornerState {
            permutation: [1, 0, 2, 3, 4, 5, 6, 7],
            orientation: [0; 8],
        };
        let twist_target = CornerState {
            permutation: [0, 1, 2, 3, 4, 5, 6, 7],
            orientation: [1, 2, 0, 0, 0, 0, 0, 0],
        };
        let optimal_swap = tables
            .solve(swap_target)
            .expect("swap target must be solvable");
        let optimal_twist = tables
            .solve(twist_target)
            .expect("twist target must be solvable");

        assert_eq!(CORNER_SWAP_RECIPE.len(), optimal_swap.len());
        assert_eq!(CORNER_TWIST_RECIPE.len(), optimal_twist.len());
    }

    #[test]
    fn ordered_pair_setup_table_reaches_every_distinct_corner_pair() {
        let table = corner_two_cycle_setup_table();

        for first in CornerSlot::ALL {
            for second in CornerSlot::ALL {
                if first == second {
                    continue;
                }

                let setup = table.setup_for_ordered_pair(first, second);
                let actual = affected_slots_after_sequence(setup);
                assert_eq!(actual, [first, second]);
            }
        }
    }

    #[test]
    fn precomputed_swap_sequences_match_solved_state_transpositions_up_to_twist() {
        let table = corner_two_cycle_setup_table();

        for first in CornerSlot::ALL {
            for second in CornerSlot::ALL {
                if first == second {
                    continue;
                }

                let actual = apply_specs(table.swap_sequence(first, second).iter().copied());
                let mut expected = CornerState {
                    permutation: [0, 1, 2, 3, 4, 5, 6, 7],
                    orientation: [0; 8],
                };
                expected.permutation.swap(first.index(), second.index());

                assert_eq!(
                    actual.permutation, expected.permutation,
                    "swap sequence permutation mismatch for {first:?}->{second:?}"
                );
            }
        }
    }

    #[test]
    fn precomputed_twist_sequences_match_solved_state_twist_targets() {
        let table = corner_two_cycle_setup_table();

        for first in CornerSlot::ALL {
            for second in CornerSlot::ALL {
                if first == second {
                    continue;
                }

                let positive = apply_specs(table.twist_sequence(first, second, 1).iter().copied());
                let mut expected_positive = CornerState {
                    permutation: [0, 1, 2, 3, 4, 5, 6, 7],
                    orientation: [0; 8],
                };
                expected_positive.orientation[first.index()] = 1;
                expected_positive.orientation[second.index()] = 2;
                assert_eq!(
                    positive, expected_positive,
                    "positive twist sequence mismatch for {first:?}->{second:?}"
                );

                let negative = apply_specs(table.twist_sequence(first, second, 2).iter().copied());
                let mut expected_negative = CornerState {
                    permutation: [0, 1, 2, 3, 4, 5, 6, 7],
                    orientation: [0; 8],
                };
                expected_negative.orientation[first.index()] = 2;
                expected_negative.orientation[second.index()] = 1;
                assert_eq!(
                    negative, expected_negative,
                    "negative twist sequence mismatch for {first:?}->{second:?}"
                );
            }
        }
    }

    #[test]
    fn two_cycle_stage_recorded_moves_replay_to_same_full_cube_state() {
        for side_length in 2..=8 {
            for seed in [0xD0E5_C001u64, 0xD0E5_C002u64] {
                let mut cube = Cube::<Byte>::new_solved(side_length);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                cube.scramble(&mut rng);
                let initial = cube.clone();
                let history_before = cube.history().len();
                let history_before_moves = initial.history().as_slice().to_vec();

                let mut stage = CornerTwoCycleReductionStage::default();
                let mut context = SolveContext::new(SolveOptions { record_moves: true });
                <CornerTwoCycleReductionStage as SolverStage<Byte>>::run(
                    &mut stage,
                    &mut cube,
                    &mut context,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "two-cycle corner stage failed for n={side_length}, seed={seed:#x}: {error}\n{}",
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
    fn two_cycle_stage_solves_scrambled_corners_for_sizes_two_to_eight() {
        for side_length in 2..=8 {
            for seed in [0xC02C_51DEu64, 0xC02C_5EEDu64] {
                let mut cube = Cube::<Byte>::new_solved(side_length);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                cube.scramble(&mut rng);

                let mut stage = CornerTwoCycleReductionStage::default();
                let mut context = SolveContext::new(SolveOptions {
                    record_moves: false,
                });
                <CornerTwoCycleReductionStage as SolverStage<Byte>>::run(
                    &mut stage,
                    &mut cube,
                    &mut context,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "two-cycle corner stage failed for n={side_length}, seed={seed:#x}: {error}\n{}",
                        cube.net_string(),
                    )
                });

                assert!(all_corner_facelets_solved(&cube));
            }
        }
    }

    #[test]
    fn full_pipeline_solves_scrambled_cubes_with_two_cycle_corner_stage() {
        for side_length in 1..=8 {
            let mut cube = Cube::<Byte>::new_solved(side_length);
            let mut rng = XorShift64::new(0xC02C_C0DE ^ side_length as u64);
            cube.scramble(&mut rng);

            let mut solver = ReductionSolver::<Byte>::new(SolveOptions {
                record_moves: false,
            })
            .with_stage(CenterReductionStage::western_default())
            .with_stage(CornerTwoCycleReductionStage::default())
            .with_stage(EdgePairingStage::default());

            solver.solve(&mut cube).unwrap_or_else(|error| {
                panic!(
                    "full solver with two-cycle corner stage failed for n={side_length}: {error}\n{}",
                    cube.net_string(),
                )
            });

            assert!(cube.is_solved(), "full solver did not solve n={side_length}");
        }
    }

    fn apply_specs(specs: impl IntoIterator<Item = CornerMoveSpec>) -> CornerState {
        let mut cube = Cube::<Byte>::new_solved(3);
        cube.apply_moves_untracked(specs.into_iter().map(|spec| spec.move_for_side_length(3)));
        read_corner_state(&cube).expect("corner recipe must produce a valid corner state")
    }

    fn affected_slots_after_sequence(specs: &[CornerMoveSpec]) -> [CornerSlot; 2] {
        let mut slots = [CANONICAL_FIRST_SLOT, CANONICAL_SECOND_SLOT];
        for spec in specs {
            let destination = corner_move_destinations()[CORNER_MOVE_SPECS
                .iter()
                .position(|candidate| *candidate == *spec)
                .expect("setup sequence spec must exist in the move list")];
            slots = slots.map(|slot| CornerSlot::ALL[destination[slot.index()] as usize]);
        }
        slots
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
