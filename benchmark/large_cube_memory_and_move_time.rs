use std::{
    env, fmt,
    hint::black_box,
    process::{Command, Stdio},
    str::FromStr,
    time::{Duration, Instant},
};

use rubik::{
    default_thread_count, line::cycle_four_line_arrays_many_with_threads, Axis, Byte, Byte3,
    ColorScheme, Cube, Facelet, FaceletArray, Move, MoveAngle, Nibble, ThreeBit, XorShift64,
};

const DEFAULT_MEMORY_SIDE_LENGTH: usize = 10_000;
const DEFAULT_RANDOM_MOVE_SIDE_LENGTH: usize = 10_000;
const DEFAULT_RANDOM_MOVE_COUNT: usize = 1_000;
const DEFAULT_SLICE_MOVE_SIDE_LENGTH: usize = 1_000_000;
const DEFAULT_SLICE_MOVE_COUNT: usize = 100;
const DEFAULT_RANDOM_SEED: u64 = 0xC0BEE_CAFE_F00D;
const METRIC_COLUMN_WIDTH: usize = 28;
const STORAGE_COLUMN_WIDTH: usize = 8;
const STORAGE_KIND_COUNT: usize = 4;
const PARALLEL_THREAD_COUNT_COUNT: usize = 5;
const PARALLEL_THREAD_COUNTS: [usize; PARALLEL_THREAD_COUNT_COUNT] = [2, 4, 8, 16, 32];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StorageKind {
    Byte,
    Nibble,
    ThreeBit,
    Byte3,
}

