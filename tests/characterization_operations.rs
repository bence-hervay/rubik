use rubik::solver::MoveSequenceOperation;
use rubik::{
    Byte, Cube, EdgeThreeCycle, FaceCommutator, FaceCommutatorMode, FaceId, Facelet, FaceletArray,
    Move, MoveAngle, MoveStats, SolveContext, SolveOptions,
};

fn patterned_cube<S: FaceletArray>(side_length: usize, seed: usize) -> Cube<S> {
    let mut cube = Cube::<S>::new_solved_with_threads(side_length, 1);

    for face in FaceId::ALL {
        for row in 0..side_length {
            for col in 0..side_length {
                let raw = (seed + face.index() * 17 + row * 7 + col * 11) % Facelet::ALL.len();
                cube.face_mut(face).set(row, col, Facelet::ALL[raw]);
            }
        }
    }

    cube
}

fn assert_cubes_match<A: FaceletArray, B: FaceletArray>(actual: &Cube<A>, expected: &Cube<B>) {
    assert_eq!(actual.side_len(), expected.side_len());

    for face in FaceId::ALL {
        assert_eq!(
            actual.face(face).rotation(),
            expected.face(face).rotation(),
            "face rotation mismatch on {face}",
        );

        for row in 0..actual.side_len() {
            for col in 0..actual.side_len() {
                assert_eq!(
                    actual.face(face).get(row, col),
                    expected.face(face).get(row, col),
                    "facelet mismatch on {face} at ({row}, {col})",
                );
            }
        }
    }
}

fn move_stats_for(side_length: usize, moves: &[Move]) -> MoveStats {
    let mut stats = MoveStats::default();
    stats.record_all(moves.iter().copied(), side_length);
    stats
}

#[test]
fn recorded_and_unrecorded_move_sequence_operations_have_the_same_cube_effect() {
    let side_length = 5;
    let moves = [
        Move::new(rubik::Axis::X, 0, MoveAngle::Positive),
        Move::new(rubik::Axis::Y, 2, MoveAngle::Negative),
        Move::new(rubik::Axis::Z, 1, MoveAngle::Double),
        Move::new(rubik::Axis::X, 4, MoveAngle::Positive),
    ];
    let operation = MoveSequenceOperation::new(side_length, &moves);
    let initial = patterned_cube::<Byte>(side_length, 13);

    let mut expected = initial.clone();
    expected.apply_moves_untracked_with_threads(moves, 1);

    let mut recorded_cube = initial.clone();
    let mut recorded_context = SolveContext::new(SolveOptions {
        thread_count: 1,
        record_moves: true,
    });
    recorded_context.apply_operation(&mut recorded_cube, &operation);

    let mut unrecorded_cube = initial;
    let mut unrecorded_context = SolveContext::new(SolveOptions {
        thread_count: 1,
        record_moves: false,
    });
    unrecorded_context.apply_operation(&mut unrecorded_cube, &operation);

    let expected_stats = move_stats_for(side_length, &moves);

    assert_cubes_match(&recorded_cube, &expected);
    assert_cubes_match(&unrecorded_cube, &expected);
    assert_eq!(recorded_context.moves(), &moves);
    assert!(unrecorded_context.moves().is_empty());
    assert_eq!(recorded_context.move_stats(), expected_stats);
    assert_eq!(unrecorded_context.move_stats(), expected_stats);
    assert!(recorded_cube.history().is_empty());
    assert!(unrecorded_cube.history().is_empty());
}

#[test]
fn recorded_and_unrecorded_face_commutator_plans_have_the_same_cube_effect() {
    let side_length = 7;
    let rows = [1usize, 4];
    let columns = [2usize, 5];
    let commutator = FaceCommutator::new(FaceId::R, FaceId::F, MoveAngle::Negative);
    let probe = Cube::<Byte>::new_solved_with_threads(side_length, 1);

    for mode in [FaceCommutatorMode::Expanded, FaceCommutatorMode::Normalized] {
        let plan = match mode {
            FaceCommutatorMode::Expanded => probe.face_commutator_plan(commutator, &rows, &columns),
            FaceCommutatorMode::Normalized => {
                probe.normalized_face_commutator_plan(commutator, &rows, &columns)
            }
        };
        let literal_moves = plan.literal_moves();
        let initial = patterned_cube::<Byte>(side_length, 29 + mode as usize);

        let mut expected = initial.clone();
        expected.apply_face_commutator_plan_literal_untracked(plan);

        let mut recorded_cube = initial.clone();
        let mut recorded_context = SolveContext::new(SolveOptions {
            thread_count: 1,
            record_moves: true,
        });
        recorded_context.apply_operation(&mut recorded_cube, &plan);

        let mut unrecorded_cube = initial;
        let mut unrecorded_context = SolveContext::new(SolveOptions {
            thread_count: 1,
            record_moves: false,
        });
        unrecorded_context.apply_operation(&mut unrecorded_cube, &plan);

        let expected_stats = move_stats_for(side_length, &literal_moves);

        assert_cubes_match(&recorded_cube, &expected);
        assert_cubes_match(&unrecorded_cube, &expected);
        assert_eq!(recorded_context.moves(), literal_moves.as_slice());
        assert!(unrecorded_context.moves().is_empty());
        assert_eq!(recorded_context.move_stats(), expected_stats);
        assert_eq!(unrecorded_context.move_stats(), expected_stats);
        assert!(recorded_cube.history().is_empty());
        assert!(unrecorded_cube.history().is_empty());
    }
}

#[test]
fn recorded_and_unrecorded_edge_three_cycle_plans_have_the_same_cube_effect() {
    let cases = [
        (5usize, EdgeThreeCycle::front_right_wing(1)),
        (
            5usize,
            EdgeThreeCycle::front_right_middle(rubik::EdgeThreeCycleDirection::Positive),
        ),
    ];

    for (index, (side_length, cycle)) in cases.into_iter().enumerate() {
        let plan =
            Cube::<Byte>::new_solved_with_threads(side_length, 1).edge_three_cycle_plan(cycle);
        let literal_moves = plan.moves().to_vec();
        let initial = patterned_cube::<Byte>(side_length, 41 + index);

        let mut expected = initial.clone();
        expected.apply_edge_three_cycle_plan_literal_untracked(&plan);

        let mut recorded_cube = initial.clone();
        let mut recorded_context = SolveContext::new(SolveOptions {
            thread_count: 1,
            record_moves: true,
        });
        recorded_context.apply_edge_three_cycle_plan(&mut recorded_cube, &plan);

        let mut unrecorded_cube = initial;
        let mut unrecorded_context = SolveContext::new(SolveOptions {
            thread_count: 1,
            record_moves: false,
        });
        unrecorded_context.apply_edge_three_cycle_plan(&mut unrecorded_cube, &plan);

        let expected_stats = move_stats_for(side_length, &literal_moves);

        assert_cubes_match(&recorded_cube, &expected);
        assert_cubes_match(&unrecorded_cube, &expected);
        assert_eq!(recorded_context.moves(), literal_moves.as_slice());
        assert!(unrecorded_context.moves().is_empty());
        assert_eq!(recorded_context.move_stats(), expected_stats);
        assert_eq!(unrecorded_context.move_stats(), expected_stats);
        assert!(recorded_cube.history().is_empty());
        assert!(unrecorded_cube.history().is_empty());
    }
}
