use std::{
    env, fmt, process,
    time::{Duration, Instant},
};

use rubik::{
    Axis, Byte, CenterReductionStage, CornerReductionStage, Cube, EdgePairingStage, ExecutionMode,
    FaceletArray, Move, MoveAngle, Nibble, RandomSource, SolveAlgorithm, SolveContext, SolveError,
    SolveOptions, SolvePhase, ThirdByte, ThreeBit, XorShift64,
};

const DEFAULT_SIDE_LENGTH: usize = 5;
const DEFAULT_SCRAMBLE_ROUNDS: usize = rubik::DEFAULT_SCRAMBLE_ROUNDS;
const DEFAULT_RANDOM_SEED: u64 = 42;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Command {
    Help,
    Run(Cli),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct Cli {
    side_length: usize,
    mode: ExecutionMode,
    backend: StorageKind,
    scramble_rounds: usize,
    seed: u64,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            side_length: DEFAULT_SIDE_LENGTH,
            mode: ExecutionMode::Standard,
            backend: StorageKind::Byte,
            scramble_rounds: DEFAULT_SCRAMBLE_ROUNDS,
            seed: DEFAULT_RANDOM_SEED,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StorageKind {
    Byte,
    Nibble,
    ThreeBit,
    ThirdByte,
}

impl StorageKind {
    fn file_name(self) -> &'static str {
        match self {
            Self::Byte => "byte",
            Self::Nibble => "nibble",
            Self::ThreeBit => "three_bit",
            Self::ThirdByte => "third_byte",
        }
    }

    fn type_name(self) -> &'static str {
        match self {
            Self::Byte => "Byte",
            Self::Nibble => "Nibble",
            Self::ThreeBit => "ThreeBit",
            Self::ThirdByte => "ThirdByte",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "byte" | "Byte" => Ok(Self::Byte),
            "nibble" | "Nibble" => Ok(Self::Nibble),
            "three_bit" | "threebit" | "ThreeBit" | "3bit" => Ok(Self::ThreeBit),
            "third_byte" | "ThirdByte" => Ok(Self::ThirdByte),
            _ => Err(format!(
                "unknown backend: {value} (expected one of: byte, nibble, three_bit, third_byte)",
            )),
        }
    }
}

impl fmt::Display for StorageKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.file_name(), self.type_name())
    }
}

#[derive(Copy, Clone, Debug)]
struct StageRun {
    phase: SolvePhase,
    name: &'static str,
    step_count: usize,
    elapsed: Duration,
    moves: usize,
}

fn main() {
    match parse_args(env::args()) {
        Ok(Command::Help) => {
            print!("{}", usage());
        }
        Ok(Command::Run(cli)) => {
            if let Err(error) = run(cli) {
                eprintln!("{error}");
                process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("{error}\n\n{}", usage());
            process::exit(2);
        }
    }
}

fn parse_args<I, S>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut cli = Cli::default();
    let mut iter = args.into_iter().map(Into::into);
    let _program = iter.next();

    while let Some(arg) = iter.next() {
        let (flag, inline_value) = if let Some((flag, value)) = arg.split_once('=') {
            (flag.to_owned(), Some(value.to_owned()))
        } else {
            (arg, None)
        };

        match flag.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-n" | "--n" => {
                let value = argument_value(&flag, inline_value, &mut iter)?;
                cli.side_length = parse_usize(&value, "n")?;
            }
            "-m" | "--mode" => {
                let value = argument_value(&flag, inline_value, &mut iter)?;
                cli.mode = parse_mode(&value)?;
            }
            "-b" | "--backend" => {
                let value = argument_value(&flag, inline_value, &mut iter)?;
                cli.backend = StorageKind::parse(&value)?;
            }
            "-r" | "--scramble-rounds" => {
                let value = argument_value(&flag, inline_value, &mut iter)?;
                cli.scramble_rounds = parse_usize(&value, "scramble rounds")?;
            }
            "-s" | "--seed" => {
                let value = argument_value(&flag, inline_value, &mut iter)?;
                cli.seed = parse_u64(&value, "seed")?;
            }
            _ if flag.starts_with('-') => return Err(format!("unknown argument: {flag}")),
            _ => return Err(format!("unexpected positional argument: {flag}")),
        }
    }

    if cli.side_length == 0 {
        return Err("n must be greater than 0".to_owned());
    }

    Ok(Command::Run(cli))
}

