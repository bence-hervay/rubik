use rubik::{
    Axis, Byte, CenterReductionStage, CornerReductionStage, CornerSearchStage, Cube,
    EdgePairingStage, ExecutionMode, FaceId, FaceletArray, Move, MoveAngle, SolveContext,
    SolveOptions, SolverStage, XorShift64,
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

fn positions_match_solved<S, F>(cube: &Cube<S>, include: F) -> bool
where
    S: FaceletArray,
    F: Fn(usize, usize, usize) -> bool,
{
    let solved = Cube::<Byte>::new_solved(cube.side_len());
    let last = cube.side_len().saturating_sub(1);

    for face in FaceId::ALL {
        for row in 0..cube.side_len() {
            for col in 0..cube.side_len() {
                if include(row, col, last)
                    && cube.face(face).get(row, col) != solved.face(face).get(row, col)
                {
                    return false;
                }
            }
        }
    }

    true
}

fn centers_are_solved<S: FaceletArray>(cube: &Cube<S>) -> bool {
    positions_match_solved(cube, |row, col, last| {
        row != 0 && row != last && col != 0 && col != last
    })
}

fn corners_are_solved<S: FaceletArray>(cube: &Cube<S>) -> bool {
    positions_match_solved(cube, |row, col, last| {
        (row == 0 || row == last) && (col == 0 || col == last)
    })
}

fn edges_are_solved<S: FaceletArray>(cube: &Cube<S>) -> bool {
    positions_match_solved(cube, |row, col, last| {
        (row == 0 || row == last) ^ (col == 0 || col == last)
    })
}

fn run_stage<T>(mut stage: T, cube: &mut Cube<Byte>, mode: ExecutionMode) -> SolveContext
where
    T: SolverStage<Byte>,
{
    assert!(
        stage.execution_mode_support().supports(mode),
        "{} must support {mode:?}",
        stage.name(),
    );
    assert!(
        stage.is_applicable_to_side_length(cube.side_len()),
        "{} must support n={}",
        stage.name(),
        cube.side_len(),
    );

    let mut context = SolveContext::new(SolveOptions::new(mode));
    stage.run(cube, &mut context).unwrap_or_else(|error| {
        panic!(
            "{} failed for n={} in {mode:?}: {error}\n{}",
            stage.name(),
            cube.side_len(),
            cube.net_string(),
        )
    });
    context
}

fn assert_stage_postcondition<T, B, F>(
    build_stage: B,
    side_length: usize,
    seed: u64,
    move_count: usize,
    postcondition: F,
) where
    T: SolverStage<Byte>,
    B: Fn() -> T,
    F: Fn(&Cube<Byte>) -> bool,
{
    let mut cube = scrambled_cube(side_length, seed, move_count);
    run_stage(build_stage(), &mut cube, ExecutionMode::Standard);

    assert!(
        postcondition(&cube),
        "{} did not meet its postcondition for n={side_length}, seed={seed:#x}\n{}",
        build_stage().name(),
        cube.net_string(),
    );
}

fn assert_stage_modes_match<T, B, F>(
    build_stage: B,
    side_length: usize,
    seed: u64,
    move_count: usize,
    postcondition: F,
) where
    T: SolverStage<Byte>,
    B: Fn() -> T,
    F: Fn(&Cube<Byte>) -> bool,
{
    let initial = scrambled_cube(side_length, seed, move_count);
    let mut standard_cube = initial.clone();
    let mut optimized_cube = initial;

    let standard_context = run_stage(build_stage(), &mut standard_cube, ExecutionMode::Standard);
    let optimized_context = run_stage(build_stage(), &mut optimized_cube, ExecutionMode::Optimized);

    assert!(postcondition(&standard_cube));
    assert!(postcondition(&optimized_cube));
    assert_cubes_match(&standard_cube, &optimized_cube);
    assert_eq!(
        standard_context.move_stats(),
        optimized_context.move_stats(),
        "move statistics must match for {}",
        build_stage().name(),
    );
}

#[test]
fn center_reduction_stage_meets_its_postcondition() {
    for (side_length, seed) in [(4usize, 0xC311_0001u64), (5usize, 0xC311_0002u64)] {
        assert_stage_postcondition::<CenterReductionStage, _, _>(
            CenterReductionStage::western_default,
            side_length,
            seed,
            48,
            centers_are_solved,
        );
    }
}

#[test]
fn corner_stages_meet_their_postcondition() {
    for (side_length, seed) in [(2usize, 0xC012_0001u64), (5usize, 0xC012_0002u64)] {
        assert_stage_postcondition::<CornerReductionStage, _, _>(
            CornerReductionStage::default,
            side_length,
            seed,
            40,
            corners_are_solved,
        );
        assert_stage_postcondition::<CornerSearchStage, _, _>(
            CornerSearchStage::default,
            side_length,
            seed ^ 0x55AA,
            40,
            corners_are_solved,
        );
    }
}

#[test]
fn edge_pairing_stage_meets_its_postcondition() {
    for (side_length, seed) in [(4usize, 0xED93_0001u64), (5usize, 0xED93_0002u64)] {
        assert_stage_postcondition::<EdgePairingStage, _, _>(
            EdgePairingStage::default,
            side_length,
            seed,
            48,
            edges_are_solved,
        );
    }
}

#[test]
fn center_reduction_stage_has_matching_standard_and_optimized_effects() {
    assert_stage_modes_match::<CenterReductionStage, _, _>(
        CenterReductionStage::western_default,
        5,
        0xC311_1001,
        48,
        centers_are_solved,
    );
}

#[test]
fn corner_stages_have_matching_standard_and_optimized_effects() {
    assert_stage_modes_match::<CornerReductionStage, _, _>(
        CornerReductionStage::default,
        5,
        0xC012_1001,
        40,
        corners_are_solved,
    );
    assert_stage_modes_match::<CornerSearchStage, _, _>(
        CornerSearchStage::default,
        5,
        0xC012_1002,
        40,
        corners_are_solved,
    );
}

#[test]
fn edge_pairing_stage_has_matching_standard_and_optimized_effects() {
    assert_stage_modes_match::<EdgePairingStage, _, _>(
        EdgePairingStage::default,
        5,
        0xED93_1001,
        48,
        edges_are_solved,
    );
}
