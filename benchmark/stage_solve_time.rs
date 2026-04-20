use std::{
    env, fmt,
    hint::black_box,
    str::FromStr,
    time::{Duration, Instant},
};

use rubik::{
    default_thread_count, Axis, Byte, Byte3, CenterCommutatorTable, CenterReductionStage, Cube,
    FaceId, Facelet, FaceletArray, Move, MoveAngle, MoveStats, Nibble, RandomSource, SolveContext,
    SolveOptions, SolverStage, ThreeBit, XorShift64, DEFAULT_SCRAMBLE_ROUNDS,
    GENERATED_CENTER_SCHEDULE,
};

const DEFAULT_SIDE_POWERS: &[usize] = &[10, 11];
const DEFAULT_RANDOM_SEED: u64 = 0x57A6_EBEE_F00D;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StorageKind {
    Byte,
    Nibble,
    ThreeBit,
    Byte3,
}

impl StorageKind {
    const ALL: [Self; 4] = [Self::Byte, Self::Nibble, Self::ThreeBit, Self::Byte3];

    fn as_str(self) -> &'static str {
        match self {
            Self::Byte => "byte",
            Self::Nibble => "nibble",
            Self::ThreeBit => "3bit",
            Self::Byte3 => "byte3",
        }
    }
}

impl fmt::Display for StorageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StorageKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "byte" => Ok(Self::Byte),
            "nibble" => Ok(Self::Nibble),
            "3bit" => Ok(Self::ThreeBit),
            "byte3" => Ok(Self::Byte3),
            _ => Err(format!("unknown storage kind: {value}")),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ScrambleKind {
    RandomMoves,
    CenterCommutators,
}

impl ScrambleKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::RandomMoves => "random",
            Self::CenterCommutators => "comm",
        }
    }
}

impl fmt::Display for ScrambleKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ScrambleKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "random" | "random_moves" => Ok(Self::RandomMoves),
            "comm" | "center_commutators" => Ok(Self::CenterCommutators),
            _ => Err(format!("unknown scramble kind: {value}")),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct CenterCommutatorScramble {
    step_index: usize,
    row: usize,
    column: usize,
}

#[derive(Clone, Debug)]
enum ScramblePlan {
    RandomMoves(Vec<Move>),
    CenterCommutators(Vec<CenterCommutatorScramble>),
}

impl ScramblePlan {
    fn operation_count(&self) -> usize {
        match self {
            Self::RandomMoves(moves) => moves.len(),
            Self::CenterCommutators(operations) => operations.len(),
        }
    }
}

#[derive(Clone, Debug)]
struct StageBenchmarkResult {
    storage: StorageKind,
    side_length: usize,
    stage: &'static str,
    scramble_kind: ScrambleKind,
    allocation_threads: usize,
    move_threads: usize,
    storage_bytes: usize,
    scramble_operations: usize,
    scramble_stats: MoveStats,
    scramble_elapsed: Duration,
    solve_elapsed: Duration,
    center_facelets: usize,
    center_score_before: usize,
    center_score_after: usize,
    solved_facelets: usize,
    stage_moves: MoveStats,
}

