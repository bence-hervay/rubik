use rubik::{
    Axis, Byte, CenterReductionStage, CornerReductionStage, Cube, CubeReachability,
    EdgePairingStage, ExecutionMode, FaceId, FaceletArray, Move, MoveAngle, ReductionSolver,
    SolveOptions, SolvePhase, Solver, XorShift64,
};

fn scrambled_cube(side_length: usize, seed: u64, move_count: usize) -> Cube<Byte> {
    let mut cube = Cube::<Byte>::new_solved(side_length);
    let mut rng = XorShift64::new(seed ^ side_length as u64);
    cube.scramble_random_moves(&mut rng, move_count);

    if cube.is_solved() {
        cube.apply_move_untracked(Move::new(Axis::Z, side_length - 1, MoveAngle::Positive));
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

fn default_solver(execution_mode: ExecutionMode) -> ReductionSolver<Byte> {
    ReductionSolver::<Byte>::new(SolveOptions::new(execution_mode))
        .with_stage(CenterReductionStage::western_default())
        .with_stage(CornerReductionStage::default())
        .with_stage(EdgePairingStage::default())
}

#[test]
fn recorded_default_pipeline_replays_to_the_same_final_cube_state() {
    for side_length in [4usize, 5] {
        let mut cube = scrambled_cube(side_length, 0xC7A6_1000, 80);
        let initial = cube.clone();
        let history_before = cube.history().len();
        let history_before_moves = initial.history().as_slice().to_vec();
        let mut solver = default_solver(ExecutionMode::Standard);

        let outcome = solver.solve(&mut cube).unwrap_or_else(|error| {
            panic!(
                "recorded default pipeline failed for n={side_length}: {error}\n{}",
                cube.net_string(),
            )
        });

        let mut replay = initial;
        replay.apply_moves_untracked(outcome.moves.iter().copied());

        assert!(cube.is_solved(), "pipeline did not solve n={side_length}");
        assert_cubes_match(&cube, &replay);
        assert_eq!(outcome.reports.len(), 3);
        assert_eq!(
            outcome
                .reports
                .iter()
                .map(|report| report.phase)
                .collect::<Vec<_>>(),
            [SolvePhase::Centers, SolvePhase::Corners, SolvePhase::Edges],
        );
        assert_eq!(
            outcome
                .reports
                .iter()
                .map(|report| report.name)
                .collect::<Vec<_>>(),
            ["center reduction", "corner reduction", "edge pairing"],
        );
        assert_eq!(outcome.reports[0].moves_before, 0);
        assert_eq!(
            outcome.reports.last().map(|report| report.moves_after),
            Some(outcome.moves.len()),
        );
        assert!(
            outcome
                .reports
                .windows(2)
                .all(|window| window[0].moves_after == window[1].moves_before),
            "stage reports must form a continuous cumulative move count",
        );
        assert_eq!(
            outcome
                .reports
                .iter()
                .map(|report| report.moves_added())
                .sum::<usize>(),
            outcome.moves.len(),
        );
        assert_eq!(outcome.move_stats.total, outcome.moves.len());
        assert_eq!(cube.history().len(), history_before);
        assert_eq!(cube.history().as_slice(), history_before_moves.as_slice());
    }
}

#[test]
fn unrecorded_default_pipeline_keeps_reported_move_counts_without_storing_moves() {
    let side_length = 5;
    let mut cube = scrambled_cube(side_length, 0xC7A6_2000, 80);
    let history_before = cube.history().len();
    let mut solver = default_solver(ExecutionMode::Optimized);

    let outcome = solver.solve(&mut cube).unwrap_or_else(|error| {
        panic!(
            "unrecorded default pipeline failed for n={side_length}: {error}\n{}",
            cube.net_string(),
        )
    });

    let total_reported_moves = outcome
        .reports
        .iter()
        .map(|report| report.moves_added())
        .sum::<usize>();

    assert!(cube.is_solved(), "pipeline did not solve n={side_length}");
    assert!(outcome.moves.is_empty());
    assert_eq!(outcome.move_stats.total, total_reported_moves);
    assert_eq!(outcome.reports.len(), 3);
    assert_eq!(
        outcome.reports.last().map(|report| report.moves_after),
        Some(total_reported_moves),
    );
    assert!(total_reported_moves > 0);
    assert_eq!(cube.history().len(), history_before);
}

#[test]
fn standard_and_optimized_default_pipelines_reach_the_same_final_cube_state() {
    for side_length in [4usize, 5] {
        let initial = scrambled_cube(side_length, 0xC7A6_3000, 80);
        let mut standard_cube = initial.clone();
        let mut optimized_cube = initial;

        let mut standard_solver = default_solver(ExecutionMode::Standard);
        let standard_outcome = standard_solver
            .solve(&mut standard_cube)
            .unwrap_or_else(|error| {
                panic!(
                    "standard default pipeline failed for n={side_length}: {error}\n{}",
                    standard_cube.net_string(),
                )
            });

        let mut optimized_solver = default_solver(ExecutionMode::Optimized);
        let optimized_outcome =
            optimized_solver
                .solve(&mut optimized_cube)
                .unwrap_or_else(|error| {
                    panic!(
                        "optimized default pipeline failed for n={side_length}: {error}\n{}",
                        optimized_cube.net_string(),
                    )
                });

        assert_cubes_match(&standard_cube, &optimized_cube);
        assert_eq!(
            standard_outcome.move_stats.total,
            optimized_outcome.move_stats.total
        );
        assert_eq!(standard_cube.reachability(), CubeReachability::Reachable);
        assert_eq!(optimized_cube.reachability(), CubeReachability::Reachable);
    }
}