fn argument_value<I>(
    flag: &str,
    inline_value: Option<String>,
    iter: &mut I,
) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    inline_value
        .or_else(|| iter.next())
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_mode(value: &str) -> Result<ExecutionMode, String> {
    match value {
        "standard" => Ok(ExecutionMode::Standard),
        "optimized" | "optimised" => Ok(ExecutionMode::Optimized),
        _ => Err(format!(
            "unknown mode: {value} (expected one of: standard, optimized)",
        )),
    }
}

fn parse_usize(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))
}

fn parse_u64(value: &str, name: &str) -> Result<u64, String> {
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        return u64::from_str_radix(hex, 16)
            .map_err(|_| format!("{name} must be a valid u64 integer"));
    }

    value
        .parse::<u64>()
        .map_err(|_| format!("{name} must be a valid u64 integer"))
}

fn usage() -> String {
    format!(
        "\
Usage: cargo run --release --bin run_pipeline_no_render -- [options]

Options:
  -n, --n <N>                        Cube side length. Default: {DEFAULT_SIDE_LENGTH}
  -m, --mode <MODE>                 standard | optimized. Default: standard
  -b, --backend <BACKEND>           byte | nibble | three_bit | third_byte. Default: byte
  -r, --scramble-rounds <ROUNDS>    Uniform random layer rounds; each round is 3*n moves. Default: {DEFAULT_SCRAMBLE_ROUNDS}
  -s, --seed <SEED>                 Scramble seed, decimal or 0x-prefixed hex.
  -h, --help                        Print this help.
"
    )
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.backend {
        StorageKind::Byte => run_with_storage::<Byte>(cli),
        StorageKind::Nibble => run_with_storage::<Nibble>(cli),
        StorageKind::ThreeBit => run_with_storage::<ThreeBit>(cli),
        StorageKind::ThirdByte => run_with_storage::<ThirdByte>(cli),
    }
}

fn run_with_storage<S: FaceletArray + 'static>(cli: Cli) -> Result<(), String> {
    let estimated_storage = estimated_storage_bytes::<S>(cli.side_length)?;
    let scramble_moves = generate_scramble_moves(cli.side_length, cli.scramble_rounds, cli.seed)?;

    println!("Starting Rubik Pipeline");
    print_value("  ", "N", cli.side_length);
    print_value("  ", "Mode", title_case(&cli.mode.to_string()));
    print_value("  ", "Backend", title_case(&cli.backend.to_string()));
    print_value("  ", "Scramble Rounds", cli.scramble_rounds);
    print_value("  ", "Scramble Seed", format_args!("0x{:016X}", cli.seed));
    print_value("  ", "Planned Scramble Moves", scramble_moves.len());
    print_value(
        "  ",
        "Estimated Facelet Storage",
        format_bytes(estimated_storage),
    );
    println!();

    let init_start = Instant::now();
    let mut cube = Cube::<S>::new_solved(cli.side_length);
    let init_elapsed = init_start.elapsed();

    println!("Finished Initialization");
    print_value_with_unit(
        "  ",
        "Time",
        format_args!("{:.3}", milliseconds(init_elapsed)),
        "ms",
    );
    print_value(
        "  ",
        "Facelet Storage",
        format_bytes(cube.estimated_storage_bytes()),
    );
    println!();

    let scramble_start = Instant::now();
    cube.apply_moves_untracked(scramble_moves.iter().copied());
    let scramble_elapsed = scramble_start.elapsed();

    println!("Finished Scramble");
    print_move_stats(scramble_elapsed, scramble_moves.len(), "  ");
    println!();

    let mut context = SolveContext::new(SolveOptions::new(cli.mode));
    let solve_start = Instant::now();
    let mut stages_completed = 0usize;

    let center = run_stage(&mut cube, &mut context, || {
        CenterReductionStage::western_default()
    })
    .map_err(format_solve_error)?;
    print_stage(center);
    stages_completed += 1;

    let corner = run_stage(&mut cube, &mut context, CornerReductionStage::default)
        .map_err(format_solve_error)?;
    print_stage(corner);
    stages_completed += 1;

    let edge = run_stage(&mut cube, &mut context, EdgePairingStage::default)
        .map_err(format_solve_error)?;
    print_stage(edge);
    stages_completed += 1;

    let solve_elapsed = solve_start.elapsed();
    println!();

    let total_moves = context.move_stats().total;
    println!("Finished Rubik Pipeline");
    print_move_stats(solve_elapsed, total_moves, "  ");
    print_value("  ", "Recorded Solution Moves", context.moves().len());
    print_value("  ", "Solved", yes_no(cube.is_solved()));
    print_value("  ", "Stages Completed", stages_completed);

    Ok(())
}