fn main() {
    let side_powers = environment_usize_list("RUBIK_STAGE_BENCHMARK_SIDE_POWERS")
        .unwrap_or_else(|| DEFAULT_SIDE_POWERS.to_vec());
    let storage_kinds = environment_storage_list("RUBIK_STAGE_BENCHMARK_STORAGE")
        .unwrap_or_else(|| StorageKind::ALL.to_vec());
    let scramble_kind = environment_scramble_kind(
        "RUBIK_STAGE_BENCHMARK_SCRAMBLE_KIND",
        ScrambleKind::RandomMoves,
    );
    let random_seed = environment_u64("RUBIK_STAGE_BENCHMARK_RANDOM_SEED", DEFAULT_RANDOM_SEED);
    let scramble_rounds = environment_usize(
        "RUBIK_STAGE_BENCHMARK_SCRAMBLE_ROUNDS",
        DEFAULT_SCRAMBLE_ROUNDS,
    );
    let explicit_commutator_scrambles = env::var("RUBIK_STAGE_BENCHMARK_CENTER_COMMUTATORS")
        .or_else(|_| env::var("RUBIK_STAGE_BENCHMARK_SCRAMBLE_MOVES"))
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .expect("RUBIK_STAGE_BENCHMARK_CENTER_COMMUTATORS must be a usize")
        });
    let allocation_threads = environment_usize(
        "RUBIK_STAGE_BENCHMARK_ALLOCATION_THREADS",
        default_thread_count(),
    );
    let move_threads = environment_usize("RUBIK_STAGE_BENCHMARK_MOVE_THREADS", 1);

    assert!(
        allocation_threads > 0,
        "RUBIK_STAGE_BENCHMARK_ALLOCATION_THREADS must be greater than zero"
    );
    assert!(
        move_threads > 0,
        "RUBIK_STAGE_BENCHMARK_MOVE_THREADS must be greater than zero"
    );

    println!("stage solve benchmarks");
    println!("  stages=center_reduction");
    println!("  side_powers={side_powers:?}");
    println!("  storage={storage_kinds:?}");
    println!("  scramble_kind={scramble_kind}");
    println!("  random_seed={random_seed}");
    println!("  scramble_rounds={scramble_rounds}");
    println!("  explicit_center_commutators={explicit_commutator_scrambles:?}");
    println!("  allocation_threads={allocation_threads}");
    println!("  move_threads={move_threads}");
    println!();

    let mut results = Vec::new();

    for power in side_powers {
        let side_length = 1usize
            .checked_shl(power as u32)
            .expect("side length power overflowed usize");
        let scramble_operation_count = match scramble_kind {
            ScrambleKind::RandomMoves => scramble_rounds,
            ScrambleKind::CenterCommutators => {
                explicit_commutator_scrambles.unwrap_or(side_length * scramble_rounds)
            }
        };
        let scramble_plan = generate_scramble_plan(
            scramble_kind,
            side_length,
            scramble_operation_count,
            random_seed ^ side_length as u64,
        );

        for storage in storage_kinds.iter().copied() {
            let result = run_stage_benchmark(
                storage,
                scramble_kind,
                side_length,
                &scramble_plan,
                allocation_threads,
                move_threads,
            );
            results.push(result);
        }
    }

    print_results_table(&results);
}

fn run_stage_benchmark(
    storage: StorageKind,
    scramble_kind: ScrambleKind,
    side_length: usize,
    scramble_plan: &ScramblePlan,
    allocation_threads: usize,
    move_threads: usize,
) -> StageBenchmarkResult {
    match storage {
        StorageKind::Byte => run_stage_benchmark_for::<Byte>(
            storage,
            scramble_kind,
            side_length,
            scramble_plan,
            allocation_threads,
            move_threads,
        ),
        StorageKind::Nibble => run_stage_benchmark_for::<Nibble>(
            storage,
            scramble_kind,
            side_length,
            scramble_plan,
            allocation_threads,
            move_threads,
        ),
        StorageKind::ThreeBit => run_stage_benchmark_for::<ThreeBit>(
            storage,
            scramble_kind,
            side_length,
            scramble_plan,
            allocation_threads,
            move_threads,
        ),
        StorageKind::Byte3 => run_stage_benchmark_for::<Byte3>(
            storage,
            scramble_kind,
            side_length,
            scramble_plan,
            allocation_threads,
            move_threads,
        ),
    }
}

fn run_stage_benchmark_for<S: FaceletArray + 'static>(
    storage: StorageKind,
    scramble_kind: ScrambleKind,
    side_length: usize,
    scramble_plan: &ScramblePlan,
    allocation_threads: usize,
    move_threads: usize,
) -> StageBenchmarkResult {
    let mut cube = Cube::<S>::new_solved_with_threads(side_length, allocation_threads);
    let storage_bytes = cube.estimated_storage_bytes();

    let scramble_start = Instant::now();
    let scramble_stats = apply_scramble_plan(&mut cube, scramble_plan, move_threads);
    let scramble_elapsed = scramble_start.elapsed();

    let center_score_before = center_score(&cube);
    let center_facelets = center_facelet_count(side_length);
    let mut stage = CenterReductionStage::western_default();
    let mut context = SolveContext::new(SolveOptions {
        thread_count: move_threads,
        record_moves: false,
    });

    let solve_start = Instant::now();
    <CenterReductionStage as SolverStage<S>>::run(&mut stage, &mut cube, &mut context)
        .expect("center stage failed");
    let solve_elapsed = solve_start.elapsed();

    let center_score_after = center_score(&cube);
    assert!(
        centers_are_solved(&cube),
        "center stage reported success without solved centers"
    );

    black_box(&cube);

    StageBenchmarkResult {
        storage,
        side_length,
        stage: "center_reduction",
        scramble_kind,
        allocation_threads,
        move_threads,
        storage_bytes,
        scramble_operations: scramble_plan.operation_count(),
        scramble_stats,
        scramble_elapsed,
        solve_elapsed,
        center_facelets,
        center_score_before,
        center_score_after,
        solved_facelets: center_score_after - center_score_before,
        stage_moves: context.move_stats(),
    }
}

