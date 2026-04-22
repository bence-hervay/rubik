use super::{
    commutator::{
        face_commutator_moves, normalized_face_commutator_moves, positions_are_unique,
        sorted_layer_sets_are_disjoint, try_expanded_face_commutator_difference_cycle,
        try_normalized_face_commutator_difference_cycle,
    },
    edge_cycles::{flip_right_edge_moves, move_sequence_updates, unflip_right_edge_moves},
    pieces::{edge_cubie_location, trace_position, FacePosition},
    *,
};
use crate::{Axis, Byte, Byte3, FaceAngle, Nibble, RandomSource, ThreeBit, XorShift64};

fn basic_singmaster_turn(side_length: usize, notation: &str) -> Move {
    let last = side_length - 1;

    match notation {
        "U" => Move::new(Axis::Y, last, MoveAngle::Positive),
        "U'" => Move::new(Axis::Y, last, MoveAngle::Negative),
        "U2" => Move::new(Axis::Y, last, MoveAngle::Double),
        "D" => Move::new(Axis::Y, 0, MoveAngle::Negative),
        "D'" => Move::new(Axis::Y, 0, MoveAngle::Positive),
        "D2" => Move::new(Axis::Y, 0, MoveAngle::Double),
        "R" => Move::new(Axis::X, last, MoveAngle::Positive),
        "R'" => Move::new(Axis::X, last, MoveAngle::Negative),
        "R2" => Move::new(Axis::X, last, MoveAngle::Double),
        "L" => Move::new(Axis::X, 0, MoveAngle::Negative),
        "L'" => Move::new(Axis::X, 0, MoveAngle::Positive),
        "L2" => Move::new(Axis::X, 0, MoveAngle::Double),
        "F" => Move::new(Axis::Z, last, MoveAngle::Positive),
        "F'" => Move::new(Axis::Z, last, MoveAngle::Negative),
        "F2" => Move::new(Axis::Z, last, MoveAngle::Double),
        "B" => Move::new(Axis::Z, 0, MoveAngle::Negative),
        "B'" => Move::new(Axis::Z, 0, MoveAngle::Positive),
        "B2" => Move::new(Axis::Z, 0, MoveAngle::Double),
        _ => panic!("unsupported basic Singmaster turn: {notation}"),
    }
}

fn random_moves(side_length: usize, count: usize, seed: u64) -> Vec<Move> {
    let mut rng = XorShift64::new(seed);
    let mut moves = Vec::with_capacity(count);

    for _ in 0..count {
        let axis = match rng.next_u64() % 3 {
            0 => Axis::X,
            1 => Axis::Y,
            _ => Axis::Z,
        };
        let depth = (rng.next_u64() as usize) % side_length;
        let angle = match rng.next_u64() % 3 {
            0 => MoveAngle::Positive,
            1 => MoveAngle::Double,
            _ => MoveAngle::Negative,
        };
        moves.push(Move::new(axis, depth, angle));
    }

    moves
}

fn patterned_cube<S: FaceletArray>(side_length: usize, seed: usize) -> Cube<S> {
    let mut cube = Cube::<S>::new_solved_with_threads(side_length, 1);

    for face in FaceId::ALL {
        for row in 0..side_length {
            for col in 0..side_length {
                let raw = (face.index() * 17 + row * 7 + col * 11 + seed * 5) % Facelet::ALL.len();
                cube.face_mut(face)
                    .set(row, col, Facelet::from_u8(raw as u8));
            }
        }
    }

    cube
}

fn disjoint_inner_layer_set_pairs(side_length: usize) -> Vec<(Vec<usize>, Vec<usize>)> {
    let layers = (1..side_length - 1).collect::<Vec<_>>();
    let mut pairs = Vec::new();

    for mask in 0..3usize.pow(layers.len() as u32) {
        let mut rows = Vec::new();
        let mut columns = Vec::new();
        let mut remaining = mask;

        for layer in layers.iter().copied() {
            match remaining % 3 {
                1 => rows.push(layer),
                2 => columns.push(layer),
                _ => {}
            }
            remaining /= 3;
        }

        pairs.push((rows, columns));
    }

    pairs
}

fn overlapping_inner_layer_set_pairs(side_length: usize) -> Vec<(Vec<usize>, Vec<usize>)> {
    let layers = (1..side_length - 1).collect::<Vec<_>>();
    let mut pairs = Vec::new();

    for mask in 0..4usize.pow(layers.len() as u32) {
        let mut rows = Vec::new();
        let mut columns = Vec::new();
        let mut remaining = mask;

        for layer in layers.iter().copied() {
            match remaining % 4 {
                1 => rows.push(layer),
                2 => columns.push(layer),
                3 => {
                    rows.push(layer);
                    columns.push(layer);
                }
                _ => {}
            }
            remaining /= 4;
        }

        if !sorted_layer_sets_are_disjoint(&rows, &columns) {
            pairs.push((rows, columns));
        }
    }

    pairs
}

fn sparse_commutator_mapping_matches_expanded(
    side_length: usize,
    destination: FaceId,
    helper: FaceId,
    rows: &[usize],
    columns: &[usize],
    slice_angle: MoveAngle,
) -> bool {
    let expanded =
        face_commutator_moves(side_length, destination, helper, rows, columns, slice_angle);
    let baseline = [super::face_layer_move(
        side_length,
        destination,
        0,
        MoveAngle::Positive,
    )];
    let mut sparse_cycles = Vec::new();

    for row in rows.iter().copied() {
        for column in columns.iter().copied() {
            let Some(cycle) = try_expanded_face_commutator_difference_cycle(
                side_length,
                destination,
                helper,
                row,
                column,
                slice_angle,
            ) else {
                return false;
            };
            sparse_cycles.extend(cycle);
        }
    }

    if !positions_are_unique(sparse_cycles.iter().map(|(from, _)| *from))
        || !positions_are_unique(sparse_cycles.iter().map(|(_, to)| *to))
    {
        return false;
    }

    for face in FaceId::ALL {
        for row in 0..side_length {
            for col in 0..side_length {
                let position = FacePosition { face, row, col };
                let baseline_position = trace_position(side_length, position, baseline);
                let sparse_position = sparse_cycles
                    .iter()
                    .find_map(|(from, to)| (*from == baseline_position).then_some(*to))
                    .unwrap_or(baseline_position);
                let expanded_position =
                    trace_position(side_length, position, expanded.iter().copied());

                if sparse_position != expanded_position {
                    return false;
                }
            }
        }
    }

    true
}