fn run_stage<S, A, F>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    build_stage: F,
) -> Result<StageRun, SolveError>
where
    S: FaceletArray,
    A: SolveAlgorithm<S>,
    F: FnOnce() -> A,
{
    let stage_start = Instant::now();
    let mut stage = build_stage();

    if !stage
        .execution_mode_support()
        .supports(context.execution_mode())
    {
        return Err(SolveError::StageFailed {
            stage: stage.name(),
            reason: "stage does not support the requested execution mode",
        });
    }

    if !stage.is_applicable_to_side_length(cube.side_len()) {
        return Err(SolveError::UnsupportedCube {
            reason: "selected stage does not support this cube size",
        });
    }

    println!("Starting {}", title_case(stage.name()));

    let moves_before = context.move_stats().total;
    stage.run(cube, context)?;
    let moves_after = context.move_stats().total;

    Ok(StageRun {
        phase: stage.phase(),
        name: stage.name(),
        step_count: stage.steps().len(),
        elapsed: stage_start.elapsed(),
        moves: moves_after - moves_before,
    })
}

fn print_stage(stage: StageRun) {
    println!("Finished {}", title_case(stage.name));
    print_value("  ", "Phase", title_case(&stage.phase.to_string()));
    print_value("  ", "Steps", stage.step_count);
    print_move_stats(stage.elapsed, stage.moves, "  ");
    println!();
}

fn print_move_stats(duration: Duration, moves: usize, indent: &str) {
    print_value_with_unit(
        indent,
        "Time",
        format_args!("{:.3}", milliseconds(duration)),
        "ms",
    );
    print_value(indent, "Moves", moves);
    print_value_with_unit(indent, "Rate", format_rate(moves, duration), "mv/s");
}

fn estimated_storage_bytes<S: FaceletArray>(side_length: usize) -> Result<usize, String> {
    let cells_per_face = side_length
        .checked_mul(side_length)
        .ok_or_else(|| "n is too large to estimate storage safely".to_owned())?;

    S::storage_bytes_for_len(cells_per_face)
        .checked_mul(6)
        .ok_or_else(|| "n is too large to estimate storage safely".to_owned())
}

fn generate_scramble_moves(
    side_length: usize,
    rounds: usize,
    seed: u64,
) -> Result<Vec<Move>, String> {
    let per_round = side_length
        .checked_mul(3)
        .ok_or_else(|| "scramble plan would overflow usize".to_owned())?;
    let capacity = rounds
        .checked_mul(per_round)
        .ok_or_else(|| "scramble plan would overflow usize".to_owned())?;

    let mut rng = XorShift64::new(seed);
    let mut moves = Vec::with_capacity(capacity);

    for _ in 0..rounds {
        for _ in 0..per_round {
            moves.push(random_move(side_length, &mut rng));
        }
    }

    Ok(moves)
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

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn format_rate(items: usize, duration: Duration) -> String {
    if items == 0 {
        return "0".to_owned();
    }

    let seconds = duration.as_secs_f64();
    if seconds == 0.0 {
        return "inf".to_owned();
    }

    format!("{:.0}", items as f64 / seconds)
}

fn format_bytes(bytes: usize) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if bytes < 1024 {
        return format!("{bytes}B");
    }

    let mut unit_index = 0usize;
    let mut value = bytes as f64;
    while value >= 1024.0 && unit_index + 1 < UNITS.len() {
        value /= 1024.0;
        unit_index += 1;
    }

    format!("{value:.2}{}", UNITS[unit_index])
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "Yes"
    } else {
        "No"
    }
}

fn print_value(indent: &str, label: &str, value: impl fmt::Display) {
    println!("{indent}{label}: {value}");
}

fn print_value_with_unit(indent: &str, label: &str, value: impl fmt::Display, unit: &str) {
    println!("{indent}{label}: {value}{unit}");
}

fn format_solve_error(error: SolveError) -> String {
    match error {
        SolveError::UnsupportedCube { reason } => {
            format!("Unsupported Cube: {reason}")
        }
        SolveError::StageFailed { stage, reason } => {
            format!("Stage {} Failed: {reason}", title_case(stage))
        }
    }
}

fn title_case(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut capitalize_next = true;

    for ch in value.chars() {
        match ch {
            '_' => {
                output.push(' ');
                capitalize_next = true;
            }
            ' ' | '-' => {
                output.push(ch);
                capitalize_next = true;
            }
            _ if capitalize_next => {
                output.extend(ch.to_uppercase());
                capitalize_next = false;
            }
            _ => output.push(ch),
        }
    }

    output
}