const TABLE_HEADERS: [&str; 29] = [
    "storage", "n", "stage", "scramble", "athr", "mthr", "memory", "scr_ops", "scr_mv", "scr_ms",
    "sol_ms", "centers", "before", "after", "fixed", "sol_mv", "mv/fix", "mv/ctr", "mv/s", "fix/s",
    "outer", "inner", "x", "y", "z", "pos", "dbl", "neg", "scr/s",
];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum ColumnAlignment {
    Left,
    Right,
}

const TABLE_ALIGNMENTS: [ColumnAlignment; 29] = [
    ColumnAlignment::Left,
    ColumnAlignment::Right,
    ColumnAlignment::Left,
    ColumnAlignment::Left,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
    ColumnAlignment::Right,
];

fn print_results_table(results: &[StageBenchmarkResult]) {
    let rows = results.iter().map(result_cells).collect::<Vec<_>>();
    let widths = table_widths(&rows);

    print_table_row(
        &TABLE_HEADERS.map(str::to_owned),
        &widths,
        &TABLE_ALIGNMENTS,
    );
    print_separator(&widths);

    for row in rows {
        print_table_row(&row, &widths, &TABLE_ALIGNMENTS);
    }
}

fn table_widths(rows: &[[String; TABLE_HEADERS.len()]]) -> [usize; TABLE_HEADERS.len()] {
    let mut widths = TABLE_HEADERS.map(str::len);

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    widths
}

fn print_table_row(
    row: &[String; TABLE_HEADERS.len()],
    widths: &[usize; TABLE_HEADERS.len()],
    alignments: &[ColumnAlignment; TABLE_HEADERS.len()],
) {
    for (index, cell) in row.iter().enumerate() {
        if index > 0 {
            print!(" | ");
        }

        match alignments[index] {
            ColumnAlignment::Left => print!("{:<width$}", cell, width = widths[index]),
            ColumnAlignment::Right => print!("{:>width$}", cell, width = widths[index]),
        }
    }
    println!();
}

fn print_separator(widths: &[usize; TABLE_HEADERS.len()]) {
    for (index, width) in widths.iter().copied().enumerate() {
        if index > 0 {
            print!("-+-");
        }
        print!("{:-<width$}", "");
    }
    println!();
}

fn result_cells(result: &StageBenchmarkResult) -> [String; TABLE_HEADERS.len()] {
    [
        result.storage.to_string(),
        result.side_length.to_string(),
        result.stage.to_owned(),
        result.scramble_kind.to_string(),
        result.allocation_threads.to_string(),
        result.move_threads.to_string(),
        format_bytes(result.storage_bytes),
        result.scramble_operations.to_string(),
        result.scramble_stats.total.to_string(),
        format!("{:.1}", milliseconds(result.scramble_elapsed)),
        format!("{:.1}", milliseconds(result.solve_elapsed)),
        result.center_facelets.to_string(),
        result.center_score_before.to_string(),
        result.center_score_after.to_string(),
        result.solved_facelets.to_string(),
        result.stage_moves.total.to_string(),
        format!(
            "{:.3}",
            ratio(result.stage_moves.total, result.solved_facelets)
        ),
        format!(
            "{:.3}",
            ratio(result.stage_moves.total, result.center_facelets)
        ),
        format!(
            "{:.0}",
            per_second(result.stage_moves.total, result.solve_elapsed)
        ),
        format!(
            "{:.0}",
            per_second(result.solved_facelets, result.solve_elapsed)
        ),
        result.stage_moves.outer_layer.to_string(),
        result.stage_moves.inner_layer.to_string(),
        result.stage_moves.axis_x.to_string(),
        result.stage_moves.axis_y.to_string(),
        result.stage_moves.axis_z.to_string(),
        result.stage_moves.positive.to_string(),
        result.stage_moves.double.to_string(),
        result.stage_moves.negative.to_string(),
        format!(
            "{:.0}",
            per_second(result.scramble_stats.total, result.scramble_elapsed)
        ),
    ]
}