fn sparse_commutator_mapping_matches_normalized(
    side_length: usize,
    destination: FaceId,
    helper: FaceId,
    rows: &[usize],
    columns: &[usize],
    slice_angle: MoveAngle,
) -> bool {
    let expanded = normalized_face_commutator_moves(
        side_length,
        destination,
        helper,
        rows,
        columns,
        slice_angle,
    );
    let mut sparse_cycles = Vec::new();

    for row in rows.iter().copied() {
        for column in columns.iter().copied() {
            let Some(cycle) = try_normalized_face_commutator_difference_cycle(
                side_length,
                destination,
                helper,
                row,
                column,
                slice_angle,
            ) else {
                return false;
            };
            sparse_cycles.extend(cycle);
        }
    }

    if !positions_are_unique(sparse_cycles.iter().map(|(from, _)| *from))
        || !positions_are_unique(sparse_cycles.iter().map(|(_, to)| *to))
    {
        return false;
    }

    for face in FaceId::ALL {
        for row in 0..side_length {
            for col in 0..side_length {
                let position = FacePosition { face, row, col };
                let sparse_position = sparse_cycles
                    .iter()
                    .find_map(|(from, to)| (*from == position).then_some(*to))
                    .unwrap_or(position);
                let expanded_position =
                    trace_position(side_length, position, expanded.iter().copied());

                if sparse_position != expanded_position {
                    return false;
                }
            }
        }
    }

    true
}

fn edge_three_cycle_specs(side_length: usize) -> Vec<EdgeThreeCycle> {
    let mut specs = Vec::new();

    if side_length % 2 == 1 && side_length >= 3 {
        for direction in EdgeThreeCycleDirection::ALL {
            specs.push(EdgeThreeCycle::front_right_middle(direction));
        }
    }

    if side_length >= 4 {
        for row in 1..side_length - 1 {
            if side_length % 2 == 1 && row == side_length / 2 {
                continue;
            }
            specs.push(EdgeThreeCycle::front_right_wing(row));
        }
    }

    specs
}

fn slice_outer_edge_three_cycle_candidate_moves(
    side_length: usize,
    slice_face: FaceId,
    slice_depth_from_face: usize,
    outer_face: FaceId,
    slice_angle: MoveAngle,
) -> Vec<Move> {
    let slice_half = super::face_layer_move(
        side_length,
        slice_face,
        slice_depth_from_face,
        MoveAngle::Double,
    );
    let slice = super::face_layer_move(side_length, slice_face, slice_depth_from_face, slice_angle);
    let outer = super::face_layer_move(side_length, outer_face, 0, MoveAngle::Positive);
    let outer_half = super::face_layer_move(side_length, outer_face, 0, MoveAngle::Double);

    vec![
        slice_half,
        outer,
        slice,
        outer_half,
        slice.inverse(),
        outer,
        slice_half,
    ]
}

fn move_defined_edge_three_cycle_plans(side_length: usize) -> Vec<EdgeThreeCyclePlan> {
    let probe = Cube::<Byte>::new_solved_with_threads(side_length, 1);
    let mut plans = Vec::new();

    if side_length == 3 {
        for slice_face in FaceId::ALL {
            for outer_face in FaceId::ALL {
                if outer_face == slice_face || outer_face == super::opposite_face(slice_face) {
                    continue;
                }

                for slice_angle in [MoveAngle::Positive, MoveAngle::Negative] {
                    let moves = slice_outer_edge_three_cycle_candidate_moves(
                        side_length,
                        slice_face,
                        1,
                        outer_face,
                        slice_angle,
                    );
                    if let Some(plan) = probe.try_edge_three_cycle_plan_from_moves(moves) {
                        plans.push(plan);
                    }
                }
            }
        }
    }

    for cycle in edge_three_cycle_specs(side_length) {
        plans.push(probe.edge_three_cycle_plan(cycle));
    }

    plans
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

fn threaded_moves_match_linear<S: FaceletArray>() {
    let side_length = 65;
    let moves = random_moves(side_length, 12, 0x7A11_DA7A);
    let mut expected = Cube::<S>::new_solved(side_length);

    expected.apply_moves_untracked(moves.iter().copied());

    for thread_count in [1usize, 2, 4, 16] {
        let mut actual = Cube::<S>::new_solved(side_length);
        actual.apply_moves_untracked_with_threads(moves.iter().copied(), thread_count);

        assert_cubes_match(&actual, &expected);
    }
}

fn every_move_inverse_restores<S: FaceletArray>() {
    for n in 1..6 {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for depth in 0..n {
                for angle in MoveAngle::ALL {
                    let mv = Move::new(axis, depth, angle);
                    let mut cube = Cube::<S>::new_solved(n);
                    cube.apply_move_untracked(mv);
                    cube.apply_move_untracked(mv.inverse());
                    assert!(cube.is_solved(), "inverse failed for n={n}, move={mv:?}");
                }
            }
        }
    }
}

fn exact_cube_storage_bytes<S: FaceletArray>(side_length: usize) -> usize {
    side_length
        .checked_mul(side_length)
        .map(S::storage_bytes_for_len)
        .and_then(|bytes_per_face| bytes_per_face.checked_mul(6))
        .expect("test cube storage estimate overflowed usize")
}