impl StorageKind {
    const ALL: [Self; STORAGE_KIND_COUNT] = [Self::Byte, Self::Nibble, Self::ThreeBit, Self::Byte3];

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
        f.pad(self.as_str())
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

#[derive(Copy, Clone, Debug)]
struct MemoryResult {
    side_length: usize,
    allocation_thread_count: usize,
    face_storage_bytes: usize,
    resident_memory_before_bytes: usize,
    resident_memory_after_bytes: usize,
    peak_resident_memory_bytes: usize,
    allocation_time: Duration,
}

#[derive(Copy, Clone, Debug)]
struct MoveResult {
    side_length: usize,
    move_count: usize,
    allocation_thread_count: usize,
    move_thread_count: usize,
    face_storage_bytes: usize,
    elapsed: Duration,
}

#[derive(Copy, Clone, Debug)]
struct MoveSpeedupResult {
    baseline: MoveResult,
    ratios: [f64; PARALLEL_THREAD_COUNT_COUNT],
}

#[derive(Copy, Clone, Debug)]
struct SliceMoveResult {
    side_length: usize,
    move_count: usize,
    allocation_thread_count: usize,
    move_thread_count: usize,
    slice_storage_bytes: usize,
    elapsed: Duration,
}

#[derive(Copy, Clone, Debug)]
struct SliceMoveSpeedupResult {
    baseline: SliceMoveResult,
    ratios: [f64; PARALLEL_THREAD_COUNT_COUNT],
}

fn main() {
    if env::var("RUBIK_BENCHMARK_CHILD_PROCESS").as_deref() == Ok("memory") {
        run_memory_child();
        return;
    }

    let memory_side_length = environment_usize(
        "RUBIK_BENCHMARK_MEMORY_SIDE_LENGTH",
        DEFAULT_MEMORY_SIDE_LENGTH,
    );
    let random_move_side_length = environment_usize(
        "RUBIK_BENCHMARK_RANDOM_MOVE_SIDE_LENGTH",
        DEFAULT_RANDOM_MOVE_SIDE_LENGTH,
    );
    let random_move_count = environment_usize(
        "RUBIK_BENCHMARK_RANDOM_MOVE_COUNT",
        DEFAULT_RANDOM_MOVE_COUNT,
    );
    let slice_move_side_length = environment_usize(
        "RUBIK_BENCHMARK_SLICE_MOVE_SIDE_LENGTH",
        DEFAULT_SLICE_MOVE_SIDE_LENGTH,
    );
    let slice_move_count =
        environment_usize("RUBIK_BENCHMARK_SLICE_MOVE_COUNT", DEFAULT_SLICE_MOVE_COUNT);
    let random_seed = environment_u64("RUBIK_BENCHMARK_RANDOM_SEED", DEFAULT_RANDOM_SEED);
    let skip_memory = environment_bool("RUBIK_BENCHMARK_SKIP_MEMORY", false);
    let skip_full_moves = environment_bool("RUBIK_BENCHMARK_SKIP_FULL_MOVES", false);
    let default_threads = default_thread_count();

    println!("large side length cube benchmarks");
    println!("  default_thread_count={default_threads}");
    println!("  benchmark_move_thread_counts=1,2,4,8,16,32");
    println!("  RUBIK_BENCHMARK_MEMORY_SIDE_LENGTH={memory_side_length}");
    println!("  RUBIK_BENCHMARK_RANDOM_MOVE_SIDE_LENGTH={random_move_side_length}");
    println!("  RUBIK_BENCHMARK_RANDOM_MOVE_COUNT={random_move_count}");
    println!("  RUBIK_BENCHMARK_SLICE_MOVE_SIDE_LENGTH={slice_move_side_length}");
    println!("  RUBIK_BENCHMARK_SLICE_MOVE_COUNT={slice_move_count}");
    println!("  RUBIK_BENCHMARK_RANDOM_SEED={random_seed}");
    println!("  RUBIK_BENCHMARK_SKIP_MEMORY={skip_memory}");
    println!("  RUBIK_BENCHMARK_SKIP_FULL_MOVES={skip_full_moves}");
    println!();

    if skip_memory {
        println!("memory allocation skipped");
    } else {
        let memory_results =
            StorageKind::ALL.map(|storage| run_memory_parent(storage, memory_side_length));

        println!("memory allocation, isolated child processes");
        print_table_header();
        print_table_row(
            "side_length",
            memory_results.map(|result| result.side_length.to_string()),
        );
        print_table_row(
            "allocation_threads",
            memory_results.map(|result| result.allocation_thread_count.to_string()),
        );
        print_table_row(
            "face_storage",
            memory_results.map(|result| format_bytes(result.face_storage_bytes)),
        );
        print_table_row(
            "resident_memory_before",
            memory_results.map(|result| format_bytes(result.resident_memory_before_bytes)),
        );
        print_table_row(
            "resident_memory_after",
            memory_results.map(|result| format_bytes(result.resident_memory_after_bytes)),
        );
        print_table_row(
            "peak_resident_memory",
            memory_results.map(|result| format_bytes(result.peak_resident_memory_bytes)),
        );
        print_table_row(
            "allocation_milliseconds",
            memory_results.map(|result| format!("{:.3}", milliseconds(result.allocation_time))),
        );
    }

    println!();
    if skip_full_moves {
        println!("full cube random move application skipped");
    } else {
        println!("random move application, same pre-generated move list, history disabled");

        let moves = generate_moves(random_move_side_length, random_move_count, random_seed);
        let move_results = StorageKind::ALL
            .map(|storage| run_move_speedup_benchmark(storage, random_move_side_length, &moves));

        print_table_header();
        print_move_speedup_results(move_results);
    }

    println!();
    println!("line-only side-strip cycle, four 1D lines, no square face allocation");

    let angles = generate_angles(slice_move_count, random_seed);
    let slice_move_results = StorageKind::ALL
        .map(|storage| run_slice_move_speedup_benchmark(storage, slice_move_side_length, &angles));

    print_table_header();
    print_slice_move_speedup_results(slice_move_results);
}

fn print_table_header() {
    print!("{:<width$}", "metric", width = METRIC_COLUMN_WIDTH);

    for storage in StorageKind::ALL {
        print!(" | {:>width$}", storage, width = STORAGE_COLUMN_WIDTH);
    }
    println!();

    print!("{:-<width$}", "", width = METRIC_COLUMN_WIDTH);
    for _ in StorageKind::ALL {
        print!("-+-{:-<width$}", "", width = STORAGE_COLUMN_WIDTH);
    }
    println!();
}

fn print_table_row(metric: &str, values: [String; STORAGE_KIND_COUNT]) {
    print!("{:<width$}", metric, width = METRIC_COLUMN_WIDTH);

    for value in values {
        print!(" | {:>width$}", value, width = STORAGE_COLUMN_WIDTH);
    }
    println!();
}

fn print_move_speedup_results(results: [MoveSpeedupResult; STORAGE_KIND_COUNT]) {
    let baseline_results = results.map(|result| result.baseline);

    print_table_row(
        "side_length",
        baseline_results.map(|result| result.side_length.to_string()),
    );
    print_table_row(
        "move_count",
        baseline_results.map(|result| result.move_count.to_string()),
    );
    print_table_row(
        "allocation_threads",
        baseline_results.map(|result| result.allocation_thread_count.to_string()),
    );
    print_table_row(
        "baseline_move_threads",
        baseline_results.map(|result| result.move_thread_count.to_string()),
    );
    print_table_row(
        "face_storage",
        baseline_results.map(|result| format_bytes(result.face_storage_bytes)),
    );
    print_table_row(
        "elapsed_milliseconds",
        baseline_results.map(|result| format!("{:.3}", milliseconds(result.elapsed))),
    );
    print_table_row(
        "moves_per_second",
        baseline_results
            .map(|result| format!("{:.1}", moves_per_second(result.move_count, result.elapsed))),
    );
    print_table_row(
        "ns_per_line_cell",
        baseline_results.map(|result| {
            format!(
                "{:.3}",
                nanoseconds_per_line_cell(result.side_length, result.move_count, result.elapsed)
            )
        }),
    );

    print_speedup_rows(results.map(|result| result.ratios));
}

fn print_slice_move_speedup_results(results: [SliceMoveSpeedupResult; STORAGE_KIND_COUNT]) {
    let baseline_results = results.map(|result| result.baseline);

    print_table_row(
        "side_length",
        baseline_results.map(|result| result.side_length.to_string()),
    );
    print_table_row(
        "move_count",
        baseline_results.map(|result| result.move_count.to_string()),
    );
    print_table_row(
        "allocation_threads",
        baseline_results.map(|result| result.allocation_thread_count.to_string()),
    );
    print_table_row(
        "baseline_move_threads",
        baseline_results.map(|result| result.move_thread_count.to_string()),
    );
    print_table_row(
        "linear_storage",
        baseline_results.map(|result| format_bytes(result.slice_storage_bytes)),
    );
    print_table_row(
        "elapsed_milliseconds",
        baseline_results.map(|result| format!("{:.3}", milliseconds(result.elapsed))),
    );
    print_table_row(
        "moves_per_second",
        baseline_results
            .map(|result| format!("{:.1}", moves_per_second(result.move_count, result.elapsed))),
    );
    print_table_row(
        "ns_per_line_cell",
        baseline_results.map(|result| {
            format!(
                "{:.3}",
                nanoseconds_per_line_cell(result.side_length, result.move_count, result.elapsed)
            )
        }),
    );

    print_speedup_rows(results.map(|result| result.ratios));
}

fn print_speedup_rows(ratios: [[f64; PARALLEL_THREAD_COUNT_COUNT]; STORAGE_KIND_COUNT]) {
    for (ratio_index, thread_count) in PARALLEL_THREAD_COUNTS.iter().copied().enumerate() {
        let label = format!("{thread_count}_threads_speedup");
        print_table_row(
            &label,
            ratios.map(|storage_ratios| format!("{:.3}", storage_ratios[ratio_index])),
        );
    }
}

fn run_memory_child() {
    let storage = env::var("RUBIK_BENCHMARK_STORAGE")
        .expect("RUBIK_BENCHMARK_STORAGE is required")
        .parse::<StorageKind>()
        .expect("invalid RUBIK_BENCHMARK_STORAGE");
    let side_length = environment_usize(
        "RUBIK_BENCHMARK_MEMORY_SIDE_LENGTH",
        DEFAULT_MEMORY_SIDE_LENGTH,
    );

    let resident_memory_before = current_resident_set_size_bytes().unwrap_or(0);
    let allocation_thread_count = default_thread_count();
    let start = Instant::now();
    let (resident_memory_after, peak_resident_memory, checksum) = match storage {
        StorageKind::Byte => allocate_and_measure::<Byte>(side_length, allocation_thread_count),
        StorageKind::Nibble => allocate_and_measure::<Nibble>(side_length, allocation_thread_count),
        StorageKind::ThreeBit => {
            allocate_and_measure::<ThreeBit>(side_length, allocation_thread_count)
        }
        StorageKind::Byte3 => allocate_and_measure::<Byte3>(side_length, allocation_thread_count),
    };
    let allocation_time = start.elapsed();

    black_box(checksum);

    println!(
        "{} {} {} {} {} {} {} {}",
        storage,
        side_length,
        allocation_thread_count,
        face_storage_bytes(storage, side_length),
        resident_memory_before,
        resident_memory_after,
        peak_resident_memory,
        allocation_time.as_nanos()
    );
}

fn allocate_and_measure<S: rubik::FaceletArray>(
    side_length: usize,
    allocation_thread_count: usize,
) -> (usize, usize, u8) {
    let scheme = ColorScheme {
        u: Facelet::Blue,
        d: Facelet::Blue,
        r: Facelet::Blue,
        l: Facelet::Blue,
        f: Facelet::Blue,
        b: Facelet::Blue,
    };
    let cube =
        Cube::<S>::new_with_scheme_with_threads(side_length, scheme, allocation_thread_count);
    let checksum = cube.face(rubik::FaceId::U).get(0, 0).as_u8()
        ^ cube
            .face(rubik::FaceId::D)
            .get(side_length - 1, side_length - 1)
            .as_u8();
    let resident_memory_after = current_resident_set_size_bytes().unwrap_or(0);
    let peak_resident_memory = peak_resident_set_size_bytes().unwrap_or(resident_memory_after);

    black_box(&cube);
    (resident_memory_after, peak_resident_memory, checksum)
}

fn run_memory_parent(storage: StorageKind, side_length: usize) -> MemoryResult {
    let executable_path =
        env::current_exe().expect("failed to resolve current benchmark executable");
    let output = Command::new(executable_path)
        .env("RUBIK_BENCHMARK_CHILD_PROCESS", "memory")
        .env("RUBIK_BENCHMARK_STORAGE", storage.as_str())
        .env(
            "RUBIK_BENCHMARK_MEMORY_SIDE_LENGTH",
            side_length.to_string(),
        )
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .expect("failed to run memory benchmark child");

    assert!(
        output.status.success(),
        "memory benchmark child failed for {storage}"
    );

    let stdout = String::from_utf8(output.stdout).expect("child output was not utf-8");
    parse_memory_result(stdout.trim())
}

fn parse_memory_result(line: &str) -> MemoryResult {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    assert_eq!(fields.len(), 8, "unexpected memory child output: {line}");
    let _storage: StorageKind = fields[0].parse().expect("invalid storage field");

    MemoryResult {
        side_length: fields[1].parse().expect("invalid side length field"),
        allocation_thread_count: fields[2]
            .parse()
            .expect("invalid allocation thread count field"),
        face_storage_bytes: fields[3].parse().expect("invalid face storage field"),
        resident_memory_before_bytes: fields[4]
            .parse()
            .expect("invalid resident memory before field"),
        resident_memory_after_bytes: fields[5]
            .parse()
            .expect("invalid resident memory after field"),
        peak_resident_memory_bytes: fields[6]
            .parse()
            .expect("invalid peak resident memory field"),
        allocation_time: Duration::from_nanos(
            fields[7]
                .parse()
                .expect("invalid allocation nanoseconds field"),
        ),
    }
}

fn run_move_speedup_benchmark(
    storage: StorageKind,
    side_length: usize,
    moves: &[Move],
) -> MoveSpeedupResult {
    let baseline = run_move_benchmark(storage, side_length, moves, 1);
    let ratios = PARALLEL_THREAD_COUNTS.map(|thread_count| {
        let result = run_move_benchmark(storage, side_length, moves, thread_count);
        speedup_ratio(baseline.elapsed, result.elapsed)
    });

    MoveSpeedupResult { baseline, ratios }
}

fn run_move_benchmark(
    storage: StorageKind,
    side_length: usize,
    moves: &[Move],
    thread_count: usize,
) -> MoveResult {
    match storage {
        StorageKind::Byte => {
            run_move_benchmark_for::<Byte>(storage, side_length, moves, thread_count)
        }
        StorageKind::Nibble => {
            run_move_benchmark_for::<Nibble>(storage, side_length, moves, thread_count)
        }
        StorageKind::ThreeBit => {
            run_move_benchmark_for::<ThreeBit>(storage, side_length, moves, thread_count)
        }
        StorageKind::Byte3 => {
            run_move_benchmark_for::<Byte3>(storage, side_length, moves, thread_count)
        }
    }
}

fn run_move_benchmark_for<S: rubik::FaceletArray>(
    storage: StorageKind,
    side_length: usize,
    moves: &[Move],
    thread_count: usize,
) -> MoveResult {
    let allocation_thread_count = default_thread_count();
    let mut cube = Cube::<S>::new_solved_with_threads(side_length, allocation_thread_count);
    let face_storage_bytes = face_storage_bytes(storage, side_length);

    let start = Instant::now();
    cube.apply_moves_untracked_with_threads(
        moves.iter().copied().map(|mv| black_box(mv)),
        thread_count,
    );
    let elapsed = start.elapsed();

    black_box(&cube);

    MoveResult {
        side_length,
        move_count: moves.len(),
        allocation_thread_count,
        move_thread_count: thread_count,
        face_storage_bytes,
        elapsed,
    }
}

fn run_slice_move_speedup_benchmark(
    storage: StorageKind,
    side_length: usize,
    angles: &[MoveAngle],
) -> SliceMoveSpeedupResult {
    let baseline = run_slice_move_benchmark(storage, side_length, angles, 1);
    let ratios = PARALLEL_THREAD_COUNTS.map(|thread_count| {
        let result = run_slice_move_benchmark(storage, side_length, angles, thread_count);
        speedup_ratio(baseline.elapsed, result.elapsed)
    });

    SliceMoveSpeedupResult { baseline, ratios }
}

fn run_slice_move_benchmark(
    storage: StorageKind,
    side_length: usize,
    angles: &[MoveAngle],
    thread_count: usize,
) -> SliceMoveResult {
    match storage {
        StorageKind::Byte => {
            run_slice_move_benchmark_for::<Byte>(storage, side_length, angles, thread_count)
        }
        StorageKind::Nibble => {
            run_slice_move_benchmark_for::<Nibble>(storage, side_length, angles, thread_count)
        }
        StorageKind::ThreeBit => {
            run_slice_move_benchmark_for::<ThreeBit>(storage, side_length, angles, thread_count)
        }
        StorageKind::Byte3 => {
            run_slice_move_benchmark_for::<Byte3>(storage, side_length, angles, thread_count)
        }
    }
}

fn run_slice_move_benchmark_for<S: rubik::FaceletArray>(
    storage: StorageKind,
    side_length: usize,
    angles: &[MoveAngle],
    thread_count: usize,
) -> SliceMoveResult {
    let allocation_thread_count = default_thread_count();
    let mut line0 = patterned_line::<S>(side_length, 0, allocation_thread_count);
    let mut line1 = patterned_line::<S>(side_length, 1, allocation_thread_count);
    let mut line2 = patterned_line::<S>(side_length, 2, allocation_thread_count);
    let mut line3 = patterned_line::<S>(side_length, 3, allocation_thread_count);
    let slice_storage_bytes = line_storage_bytes(storage, side_length);

    let start = Instant::now();
    cycle_four_line_arrays_many_with_threads(
        &mut line0,
        &mut line1,
        &mut line2,
        &mut line3,
        angles.iter().copied().map(|angle| black_box(angle)),
        thread_count,
    );
    let elapsed = start.elapsed();

    let last = side_length - 1;
    let checksum = line0.get(0).as_u8()
        ^ line1.get(last).as_u8()
        ^ line2.get(0).as_u8()
        ^ line3.get(last).as_u8();
    black_box(checksum);
    black_box((&line0, &line1, &line2, &line3));

    SliceMoveResult {
        side_length,
        move_count: angles.len(),
        allocation_thread_count,
        move_thread_count: thread_count,
        slice_storage_bytes,
        elapsed,
    }
}

fn patterned_line<S: rubik::FaceletArray>(
    side_length: usize,
    offset: u8,
    allocation_thread_count: usize,
) -> S {
    let mut line = S::with_len_with_threads(side_length, Facelet::White, allocation_thread_count);

    for index in 0..side_length {
        let raw = ((index.wrapping_mul(5) + offset as usize) % Facelet::ALL.len()) as u8;
        line.set(index, Facelet::from_u8(raw));
    }

    line
}

fn generate_moves(side_length: usize, count: usize, seed: u64) -> Vec<Move> {
    let mut rng = XorShift64::new(seed);
    let mut moves = Vec::with_capacity(count);

    for _ in 0..count {
        let axis = match next_u64(&mut rng) % 3 {
            0 => Axis::X,
            1 => Axis::Y,
            _ => Axis::Z,
        };
        let depth = (next_u64(&mut rng) as usize) % side_length;
        let angle = match next_u64(&mut rng) % 3 {
            0 => MoveAngle::Positive,
            1 => MoveAngle::Double,
            _ => MoveAngle::Negative,
        };
        moves.push(Move::new(axis, depth, angle));
    }

    moves
}

fn generate_angles(count: usize, seed: u64) -> Vec<MoveAngle> {
    let mut rng = XorShift64::new(seed);
    let mut angles = Vec::with_capacity(count);

    for _ in 0..count {
        let angle = match next_u64(&mut rng) % 3 {
            0 => MoveAngle::Positive,
            1 => MoveAngle::Double,
            _ => MoveAngle::Negative,
        };
        angles.push(angle);
    }

    angles
}

fn next_u64(rng: &mut XorShift64) -> u64 {
    use rubik::RandomSource;
    rng.next_u64()
}

fn environment_usize(name: &str, default: usize) -> usize {
    let value = env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .unwrap_or_else(|_| panic!("{name} must be a positive integer"))
        })
        .unwrap_or(default);
    assert!(value > 0, "{name} must be greater than zero");
    value
}