fn generate_scramble_plan(
    kind: ScrambleKind,
    side_length: usize,
    operation_count: usize,
    seed: u64,
) -> ScramblePlan {
    match kind {
        ScrambleKind::RandomMoves => {
            ScramblePlan::RandomMoves(generate_scramble_moves(side_length, operation_count, seed))
        }
        ScrambleKind::CenterCommutators => ScramblePlan::CenterCommutators(
            generate_center_commutator_scrambles(side_length, operation_count, seed),
        ),
    }
}

fn generate_scramble_moves(side_length: usize, rounds: usize, seed: u64) -> Vec<Move> {
    let mut rng = XorShift64::new(seed);
    let mut moves = Vec::with_capacity(rounds * (side_length + FaceId::ALL.len()));

    for _ in 0..rounds {
        for _ in 0..side_length {
            moves.push(random_move(side_length, &mut rng));
        }

        for face in FaceId::ALL {
            moves.push(face_outer_move(
                side_length,
                face,
                random_move_angle(&mut rng),
            ));
        }
    }

    moves
}

fn random_move(side_length: usize, rng: &mut impl RandomSource) -> Move {
    let axis = match (rng.next_u64() % 3) as u8 {
        0 => Axis::X,
        1 => Axis::Y,
        _ => Axis::Z,
    };
    let depth = (rng.next_u64() as usize) % side_length;

    Move::new(axis, depth, random_move_angle(rng))
}

fn random_move_angle(rng: &mut impl RandomSource) -> MoveAngle {
    match (rng.next_u64() % 3) as u8 {
        0 => MoveAngle::Positive,
        1 => MoveAngle::Double,
        _ => MoveAngle::Negative,
    }
}

fn generate_center_commutator_scrambles(
    side_length: usize,
    count: usize,
    seed: u64,
) -> Vec<CenterCommutatorScramble> {
    let mut rng = XorShift64::new(seed);
    let mut operations = Vec::with_capacity(count);

    while operations.len() < count {
        let step_index = (rng.next_u64() as usize) % GENERATED_CENTER_SCHEDULE.len();
        let row = 1 + (rng.next_u64() as usize) % (side_length - 2);
        let column = 1 + (rng.next_u64() as usize) % (side_length - 2);

        if row == column {
            continue;
        }

        operations.push(CenterCommutatorScramble {
            step_index,
            row,
            column,
        });
    }

    operations
}

fn apply_scramble_plan<S: FaceletArray>(
    cube: &mut Cube<S>,
    plan: &ScramblePlan,
    move_threads: usize,
) -> MoveStats {
    match plan {
        ScramblePlan::RandomMoves(moves) => {
            cube.apply_moves_untracked_with_threads(moves.iter().copied(), move_threads);
            move_stats_for(moves, cube.side_len())
        }
        ScramblePlan::CenterCommutators(operations) => {
            apply_center_commutator_scrambles(cube, operations, move_threads)
        }
    }
}

fn apply_center_commutator_scrambles<S: FaceletArray>(
    cube: &mut Cube<S>,
    operations: &[CenterCommutatorScramble],
    _move_threads: usize,
) -> MoveStats {
    let table = CenterCommutatorTable::new();
    let mut stats = MoveStats::default();

    for operation in operations.iter().copied() {
        let step = GENERATED_CENTER_SCHEDULE[operation.step_index];
        let commutator = table
            .get(step.destination, step.helper, step.angle)
            .expect("generated center schedule step must have a commutator");

        for _ in 0..2 {
            record_normalized_center_commutator_move_stats(
                &mut stats,
                cube.side_len(),
                commutator,
                &[operation.row],
                &[operation.column],
            );
            cube.apply_normalized_face_commutator_plan_untracked(
                commutator,
                &[operation.row],
                &[operation.column],
            );
        }
    }

    stats
}

fn move_stats_for(moves: &[Move], side_length: usize) -> MoveStats {
    let mut stats = MoveStats::default();
    stats.record_all(moves.iter().copied(), side_length);
    stats
}

