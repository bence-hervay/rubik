use std::{
    fmt::Write,
    fs,
    path::Path,
    time::{Duration, Instant},
};

use rubik::{
    Byte, CenterReductionStage, CornerReductionStage, Cube, EdgePairingStage, ExecutionMode,
    ReductionSolver, SolveOptions, Solver, XorShift64, DEFAULT_SCRAMBLE_ROUNDS,
};

const MIN_SIZE: usize = 1;
const MAX_SIZE: usize = 20;
const ATTEMPTS: usize = 25;
const BASE_SEED: u64 = 0x51C4_AA11_0000;
const EXAMPLE_NET_PATH: &str = "tmp/random_layer_scramble_example_nets_1_to_7.txt";

fn main() -> Result<(), String> {
    validate_random_layer_scrambles()?;
    let rows = benchmark_scramblers();
    print_benchmark(&rows);
    write_example_nets(Path::new(EXAMPLE_NET_PATH))?;
    println!("\nwrote {EXAMPLE_NET_PATH}");
    Ok(())
}

#[derive(Copy, Clone, Debug)]
struct BenchmarkRow {
    side_length: usize,
    random_layer: Duration,
    direct_reference: Duration,
}

fn validate_random_layer_scrambles() -> Result<(), String> {
    for side_length in MIN_SIZE..=MAX_SIZE {
        for mode in [ExecutionMode::Standard, ExecutionMode::Optimized] {
            let mut cube = Cube::<Byte>::new_solved(side_length);
            let mut rng = XorShift64::new(BASE_SEED ^ side_length as u64);
            cube.scramble(&mut rng);

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
        "validated {DEFAULT_SCRAMBLE_ROUNDS}-round uniform random layer scrambles for n={MIN_SIZE}..={MAX_SIZE} in standard and optimized modes"
    );
    Ok(())
}

fn benchmark_scramblers() -> Vec<BenchmarkRow> {
    let mut rows = Vec::new();

    for side_length in MIN_SIZE..=MAX_SIZE {
        let mut random_layer_total = Duration::ZERO;
        let mut direct_reference_total = Duration::ZERO;

        for attempt in 0..ATTEMPTS {
            let seed = BASE_SEED ^ ((side_length as u64) << 16) ^ attempt as u64;

            let mut random_layer = Cube::<Byte>::new_solved(side_length);
            let mut random_layer_rng = XorShift64::new(seed);
            let start = Instant::now();
            random_layer.scramble(&mut random_layer_rng);
            random_layer_total += start.elapsed();

            let mut direct_reference = Cube::<Byte>::new_solved(side_length);
            let mut direct_reference_rng = XorShift64::new(seed);
            let start = Instant::now();
            direct_reference.scramble_direct(&mut direct_reference_rng);
            direct_reference_total += start.elapsed();
        }

        rows.push(BenchmarkRow {
            side_length,
            random_layer: random_layer_total / ATTEMPTS as u32,
            direct_reference: direct_reference_total / ATTEMPTS as u32,
        });
    }

    rows
}

fn print_benchmark(rows: &[BenchmarkRow]) {
    println!();
    println!("average scramble time over {ATTEMPTS} attempts");
    println!(
        "n   random_layer_{DEFAULT_SCRAMBLE_ROUNDS}_round_ms  direct_reference_ms  direct/random"
    );
    for row in rows {
        let random_layer_ms = row.random_layer.as_secs_f64() * 1000.0;
        let direct_reference_ms = row.direct_reference.as_secs_f64() * 1000.0;
        let ratio = if random_layer_ms > 0.0 {
            direct_reference_ms / random_layer_ms
        } else {
            f64::INFINITY
        };
        println!(
            "{:<3} {:>23.4} {:>20.4} {:>12.2}x",
            row.side_length, random_layer_ms, direct_reference_ms, ratio
        );
    }
}

fn write_example_nets(path: &Path) -> Result<(), String> {
    let mut out = String::new();

    for side_length in 1..=7 {
        let mut cube = Cube::<Byte>::new_solved(side_length);
        let mut rng = XorShift64::new(BASE_SEED ^ 0xE4A4_0000 ^ side_length as u64);
        cube.scramble(&mut rng);
        writeln!(
            out,
            "uniform random layer scramble example n={side_length}, rounds={DEFAULT_SCRAMBLE_ROUNDS}, seed=0x{:016X}\n{}",
            BASE_SEED ^ 0xE4A4_0000 ^ side_length as u64,
            cube.net_string(),
        )
        .map_err(|error| error.to_string())?;
    }

    fs::write(path, out).map_err(|error| format!("failed to write {}: {error}", path.display()))
}