fn environment_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .unwrap_or_else(|_| panic!("{name} must be an unsigned 64-bit integer"))
        })
        .unwrap_or(default)
}

fn environment_bool(name: &str, default: bool) -> bool {
    env::var(name)
        .ok()
        .map(|value| match value.as_str() {
            "1" | "true" | "TRUE" | "yes" | "YES" => true,
            "0" | "false" | "FALSE" | "no" | "NO" => false,
            _ => panic!("{name} must be one of 1, 0, true, false, yes, or no"),
        })
        .unwrap_or(default)
}

fn current_resident_set_size_bytes() -> Option<usize> {
    proc_status_value_bytes("VmRSS:")
}

fn peak_resident_set_size_bytes() -> Option<usize> {
    proc_status_value_bytes("VmHWM:")
}

fn proc_status_value_bytes(key: &str) -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix(key) {
            let mut parts = rest.split_whitespace();
            let value = parts.next()?.parse::<usize>().ok()?;
            let unit = parts.next()?;
            return match unit {
                "kB" => value.checked_mul(1024),
                _ => None,
            };
        }
    }
    None
}

fn format_bytes(bytes: usize) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    const TIB: f64 = GIB * 1024.0;

    let bytes = bytes as f64;
    if bytes >= TIB {
        format!("{:.1}T", bytes / TIB)
    } else if bytes >= GIB {
        format!("{:.1}G", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1}M", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1}K", bytes / KIB)
    } else {
        format!("{bytes:.0}B")
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn speedup_ratio(baseline: Duration, elapsed: Duration) -> f64 {
    baseline.as_secs_f64() / elapsed.as_secs_f64()
}

fn moves_per_second(moves: usize, duration: Duration) -> f64 {
    moves as f64 / duration.as_secs_f64()
}

fn nanoseconds_per_line_cell(side_length: usize, moves: usize, duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000_000_000.0 / (side_length as f64 * moves as f64)
}

fn face_storage_bytes(storage: StorageKind, side_length: usize) -> usize {
    let cells_per_face = side_length
        .checked_mul(side_length)
        .expect("cube face cell count overflowed usize");

    match storage {
        StorageKind::Byte => cells_per_face
            .checked_mul(6)
            .expect("byte storage size overflowed usize"),
        StorageKind::Nibble => cells_per_face
            .div_ceil(2)
            .checked_mul(6)
            .expect("nibble storage size overflowed usize"),
        StorageKind::ThreeBit => cells_per_face
            .checked_mul(3)
            .expect("3bit storage bit count overflowed usize")
            .div_ceil(64)
            .checked_mul(8)
            .and_then(|bytes_per_face| bytes_per_face.checked_mul(6))
            .expect("3bit storage size overflowed usize"),
        StorageKind::Byte3 => cells_per_face
            .div_ceil(3)
            .checked_mul(6)
            .expect("byte3 storage size overflowed usize"),
    }
}

fn line_storage_bytes(storage: StorageKind, side_length: usize) -> usize {
    let bytes_per_line = match storage {
        StorageKind::Byte => Byte::storage_bytes_for_len(side_length),
        StorageKind::Nibble => Nibble::storage_bytes_for_len(side_length),
        StorageKind::ThreeBit => ThreeBit::storage_bytes_for_len(side_length),
        StorageKind::Byte3 => Byte3::storage_bytes_for_len(side_length),
    };

    bytes_per_line
        .checked_mul(4)
        .expect("line-only storage size overflowed usize")
}
