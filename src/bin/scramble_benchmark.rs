use std::{
    fmt::Write,
    fs,
    path::Path,
    time::{Duration, Instant},
};

use rubik::{
    conventions::face_outer_move, Axis, Byte, CenterReductionStage, CornerReductionStage, Cube,
    EdgePairingStage, ExecutionMode, FaceId, Move, MoveAngle, RandomSource, ReductionSolver,
    SolveOptions, Solver, XorShift64,
};

const MIN_SIZE: usize = 1;
const MAX_SIZE: usize = 20;
const ATTEMPTS: usize = 25;
const TRADITIONAL_ROUNDS: usize = 8;
const BASE_SEED: u64 = 0x51C4_AA11_0000;
const EXAMPLE_NET_PATH: &str = "tmp/direct_scramble_example_nets_1_to_7.txt";

fn main() -> Result<(), String> {
    validate_direct_scrambles()?;
    let rows = benchmark_scramblers();
    print_benchmark(&rows);
    write_example_nets(Path::new(EXAMPLE_NET_PATH))?;
    println!("\nwrote {EXAMPLE_NET_PATH}");
    Ok(())
}

#[derive(Copy, Clone, Debug)]
struct BenchmarkRow {
    side_length: usize,
    direct: Duration,
    traditional: Duration,
}

fn validate_direct_scrambles() -> Result<(), String> {
    for side_length in MIN_SIZE..=MAX_SIZE {
        for mode in [ExecutionMode::Standard, ExecutionMode::Optimized] {
            let mut cube = Cube::<Byte>::new_solved(side_length);
            let mut rng = XorShift64::new(BASE_SEED ^ side_length as u64);
            cube.scramble_direct(&mut rng);

            let mut solver = ReductionSolver::<Byte>::new(SolveOptions::new(mode))
                .with_stage(CenterReductionStage::western_default())
                .with_stage(CornerReductionStage::default())
                .with_stage(EdgePairingStage::default());
            solver
                .solve(&mut cube)
                .map_err(|error| format!("n={side_length} mode={mode:?} failed: {error}"))?;

            if !cube.is_solved() {
                return Err(format!("n={side_length} mode={mode:?} did not solve"));
            }
        }
    }

    println!(
        "validated direct scrambles for n={MIN_SIZE}..={MAX_SIZE} in standard and optimized modes"
    );
    Ok(())
}

fn benchmark_scramblers() -> Vec<BenchmarkRow> {
    let mut rows = Vec::new();

    for side_length in MIN_SIZE..=MAX_SIZE {
        let mut direct_total = Duration::ZERO;
        let mut traditional_total = Duration::ZERO;

        for attempt in 0..ATTEMPTS {
            let seed = BASE_SEED ^ ((side_length as u64) << 16) ^ attempt as u64;

            let mut direct = Cube::<Byte>::new_solved(side_length);
            let mut direct_rng = XorShift64::new(seed);
            let start = Instant::now();
            direct.scramble_direct(&mut direct_rng);
            direct_total += start.elapsed();

            let moves = traditional_scramble_moves(side_length, TRADITIONAL_ROUNDS, seed);
            let mut traditional = Cube::<Byte>::new_solved(side_length);
            let start = Instant::now();
            traditional.apply_moves_untracked(moves);
            traditional_total += start.elapsed();
        }

        rows.push(BenchmarkRow {
            side_length,
            direct: direct_total / ATTEMPTS as u32,
            traditional: traditional_total / ATTEMPTS as u32,
        });
    }

    rows
}

fn print_benchmark(rows: &[BenchmarkRow]) {
    println!();
    println!("average scramble time over {ATTEMPTS} attempts");
    println!("n   direct_ms  traditional_8_round_ms  speedup");
    for row in rows {
        let direct_ms = row.direct.as_secs_f64() * 1000.0;
        let traditional_ms = row.traditional.as_secs_f64() * 1000.0;
        let speedup = if direct_ms > 0.0 {
            traditional_ms / direct_ms
        } else {
            f64::INFINITY
        };
        println!(
            "{:<3} {:>9.4} {:>23.4} {:>8.2}x",
            row.side_length, direct_ms, traditional_ms, speedup
        );
    }
}

fn write_example_nets(path: &Path) -> Result<(), String> {
    let mut out = String::new();

    for side_length in 1..=7 {
        let mut cube = Cube::<Byte>::new_solved(side_length);
        let mut rng = XorShift64::new(BASE_SEED ^ 0xE4A4_0000 ^ side_length as u64);
        cube.scramble_direct(&mut rng);
        writeln!(
            out,
            "direct scramble example n={side_length}, seed=0x{:016X}\n{}",
            BASE_SEED ^ 0xE4A4_0000 ^ side_length as u64,
            cube.net_string(),
        )
        .map_err(|error| error.to_string())?;
    }

    fs::write(path, out).map_err(|error| format!("failed to write {}: {error}", path.display()))
}

fn traditional_scramble_moves(side_length: usize, rounds: usize, seed: u64) -> Vec<Move> {
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