fn record_center_commutator_move_stats(
    stats: &mut MoveStats,
    side_length: usize,
    commutator: rubik::FaceCommutator,
    rows: &[usize],
    columns: &[usize],
) {
    let reverse = commutator.slice_angle().inverse();

    for column in columns.iter().copied() {
        stats.record(
            face_layer_move(side_length, commutator.helper(), column, reverse),
            side_length,
        );
    }
    stats.record(
        face_outer_move(side_length, commutator.destination(), MoveAngle::Positive),
        side_length,
    );
    for row in rows.iter().copied() {
        stats.record(
            face_layer_move(side_length, commutator.helper(), row, reverse),
            side_length,
        );
    }
    stats.record(
        face_outer_move(side_length, commutator.destination(), MoveAngle::Negative),
        side_length,
    );
    for column in columns.iter().copied() {
        stats.record(
            face_layer_move(
                side_length,
                commutator.helper(),
                column,
                commutator.slice_angle(),
            ),
            side_length,
        );
    }
    stats.record(
        face_outer_move(side_length, commutator.destination(), MoveAngle::Positive),
        side_length,
    );
    for row in rows.iter().copied() {
        stats.record(
            face_layer_move(
                side_length,
                commutator.helper(),
                row,
                commutator.slice_angle(),
            ),
            side_length,
        );
    }
}

fn record_normalized_center_commutator_move_stats(
    stats: &mut MoveStats,
    side_length: usize,
    commutator: rubik::FaceCommutator,
    rows: &[usize],
    columns: &[usize],
) {
    record_center_commutator_move_stats(stats, side_length, commutator, rows, columns);
    stats.record(
        face_outer_move(side_length, commutator.destination(), MoveAngle::Positive).inverse(),
        side_length,
    );
}

fn face_outer_move(side_length: usize, face: FaceId, angle: MoveAngle) -> Move {
    face_layer_move(side_length, face, 0, angle)
}

fn face_layer_move(
    side_length: usize,
    face: FaceId,
    depth_from_face: usize,
    angle: MoveAngle,
) -> Move {
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

fn centers_are_solved<S: FaceletArray>(cube: &Cube<S>) -> bool {
    FaceId::ALL.iter().copied().all(|face| {
        let target = Facelet::from_u8(face.index() as u8);
        for row in 1..cube.side_len().saturating_sub(1) {
            for column in 1..cube.side_len().saturating_sub(1) {
                if cube.face(face).get(row, column) != target {
                    return false;
                }
            }
        }
        true
    })
}

fn center_score<S: FaceletArray>(cube: &Cube<S>) -> usize {
    let mut score = 0;

    for face in FaceId::ALL {
        let target = Facelet::from_u8(face.index() as u8);
        for row in 1..cube.side_len().saturating_sub(1) {
            for column in 1..cube.side_len().saturating_sub(1) {
                score += usize::from(cube.face(face).get(row, column) == target);
            }
        }
    }

    score
}

fn center_facelet_count(side_length: usize) -> usize {
    let centers_per_face = side_length.saturating_sub(2);
    centers_per_face * centers_per_face * FaceId::ALL.len()
}

fn environment_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("{name} must be a usize"))
        })
        .unwrap_or(default)
}

fn environment_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .unwrap_or_else(|_| panic!("{name} must be a u64"))
        })
        .unwrap_or(default)
}

fn environment_scramble_kind(name: &str, default: ScrambleKind) -> ScrambleKind {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<ScrambleKind>()
                .unwrap_or_else(|error| panic!("{error}"))
        })
        .unwrap_or(default)
}

fn environment_usize_list(name: &str) -> Option<Vec<usize>> {
    let value = env::var(name).ok()?;
    Some(
        value
            .split(',')
            .filter(|item| !item.trim().is_empty())
            .map(|item| {
                item.trim()
                    .parse::<usize>()
                    .unwrap_or_else(|_| panic!("{name} must be a comma-separated usize list"))
            })
            .collect(),
    )
}

fn environment_storage_list(name: &str) -> Option<Vec<StorageKind>> {
    let value = env::var(name).ok()?;
    Some(
        value
            .split(',')
            .filter(|item| !item.trim().is_empty())
            .map(|item| {
                item.trim()
                    .parse::<StorageKind>()
                    .unwrap_or_else(|error| panic!("{error}"))
            })
            .collect(),
    )
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn per_second(count: usize, duration: Duration) -> f64 {
    if duration.is_zero() {
        0.0
    } else {
        count as f64 / duration.as_secs_f64()
    }
}

fn format_bytes(bytes: usize) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];

    for next_unit in UNITS.iter().skip(1).copied() {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next_unit;
    }

    if unit == "B" {
        format!("{bytes}{unit}")
    } else {
        format!("{value:.1}{unit}")
    }
}