#[test]
fn inverse_restores_byte() {
    every_move_inverse_restores::<Byte>();
}

#[test]
fn inverse_restores_byte3() {
    every_move_inverse_restores::<Byte3>();
}

#[test]
fn inverse_restores_nibble() {
    every_move_inverse_restores::<Nibble>();
}

#[test]
fn inverse_restores_three_bit() {
    every_move_inverse_restores::<ThreeBit>();
}

#[test]
fn cube_backends_agree_after_random_moves() {
    let side_length = 6;
    let moves = random_moves(side_length, 1_000, 0xC0DE_CAFE);

    let mut byte = Cube::<Byte>::new_solved(side_length);
    let mut byte3 = Cube::<Byte3>::new_solved(side_length);
    let mut nibble = Cube::<Nibble>::new_solved(side_length);
    let mut three_bit = Cube::<ThreeBit>::new_solved(side_length);

    byte.apply_moves_untracked(moves.iter().copied());
    byte3.apply_moves_untracked(moves.iter().copied());
    nibble.apply_moves_untracked(moves.iter().copied());
    three_bit.apply_moves_untracked(moves.iter().copied());

    assert_cubes_match(&byte3, &byte);
    assert_cubes_match(&nibble, &byte);
    assert_cubes_match(&three_bit, &byte);
}

fn optimized_face_commutators_match_expanded_moves_for(destination: FaceId, helper: FaceId) {
    assert_ne!(helper, destination);
    assert_ne!(helper, super::opposite_face(destination));

    for side_length in 3..=6 {
        for slice_angle in MoveAngle::ALL {
            for (rows, columns) in disjoint_inner_layer_set_pairs(side_length) {
                for seed in 0..2 {
                    let expanded_plan = Cube::<Byte>::new_solved_with_threads(side_length, 1)
                        .face_commutator_plan(
                            FaceCommutator::new(destination, helper, slice_angle),
                            &rows,
                            &columns,
                        );
                    let mut expected = patterned_cube::<Byte>(side_length, seed);
                    expected.apply_face_commutator_plan_literal_untracked(expanded_plan);

                    let mut reference = patterned_cube::<Byte>(side_length, seed);
                    reference.apply_face_commutator_untracked_reference(
                        destination,
                        helper,
                        &rows,
                        &columns,
                        slice_angle,
                    );
                    assert_cubes_match(&reference, &expected);

                    let mut actual = patterned_cube::<Byte>(side_length, seed);
                    actual.apply_face_commutator_plan_untracked(expanded_plan);

                    assert_cubes_match(&actual, &expected);

                    let normalized_plan = Cube::<Byte>::new_solved_with_threads(side_length, 1)
                        .normalized_face_commutator_plan(
                            FaceCommutator::new(destination, helper, slice_angle),
                            &rows,
                            &columns,
                        );
                    let mut normalized_expected = patterned_cube::<Byte>(side_length, seed);
                    normalized_expected
                        .apply_face_commutator_plan_literal_untracked(normalized_plan);

                    let mut normalized_actual = patterned_cube::<Byte>(side_length, seed);
                    normalized_actual.apply_face_commutator_plan_untracked(normalized_plan);

                    assert_cubes_match(&normalized_actual, &normalized_expected);
                }
            }
        }
    }
}

macro_rules! optimized_face_commutator_pair_tests {
    ($($name:ident: $destination:ident => $helper:ident,)+) => {
        $(
            #[test]
            fn $name() {
                optimized_face_commutators_match_expanded_moves_for(
                    FaceId::$destination,
                    FaceId::$helper,
                );
            }
        )+
    };
}

optimized_face_commutator_pair_tests! {
    optimized_face_commutators_match_expanded_moves_u_r: U => R,
    optimized_face_commutators_match_expanded_moves_u_l: U => L,
    optimized_face_commutators_match_expanded_moves_u_f: U => F,
    optimized_face_commutators_match_expanded_moves_u_b: U => B,
    optimized_face_commutators_match_expanded_moves_d_r: D => R,
    optimized_face_commutators_match_expanded_moves_d_l: D => L,
    optimized_face_commutators_match_expanded_moves_d_f: D => F,
    optimized_face_commutators_match_expanded_moves_d_b: D => B,
    optimized_face_commutators_match_expanded_moves_r_u: R => U,
    optimized_face_commutators_match_expanded_moves_r_d: R => D,
    optimized_face_commutators_match_expanded_moves_r_f: R => F,
    optimized_face_commutators_match_expanded_moves_r_b: R => B,
    optimized_face_commutators_match_expanded_moves_l_u: L => U,
    optimized_face_commutators_match_expanded_moves_l_d: L => D,
    optimized_face_commutators_match_expanded_moves_l_f: L => F,
    optimized_face_commutators_match_expanded_moves_l_b: L => B,
    optimized_face_commutators_match_expanded_moves_f_u: F => U,
    optimized_face_commutators_match_expanded_moves_f_d: F => D,
    optimized_face_commutators_match_expanded_moves_f_r: F => R,
    optimized_face_commutators_match_expanded_moves_f_l: F => L,
    optimized_face_commutators_match_expanded_moves_b_u: B => U,
    optimized_face_commutators_match_expanded_moves_b_d: B => D,
    optimized_face_commutators_match_expanded_moves_b_r: B => R,
    optimized_face_commutators_match_expanded_moves_b_l: B => L,
}

