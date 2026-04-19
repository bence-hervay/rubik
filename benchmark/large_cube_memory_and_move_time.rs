use std::{
    env, fmt,
    hint::black_box,
    process::{Command, Stdio},
    str::FromStr,
    time::{Duration, Instant},
};

use rubik::{
    Angle, Axis, ByteArray, ColorScheme, Cube, Facelet, Move, NibbleArray, Packed3Array, XorShift64,
};

const DEFAULT_MEMORY_SIDE_LENGTH: usize = 4096;
const DEFAULT_RANDOM_MOVE_SIDE_LENGTH: usize = 4096;
const DEFAULT_RANDOM_MOVE_COUNT: usize = 1_000;
const DEFAULT_RANDOM_SEED: u64 = 0xC0BEE_CAFE_F00D;
const METRIC_COLUMN_WIDTH: usize = 28;
const STORAGE_COLUMN_WIDTH: usize = 20;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StorageKind {
    Byte,
    Nibble,
    PackedThreeBits,
}

impl StorageKind {
    const ALL: [Self; 3] = [Self::Byte, Self::Nibble, Self::PackedThreeBits];

    fn as_str(self) -> &'static str {
        match self {
            Self::Byte => "byte",
            Self::Nibble => "nibble",
            Self::PackedThreeBits => "packed_three_bits",
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
            "packed_three_bits" => Ok(Self::PackedThreeBits),
            _ => Err(format!("unknown storage kind: {value}")),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct MemoryResult {
    side_length: usize,
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
    face_storage_bytes: usize,
    elapsed: Duration,
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
    let random_seed = environment_u64("RUBIK_BENCHMARK_RANDOM_SEED", DEFAULT_RANDOM_SEED);

    println!("large side length cube benchmarks");
    println!("  RUBIK_BENCHMARK_MEMORY_SIDE_LENGTH={memory_side_length}");
    println!("  RUBIK_BENCHMARK_RANDOM_MOVE_SIDE_LENGTH={random_move_side_length}");
    println!("  RUBIK_BENCHMARK_RANDOM_MOVE_COUNT={random_move_count}");
    println!("  RUBIK_BENCHMARK_RANDOM_SEED={random_seed}");
    println!();

    let memory_results =
        StorageKind::ALL.map(|storage| run_memory_parent(storage, memory_side_length));

    println!("memory allocation, isolated child processes");
    print_table_header();
    print_table_row(
        "side_length",
        memory_results.map(|result| result.side_length.to_string()),
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

    println!();
    println!("random move application, same pre-generated move list, history disabled");

    let moves = generate_moves(random_move_side_length, random_move_count, random_seed);
    let move_results = StorageKind::ALL
        .map(|storage| run_move_benchmark(storage, random_move_side_length, &moves));

    print_table_header();
    print_table_row(
        "side_length",
        move_results.map(|result| result.side_length.to_string()),
    );
    print_table_row(
        "move_count",
        move_results.map(|result| result.move_count.to_string()),
    );
    print_table_row(
        "face_storage",
        move_results.map(|result| format_bytes(result.face_storage_bytes)),
    );
    print_table_row(
        "elapsed_milliseconds",
        move_results.map(|result| format!("{:.3}", milliseconds(result.elapsed))),
    );
    print_table_row(
        "moves_per_second",
        move_results
            .map(|result| format!("{:.1}", moves_per_second(result.move_count, result.elapsed))),
    );
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

fn print_table_row(metric: &str, values: [String; 3]) {
    print!("{:<width$}", metric, width = METRIC_COLUMN_WIDTH);

    for value in values {
        print!(" | {:>width$}", value, width = STORAGE_COLUMN_WIDTH);
    }
    println!();
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
    let start = Instant::now();
    let (resident_memory_after, peak_resident_memory, checksum) = match storage {
        StorageKind::Byte => allocate_and_measure::<ByteArray>(side_length),
        StorageKind::Nibble => allocate_and_measure::<NibbleArray>(side_length),
        StorageKind::PackedThreeBits => allocate_and_measure::<Packed3Array>(side_length),
    };
    let allocation_time = start.elapsed();

    black_box(checksum);

    println!(
        "{} {} {} {} {} {} {}",
        storage,
        side_length,
        face_storage_bytes(storage, side_length),
        resident_memory_before,
        resident_memory_after,
        peak_resident_memory,
        allocation_time.as_nanos()
    );
}

fn allocate_and_measure<S: rubik::FaceletArray>(side_length: usize) -> (usize, usize, u8) {
    let scheme = ColorScheme {
        u: Facelet::Blue,
        d: Facelet::Blue,
        r: Facelet::Blue,
        l: Facelet::Blue,
        f: Facelet::Blue,
        b: Facelet::Blue,
    };
    let cube = Cube::<S>::new_with_scheme(side_length, scheme);
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
    assert_eq!(fields.len(), 7, "unexpected memory child output: {line}");
    let _storage: StorageKind = fields[0].parse().expect("invalid storage field");

    MemoryResult {
        side_length: fields[1].parse().expect("invalid side length field"),
        face_storage_bytes: fields[2].parse().expect("invalid face storage field"),
        resident_memory_before_bytes: fields[3]
            .parse()
            .expect("invalid resident memory before field"),
        resident_memory_after_bytes: fields[4]
            .parse()
            .expect("invalid resident memory after field"),
        peak_resident_memory_bytes: fields[5]
            .parse()
            .expect("invalid peak resident memory field"),
        allocation_time: Duration::from_nanos(
            fields[6]
                .parse()
                .expect("invalid allocation nanoseconds field"),
        ),
    }
}

fn run_move_benchmark(storage: StorageKind, side_length: usize, moves: &[Move]) -> MoveResult {
    match storage {
        StorageKind::Byte => run_move_benchmark_for::<ByteArray>(storage, side_length, moves),
        StorageKind::Nibble => run_move_benchmark_for::<NibbleArray>(storage, side_length, moves),
        StorageKind::PackedThreeBits => {
            run_move_benchmark_for::<Packed3Array>(storage, side_length, moves)
        }
    }
}

fn run_move_benchmark_for<S: rubik::FaceletArray>(
    storage: StorageKind,
    side_length: usize,
    moves: &[Move],
) -> MoveResult {
    let mut cube = Cube::<S>::new_solved(side_length);
    let face_storage_bytes = face_storage_bytes(storage, side_length);

    let start = Instant::now();
    for mv in moves.iter().copied() {
        cube.apply_move_untracked(black_box(mv));
    }
    let elapsed = start.elapsed();

    black_box(&cube);

    MoveResult {
        side_length,
        move_count: moves.len(),
        face_storage_bytes,
        elapsed,
    }
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
            0 => Angle::Positive,
            1 => Angle::Negative,
            _ => Angle::Double,
        };
        moves.push(Move::new(axis, depth, angle));
    }

    moves
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

    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.2} gibibytes", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.2} mebibytes", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.2} kibibytes", bytes / KIB)
    } else {
        format!("{bytes:.0} bytes")
    }
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn moves_per_second(moves: usize, duration: Duration) -> f64 {
    moves as f64 / duration.as_secs_f64()
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
        StorageKind::PackedThreeBits => cells_per_face
            .checked_mul(3)
            .expect("packed three bit count overflowed usize")
            .div_ceil(64)
            .checked_mul(8)
            .and_then(|bytes_per_face| bytes_per_face.checked_mul(6))
            .expect("packed three storage size overflowed usize"),
    }
}