#[test]
fn edge_three_cycles_match_expanded_moves_exhaustively() {
    for side_length in 3..=6 {
        let plans = move_defined_edge_three_cycle_plans(side_length);
        assert!(
            !plans.is_empty(),
            "expected edge three-cycle plans for n={side_length}"
        );

        for plan in plans {
            let mut expected = patterned_cube::<Byte>(side_length, 17);
            expected.apply_edge_three_cycle_plan_literal_untracked(&plan);

            let mut actual = patterned_cube::<Byte>(side_length, 17);
            assert_eq!(plan.updates().len(), 6);
            assert_eq!(plan.cubies().len(), 3);
            actual.apply_edge_three_cycle_plan_untracked(&plan);

            assert_cubes_match(&actual, &expected);
        }
    }
}

#[test]
fn front_right_middle_edge_three_cycles_match_expanded_moves_for_larger_odd_sizes() {
    for side_length in [7usize, 9] {
        let probe = Cube::<Byte>::new_solved_with_threads(side_length, 1);

        for direction in EdgeThreeCycleDirection::ALL {
            let cycle = EdgeThreeCycle::front_right_middle(direction);
            let plan = probe.edge_three_cycle_plan(cycle);

            let mut expected = patterned_cube::<Byte>(side_length, 31);
            expected.apply_edge_three_cycle_plan_literal_untracked(&plan);

            let mut actual = patterned_cube::<Byte>(side_length, 31);
            actual.apply_edge_three_cycle_plan_untracked(&plan);

            assert_cubes_match(&actual, &expected);
        }
    }
}

#[test]
fn edge_three_cycle_direct_updates_only_declared_edge_cubies() {
    for side_length in 3..=6 {
        for plan in move_defined_edge_three_cycle_plans(side_length) {
            let before = patterned_cube::<Byte>(side_length, 23);
            let mut affected = std::collections::HashSet::new();

            for update in plan.updates() {
                for location in [update.from, update.to] {
                    assert!(
                        edge_cubie_location(side_length, location).is_some(),
                        "edge three-cycle touched a non-edge location: {location:?}"
                    );
                    affected.insert(location);
                }
            }

            assert_eq!(affected.len(), 6);

            let mut after = before.clone();
            after.apply_edge_three_cycle_plan_untracked(&plan);

            for face in FaceId::ALL {
                assert_eq!(
                    after.face(face).rotation(),
                    before.face(face).rotation(),
                    "edge three-cycle direct apply changed face rotation metadata"
                );

                for row in 0..side_length {
                    for col in 0..side_length {
                        let location = FaceletLocation { face, row, col };
                        if !affected.contains(&location) {
                            assert_eq!(
                                after.face(face).get(row, col),
                                before.face(face).get(row, col),
                                "edge three-cycle direct apply changed undeclared location {location:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn edge_three_cycles_work_for_all_storage_backends() {
    for side_length in [5usize, 6] {
        let probe = Cube::<Byte>::new_solved_with_threads(side_length, 1);

        for cycle in edge_three_cycle_specs(side_length) {
            let plan = probe.edge_three_cycle_plan(cycle);
            let mut byte = patterned_cube::<Byte>(side_length, 29);
            let mut byte3 = patterned_cube::<Byte3>(side_length, 29);
            let mut nibble = patterned_cube::<Nibble>(side_length, 29);
            let mut three_bit = patterned_cube::<ThreeBit>(side_length, 29);

            byte.apply_edge_three_cycle_plan_untracked(&plan);
            byte3.apply_edge_three_cycle_plan_untracked(&plan);
            nibble.apply_edge_three_cycle_plan_untracked(&plan);
            three_bit.apply_edge_three_cycle_plan_untracked(&plan);

            assert_cubes_match(&byte3, &byte);
            assert_cubes_match(&nibble, &byte);
            assert_cubes_match(&three_bit, &byte);
        }
    }
}

#[test]
#[should_panic(expected = "edge three-cycle row must be an inner layer")]
fn edge_three_cycle_rejects_outer_row() {
    let cube = Cube::<Byte>::new_solved_with_threads(4, 1);
    let cycle = EdgeThreeCycle::front_right_wing(0);
    cube.edge_three_cycle_plan(cycle);
}

#[test]
#[should_panic(
    expected = "front-right wing edge three-cycle row cannot be the middle layer on odd cubes"
)]
fn edge_three_cycle_rejects_odd_middle_row() {
    let cube = Cube::<Byte>::new_solved_with_threads(5, 1);
    let cycle = EdgeThreeCycle::front_right_wing(2);
    cube.edge_three_cycle_plan(cycle);
}

#[test]
#[should_panic(expected = "front-right middle edge three-cycles require odd side length")]
fn edge_three_cycle_rejects_even_middle_cycle() {
    let cube = Cube::<Byte>::new_solved_with_threads(6, 1);
    let cycle = EdgeThreeCycle::front_right_middle(EdgeThreeCycleDirection::Positive);
    cube.edge_three_cycle_plan(cycle);
}

#[test]
fn middle_edge_precheck_style_sequence_only_changes_edge_locations() {
    let n = 5;
    let middle = n / 2;
    let mut moves = Vec::new();
    moves.push(face_layer_move(n, FaceId::D, middle, MoveAngle::Positive));
    moves.extend(flip_right_edge_moves(n));
    moves.push(face_layer_move(n, FaceId::D, middle, MoveAngle::Negative));
    moves.extend(unflip_right_edge_moves(n));

    let updates = move_sequence_updates(n, &moves).expect("probe sequence must be valid");
    assert!(!updates.is_empty());
    assert!(
        updates
            .iter()
            .all(|update| edge_cubie_location(n, update.from).is_some()
                && edge_cubie_location(n, update.to).is_some()),
        "precheck-style sequence must stay edge-only",
    );
}

#[test]
fn parity_fix_style_sequence_only_changes_edge_locations() {
    let n = 6;
    let row = 1usize;
    let moves = vec![
        face_layer_move(n, FaceId::D, row, MoveAngle::Negative),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Double),
        face_layer_move(n, FaceId::U, row, MoveAngle::Positive),
        face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
        face_layer_move(n, FaceId::U, row, MoveAngle::Negative),
        face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
        face_layer_move(n, FaceId::D, row, MoveAngle::Double),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Double),
        face_layer_move(n, FaceId::D, row, MoveAngle::Positive),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Double),
        face_layer_move(n, FaceId::D, row, MoveAngle::Negative),
        face_layer_move(n, FaceId::R, 0, MoveAngle::Double),
        face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
        face_layer_move(n, FaceId::D, row, MoveAngle::Double),
        face_layer_move(n, FaceId::F, 0, MoveAngle::Double),
    ];

    let updates = move_sequence_updates(n, &moves).expect("probe sequence must be valid");
    assert!(!updates.is_empty());
    assert!(
        updates
            .iter()
            .any(|update| edge_cubie_location(n, update.from).is_none()
                || edge_cubie_location(n, update.to).is_none()),
        "parity-fix sequence is expected to touch non-edge locations and must stay as literal moves",
    );
}

#[test]
fn direct_face_commutators_work_for_all_storage_backends() {
    let side_length = 7;
    let rows = [1usize, 4];
    let columns = [2usize, 3, 5];

    for destination in FaceId::ALL {
        for helper in FaceId::ALL {
            if helper == destination || helper == super::opposite_face(destination) {
                continue;
            }

            for slice_angle in MoveAngle::ALL {
                let commutator = FaceCommutator::new(destination, helper, slice_angle);
                let probe = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                let expanded_plan = probe.face_commutator_plan(commutator, &rows, &columns);
                let mut byte = patterned_cube::<Byte>(side_length, 3);
                let mut byte3 = patterned_cube::<Byte3>(side_length, 3);
                let mut nibble = patterned_cube::<Nibble>(side_length, 3);
                let mut three_bit = patterned_cube::<ThreeBit>(side_length, 3);

                byte.apply_face_commutator_plan_untracked(expanded_plan);
                byte3.apply_face_commutator_plan_untracked(expanded_plan);
                nibble.apply_face_commutator_plan_untracked(expanded_plan);
                three_bit.apply_face_commutator_plan_untracked(expanded_plan);

                assert_cubes_match(&byte3, &byte);
                assert_cubes_match(&nibble, &byte);
                assert_cubes_match(&three_bit, &byte);

                let normalized_plan =
                    probe.normalized_face_commutator_plan(commutator, &rows, &columns);
                let mut byte = patterned_cube::<Byte>(side_length, 5);
                let mut byte3 = patterned_cube::<Byte3>(side_length, 5);
                let mut nibble = patterned_cube::<Nibble>(side_length, 5);
                let mut three_bit = patterned_cube::<ThreeBit>(side_length, 5);

                byte.apply_face_commutator_plan_untracked(normalized_plan);
                byte3.apply_face_commutator_plan_untracked(normalized_plan);
                nibble.apply_face_commutator_plan_untracked(normalized_plan);
                three_bit.apply_face_commutator_plan_untracked(normalized_plan);

                assert_cubes_match(&byte3, &byte);
                assert_cubes_match(&nibble, &byte);
                assert_cubes_match(&three_bit, &byte);
            }
        }
    }
}

#[test]
fn normalized_face_commutator_only_changes_declared_center_positions() {
    let side_length = 9;
    let rows = [1usize, 4, 7];
    let columns = [2usize, 3, 5, 6];

    for destination in FaceId::ALL {
        for helper in FaceId::ALL {
            if helper == destination || helper == super::opposite_face(destination) {
                continue;
            }

            for slice_angle in MoveAngle::ALL {
                let commutator = FaceCommutator::new(destination, helper, slice_angle);
                let before = patterned_cube::<Byte>(side_length, 9);
                let mut after = before.clone();
                let mut affected = std::collections::HashSet::new();

                for row in rows {
                    for column in columns {
                        let updates = before
                            .normalized_face_commutator_sparse_updates(commutator, row, column);
                        for update in updates {
                            for location in [update.from, update.to] {
                                assert!(
                                    location.row > 0
                                        && location.row + 1 < side_length
                                        && location.col > 0
                                        && location.col + 1 < side_length,
                                    "normalized commutator touched a non-center location: {location:?}"
                                );
                                affected.insert(location);
                            }
                        }
                    }
                }

                assert_eq!(affected.len(), rows.len() * columns.len() * 3);

                let plan = after.normalized_face_commutator_plan(commutator, &rows, &columns);
                after.apply_face_commutator_plan_untracked(plan);

                for face in FaceId::ALL {
                    assert_eq!(
                        after.face(face).rotation(),
                        before.face(face).rotation(),
                        "normalized commutator changed face rotation metadata"
                    );

                    for row in 0..side_length {
                        for col in 0..side_length {
                            let location = FaceletLocation { face, row, col };
                            if !affected.contains(&location) {
                                assert_eq!(
                                    after.face(face).get(row, col),
                                    before.face(face).get(row, col),
                                    "normalized commutator changed undeclared location {location:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn overlapping_row_and_column_sets_cannot_extend_sparse_commutator_family() {
    for side_length in 3..=6 {
        for destination in FaceId::ALL {
            for helper in FaceId::ALL {
                if helper == destination || helper == super::opposite_face(destination) {
                    continue;
                }

                for slice_angle in MoveAngle::ALL {
                    for (rows, columns) in overlapping_inner_layer_set_pairs(side_length) {
                        assert!(
                            !sparse_commutator_mapping_matches_expanded(
                                side_length,
                                destination,
                                helper,
                                &rows,
                                &columns,
                                slice_angle,
                            ),
                            "overlapping row/column sets unexpectedly matched for n={side_length}, destination={destination}, helper={helper}, angle={slice_angle}, rows={rows:?}, columns={columns:?}"
                        );
                        assert!(
                            !sparse_commutator_mapping_matches_normalized(
                                side_length,
                                destination,
                                helper,
                                &rows,
                                &columns,
                                slice_angle,
                            ),
                            "overlapping row/column sets unexpectedly matched normalized commutator for n={side_length}, destination={destination}, helper={helper}, angle={slice_angle}, rows={rows:?}, columns={columns:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
#[should_panic(expected = "destination and helper faces must be perpendicular")]
fn face_commutator_rejects_parallel_helper_face() {
    let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);
    cube.apply_face_commutator_untracked(FaceId::U, FaceId::D, &[1], &[2], MoveAngle::Positive);
}

#[test]
#[should_panic(expected = "destination and helper faces must be perpendicular")]
fn normalized_face_commutator_rejects_parallel_helper_face() {
    let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);
    cube.apply_normalized_face_commutator_untracked(
        FaceId::U,
        FaceId::D,
        &[1],
        &[2],
        MoveAngle::Positive,
    );
}

#[test]
#[should_panic(expected = "commutator row and column layer sets must be disjoint")]
fn face_commutator_rejects_same_row_and_column_layer() {
    let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);
    cube.apply_face_commutator_untracked(FaceId::U, FaceId::R, &[1], &[1], MoveAngle::Positive);
}

#[test]
#[should_panic(expected = "commutator row and column layer sets must be disjoint")]
fn normalized_face_commutator_rejects_same_row_and_column_layer() {
    let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);
    cube.apply_normalized_face_commutator_untracked(
        FaceId::U,
        FaceId::R,
        &[1],
        &[1],
        MoveAngle::Positive,
    );
}

#[test]
fn sorted_layer_set_disjointness_is_linear_merge_compatible() {
    assert!(sorted_layer_sets_are_disjoint(&[], &[]));
    assert!(sorted_layer_sets_are_disjoint(&[1, 3, 5], &[2, 4, 6]));
    assert!(sorted_layer_sets_are_disjoint(&[1, 2, 3], &[]));
    assert!(sorted_layer_sets_are_disjoint(&[], &[1, 2, 3]));
    assert!(!sorted_layer_sets_are_disjoint(&[1, 3, 5], &[0, 3, 6]));
    assert!(!sorted_layer_sets_are_disjoint(&[1, 2, 3], &[3, 4, 5]));
}

#[test]
fn face_commutator_plan_checker_rejects_overlapping_layers() {
    let commutator = FaceCommutator::new(FaceId::U, FaceId::R, MoveAngle::Positive);
    let error = FaceCommutatorPlan::try_new(
        5,
        commutator,
        FaceCommutatorMode::Normalized,
        &[1, 2],
        &[2, 3],
    )
    .unwrap_err();

    assert_eq!(
        error,
        FaceCommutatorValidationError::RowAndColumnSetsMustBeDisjoint
    );
}

#[test]
fn face_commutator_plan_checker_accepts_valid_configuration() {
    let commutator = FaceCommutator::new(FaceId::F, FaceId::U, MoveAngle::Negative);
    let plan = FaceCommutatorPlan::try_new(
        7,
        commutator,
        FaceCommutatorMode::Expanded,
        &[1, 3],
        &[2, 4, 5],
    )
    .expect("valid face commutator plan must pass validation");

    assert_eq!(plan.try_validate(), Ok(()));
    assert_eq!(plan.layers().rows(), &[1, 3]);
    assert_eq!(plan.layers().columns(), &[2, 4, 5]);
}

#[test]
fn edge_three_cycle_checker_rejects_invalid_cycles() {
    assert_eq!(
        EdgeThreeCycle::front_right_wing(0).try_validate(6),
        Err(EdgeThreeCycleValidationError::RowMustBeInnerLayer)
    );
    assert_eq!(
        EdgeThreeCycle::front_right_middle(EdgeThreeCycleDirection::Positive).try_validate(6),
        Err(EdgeThreeCycleValidationError::MiddleCycleRequiresOddSideLengthAtLeastThree)
    );
}

#[test]
fn threaded_byte_moves_match_linear() {
    threaded_moves_match_linear::<Byte>();
}

#[test]
fn threaded_nibble_moves_match_linear() {
    threaded_moves_match_linear::<Nibble>();
}

#[test]
fn threaded_three_bit_moves_match_linear() {
    threaded_moves_match_linear::<ThreeBit>();
}

#[test]
fn threaded_byte3_moves_match_linear() {
    threaded_moves_match_linear::<Byte3>();
}

#[test]
fn cube_storage_estimates_are_exact() {
    for side_length in [1usize, 2, 3, 4, 5, 8, 9, 10, 17] {
        assert_eq!(
            Cube::<Byte>::new_solved(side_length).estimated_storage_bytes(),
            exact_cube_storage_bytes::<Byte>(side_length)
        );
        assert_eq!(
            Cube::<Byte3>::new_solved(side_length).estimated_storage_bytes(),
            exact_cube_storage_bytes::<Byte3>(side_length)
        );
        assert_eq!(
            Cube::<Nibble>::new_solved(side_length).estimated_storage_bytes(),
            exact_cube_storage_bytes::<Nibble>(side_length)
        );
        assert_eq!(
            Cube::<ThreeBit>::new_solved(side_length).estimated_storage_bytes(),
            exact_cube_storage_bytes::<ThreeBit>(side_length)
        );
    }
}

#[test]
fn four_quarter_turns_restore() {
    for n in 1..6 {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for depth in 0..n {
                let mv = Move::new(axis, depth, MoveAngle::Positive);
                let mut cube = Cube::<Byte>::new_solved(n);
                for _ in 0..4 {
                    cube.apply_move_untracked(mv);
                }
                assert!(cube.is_solved(), "four turns failed for n={n}, move={mv:?}");
            }
        }
    }
}

#[test]
fn tracked_moves_enter_history() {
    let mut cube = Cube::<Byte>::new_solved(3);
    cube.apply_move(Move::new(Axis::Z, 2, MoveAngle::Positive));
    assert_eq!(cube.history().len(), 1);
}

#[test]
fn solved_cubes_start_reachable() {
    let cube = Cube::<Byte>::new_solved(4);

    assert_eq!(cube.reachability(), CubeReachability::Reachable);
    assert!(cube.is_reachable());
}

#[test]
fn arbitrary_facelet_constructor_preserves_explicit_reachability() {
    let cube = Cube::<Byte>::from_facelet_fn_with_threads(
        3,
        CubeReachability::Unverified,
        1,
        |face, row, col| Facelet::ALL[(face.index() + row + col) % Facelet::ALL.len()],
    );

    assert_eq!(cube.reachability(), CubeReachability::Unverified);
    assert!(!cube.is_reachable());
    assert_eq!(cube.face(FaceId::U).get(0, 0), Facelet::White);
    assert_eq!(cube.face(FaceId::U).get(0, 1), Facelet::Yellow);
    assert_eq!(cube.face(FaceId::F).get(2, 1), Facelet::Yellow);
    assert!(cube.history().is_empty());
}

#[test]
fn direct_face_edits_mark_reachability_unverified() {
    let mut cube = Cube::<Byte>::new_solved(3);

    cube.face_mut(FaceId::U).set(0, 0, Facelet::Red);

    assert_eq!(cube.reachability(), CubeReachability::Unverified);
    assert!(!cube.is_reachable());

    cube.set_reachability(CubeReachability::Reachable);
    assert_eq!(cube.reachability(), CubeReachability::Reachable);
}

#[test]
fn move_and_optimized_update_paths_preserve_reachability() {
    let mut cube = Cube::<Byte>::new_solved_with_threads(4, 1);

    cube.apply_move_untracked(Move::new(Axis::X, 3, MoveAngle::Positive));
    assert_eq!(cube.reachability(), CubeReachability::Reachable);

    let commutator = FaceCommutator::new(FaceId::U, FaceId::R, MoveAngle::Positive);
    let plan = cube.normalized_face_commutator_plan(commutator, &[1], &[2]);
    cube.apply_face_commutator_plan_untracked(plan);
    assert_eq!(cube.reachability(), CubeReachability::Reachable);

    let edge_plan = EdgeThreeCyclePlan::from_cycle(4, EdgeThreeCycle::front_right_wing(1));
    cube.apply_edge_three_cycle_plan_untracked(&edge_plan);
    assert_eq!(cube.reachability(), CubeReachability::Reachable);
}

#[test]
fn random_move_stays_within_cube_bounds() {
    let side_length = 11;
    let cube = Cube::<Byte>::new_solved(side_length);
    let mut rng = XorShift64::new(0x5C4A_4B1E);

    for _ in 0..1_000 {
        let mv = cube.random_move(&mut rng);
        assert!(mv.depth < side_length, "random move depth out of bounds");
    }
}

#[test]
fn scramble_applies_six_rounds_of_random_moves_and_outer_face_turns() {
    let side_length = 5;
    let seed = 0x5C4A_2B1E;

    let mut actual = Cube::<Byte>::new_solved(side_length);
    let mut actual_rng = XorShift64::new(seed);
    actual.scramble(&mut actual_rng);

    let mut expected = Cube::<Byte>::new_solved(side_length);
    let mut expected_rng = XorShift64::new(seed);
    for _ in 0..DEFAULT_SCRAMBLE_ROUNDS {
        expected.scramble_random_moves(&mut expected_rng, side_length);

        for face in FaceId::ALL {
            let mv = expected.random_outer_face_move(face, &mut expected_rng);
            expected.apply_move(mv);
        }
    }

    assert_eq!(
        actual.history().len(),
        DEFAULT_SCRAMBLE_ROUNDS * (side_length + FaceId::ALL.len())
    );
    assert_cubes_match(&actual, &expected);
    assert_eq!(actual.history().as_slice(), expected.history().as_slice());
}

#[test]
fn basic_singmaster_turns_match_our_move_notation() {
    let side_length = 5;
    let last = side_length - 1;

    let cases = [
        ("U", Axis::Y, last, MoveAngle::Positive),
        ("U'", Axis::Y, last, MoveAngle::Negative),
        ("U2", Axis::Y, last, MoveAngle::Double),
        ("D", Axis::Y, 0, MoveAngle::Negative),
        ("D'", Axis::Y, 0, MoveAngle::Positive),
        ("D2", Axis::Y, 0, MoveAngle::Double),
        ("R", Axis::X, last, MoveAngle::Positive),
        ("R'", Axis::X, last, MoveAngle::Negative),
        ("R2", Axis::X, last, MoveAngle::Double),
        ("L", Axis::X, 0, MoveAngle::Negative),
        ("L'", Axis::X, 0, MoveAngle::Positive),
        ("L2", Axis::X, 0, MoveAngle::Double),
        ("F", Axis::Z, last, MoveAngle::Positive),
        ("F'", Axis::Z, last, MoveAngle::Negative),
        ("F2", Axis::Z, last, MoveAngle::Double),
        ("B", Axis::Z, 0, MoveAngle::Negative),
        ("B'", Axis::Z, 0, MoveAngle::Positive),
        ("B2", Axis::Z, 0, MoveAngle::Double),
    ];

    for (notation, axis, depth, angle) in cases {
        assert_eq!(
            basic_singmaster_turn(side_length, notation),
            Move::new(axis, depth, angle),
            "unexpected move notation for {notation}"
        );
    }
}

#[test]
fn basic_singmaster_prime_and_double_turns_match_inverse_rules() {
    let side_length = 5;
    let cases = [
        ("U", "U'", "U2"),
        ("D", "D'", "D2"),
        ("R", "R'", "R2"),
        ("L", "L'", "L2"),
        ("F", "F'", "F2"),
        ("B", "B'", "B2"),
    ];

    for (turn, prime, double) in cases {
        let turn_move = basic_singmaster_turn(side_length, turn);
        let prime_move = basic_singmaster_turn(side_length, prime);
        let double_move = basic_singmaster_turn(side_length, double);

        assert_eq!(
            prime_move,
            turn_move.inverse(),
            "{prime} should invert {turn}"
        );
        assert_eq!(
            double_move,
            double_move.inverse(),
            "{double} should be self-inverse"
        );

        let mut cube = Cube::<Byte>::new_solved(side_length);
        cube.apply_move_untracked(turn_move);
        cube.apply_move_untracked(prime_move);
        assert!(
            cube.is_solved(),
            "{turn} followed by {prime} should restore"
        );

        let mut cube = Cube::<Byte>::new_solved(side_length);
        cube.apply_move_untracked(double_move);
        cube.apply_move_untracked(double_move);
        assert!(cube.is_solved(), "{double} twice should restore");
    }
}

#[test]
fn outer_face_rotation_matches_axis_move_direction() {
    let mut cube = Cube::<Byte>::new_solved(3);

    cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
    assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(1));

    cube.apply_move_untracked(Move::new(Axis::Z, 0, MoveAngle::Positive));
    assert_eq!(cube.face(FaceId::B).rotation(), FaceAngle::new(3));

    cube.apply_move_untracked(Move::new(Axis::X, 2, MoveAngle::Negative));
    assert_eq!(cube.face(FaceId::R).rotation(), FaceAngle::new(3));

    cube.apply_move_untracked(Move::new(Axis::X, 0, MoveAngle::Double));
    assert_eq!(cube.face(FaceId::L).rotation(), FaceAngle::new(2));
}

#[test]
fn face_rotation_accumulates_angles_modulo_four() {
    let mut cube = Cube::<Byte>::new_solved(3);

    assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(0));

    cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
    assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(1));

    cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
    assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(2));

    cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
    assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(3));

    cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
    assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(0));
}

#[test]
fn net_uses_traditional_geometry() {
    let cube = Cube::<Byte>::new_solved(2);

    assert_eq!(
        cube.net_string(),
        concat!(
            "Cube(n=2, history=0, storage~24 bytes)\n",
            "      W W\n",
            "      W W\n",
            "\n",
            "O O   G G   R R   B B\n",
            "O O   G G   R R   B B\n",
            "\n",
            "      Y Y\n",
            "      Y Y\n",
        )
    );
}

#[test]
fn net_keeps_unfolded_face_orientations() {
    let mut cube = Cube::<Byte>::new_solved(3);

    for row in 0..3 {
        for col in 0..3 {
            cube.face_mut(FaceId::U)
                .set(row, col, Facelet::from_u8(row as u8));
            cube.face_mut(FaceId::D)
                .set(row, col, Facelet::from_u8((2 - row) as u8));
            cube.face_mut(FaceId::F)
                .set(row, col, Facelet::from_u8(col as u8));
            cube.face_mut(FaceId::B)
                .set(row, col, Facelet::from_u8((2 - col) as u8));
            cube.face_mut(FaceId::L)
                .set(row, col, Facelet::from_u8((row + col) as u8));
            cube.face_mut(FaceId::R)
                .set(row, col, Facelet::from_u8((row + 2 - col) as u8));
        }
    }

    assert_eq!(
        cube.net_string(),
        concat!(
            "Cube(n=3, history=0, storage~54 bytes)\n",
            "        W W W\n",
            "        Y Y Y\n",
            "        R R R\n",
            "\n",
            "W Y R   W Y R   R Y W   R Y W\n",
            "Y R O   W Y R   O R Y   R Y W\n",
            "R O G   W Y R   G O R   R Y W\n",
            "\n",
            "        R R R\n",
            "        Y Y Y\n",
            "        W W W\n",
        )
    );
}

#[test]
fn net_prints_full_small_faces() {
    let cube = Cube::<Byte>::new_solved(4);
    let net = cube.net_string();

    assert!(!net.contains("..."));
    assert!(net.contains("W W W W"));
    assert!(net.contains("O O O O   G G G G   R R R R   B B B B"));
}

#[test]
fn net_prints_full_large_faces_without_ellipsis_markers() {
    let cube = Cube::<Byte>::new_solved(8);
    let net = cube.net_string();

    assert!(!net.contains("..."));
    assert!(!net.contains("-"));
    assert!(net.contains("                  W W W W W W W W\n"));
    assert!(net.contains("O O O O O O O O   G G G G G G G G   R R R R R R R R   B B B B B B B B\n"));
    assert!(net.contains("                  Y Y Y Y Y Y Y Y\n"));
}

#[test]
fn net_limits_large_faces_to_outer_four_layers_with_separator() {
    let mut cube = Cube::<Byte>::new_solved(10);

    cube.face_mut(FaceId::U).set(0, 0, Facelet::Red);
    cube.face_mut(FaceId::U).set(0, 3, Facelet::Green);
    cube.face_mut(FaceId::U).set(0, 4, Facelet::Orange);
    cube.face_mut(FaceId::U).set(0, 5, Facelet::Yellow);
    cube.face_mut(FaceId::U).set(0, 6, Facelet::Blue);
    cube.face_mut(FaceId::U).set(0, 9, Facelet::Red);

    let net = cube.net_string();

    assert!(!net.contains("..."));
    assert!(net.contains("                    R W W G - B W W R\n"));
    assert!(net.contains("                    - - - - - - - - -\n"));
    assert!(net.contains(
        "O O O O - O O O O   G G G G - G G G G   R R R R - R R R R   B B B B - B B B B\n"
    ));
    assert!(net.contains(
        "- - - - - - - - -   - - - - - - - - -   - - - - - - - - -   - - - - - - - - -\n"
    ));
}
