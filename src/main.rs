use std::{
    env, fmt, process,
    time::{Duration, Instant},
};

use rubik::{
    conventions::face_outer_move, Axis, Byte, Byte3, CenterReductionStage, CornerReductionStage,
    Cube, EdgePairingStage, ExecutionMode, FaceId, FaceletArray, Move, MoveAngle, Nibble,
    RandomSource, SolveAlgorithm, SolveContext, SolveError, SolveOptions, SolvePhase, ThreeBit,
    XorShift64,
};

const DEFAULT_SIDE_LENGTH: usize = 5;
const DEFAULT_SCRAMBLE_ROUNDS: usize = 8;
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
    Byte3,
}

impl StorageKind {
    fn file_name(self) -> &'static str {
        match self {
            Self::Byte => "byte",
            Self::Nibble => "nibble",
            Self::ThreeBit => "three_bit",
            Self::Byte3 => "byte3",
        }
    }

    fn type_name(self) -> &'static str {
        match self {
            Self::Byte => "Byte",
            Self::Nibble => "Nibble",
            Self::ThreeBit => "ThreeBit",
            Self::Byte3 => "Byte3",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "byte" | "Byte" => Ok(Self::Byte),
            "nibble" | "Nibble" => Ok(Self::Nibble),
            "three_bit" | "threebit" | "ThreeBit" | "3bit" => Ok(Self::ThreeBit),
            "byte3" | "Byte3" => Ok(Self::Byte3),
            _ => Err(format!(
                "unknown backend: {value} (expected one of: byte, nibble, three_bit, byte3)",
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
    note: Option<&'static str>,
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
Usage: cargo run -- [options]

Options:
  -n, --n <N>                        Cube side length. Default: {DEFAULT_SIDE_LENGTH}
  -m, --mode <MODE>                 standard | optimized. Default: standard
  -b, --backend <BACKEND>           byte | nibble | three_bit | byte3. Default: byte
  -r, --scramble-rounds <ROUNDS>    Scramble rounds. Default: {DEFAULT_SCRAMBLE_ROUNDS}
  -s, --seed <SEED>                 Scramble seed, decimal or 0x-prefixed hex.
  -h, --help                        Print this help.

Examples:
  cargo run -- --n 5 --mode optimized --backend byte3
  cargo run -- -n 7 -m standard -b ThreeBit --seed 0xC0FFEE
"
    )
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.backend {
        StorageKind::Byte => run_with_storage::<Byte>(cli),
        StorageKind::Nibble => run_with_storage::<Nibble>(cli),
        StorageKind::ThreeBit => run_with_storage::<ThreeBit>(cli),
        StorageKind::Byte3 => run_with_storage::<Byte3>(cli),
    }
}

fn run_with_storage<S: FaceletArray>(cli: Cli) -> Result<(), String> {
    let estimated_storage = estimated_storage_bytes::<S>(cli.side_length)?;
    let scramble_moves = generate_scramble_moves(cli.side_length, cli.scramble_rounds, cli.seed)?;

    println!("rubik solve run");
    println!("  n={}", cli.side_length);
    println!("  mode={}", cli.mode);
    println!("  backend={}", cli.backend);
    println!("  scramble_rounds={}", cli.scramble_rounds);
    println!("  scramble_seed=0x{:016X}", cli.seed);
    println!("  planned_scramble_moves={}", scramble_moves.len());
    println!(
        "  estimated_facelet_storage={}",
        format_bytes(estimated_storage)
    );
    println!();

    let init_start = Instant::now();
    let mut cube = Cube::<S>::new_solved(cli.side_length);
    let init_elapsed = init_start.elapsed();

    println!("initialization");
    println!("  elapsed={:.3} ms", milliseconds(init_elapsed));
    println!(
        "  estimated_facelet_storage={}",
        format_bytes(cube.estimated_storage_bytes())
    );
    println!();

    let scramble_start = Instant::now();
    cube.apply_moves_untracked(scramble_moves.iter().copied());
    let scramble_elapsed = scramble_start.elapsed();

    println!("scramble");
    println!("  elapsed={:.3} ms", milliseconds(scramble_elapsed));
    println!("  moves={}", scramble_moves.len());
    println!(
        "  moves_per_second={}",
        format_rate(scramble_moves.len(), scramble_elapsed)
    );
    println!("  render after scramble:");
    print!("{}", cube.net_string());
    println!();

    let mut context = SolveContext::new(SolveOptions::new(cli.mode));
    let solve_start = Instant::now();
    let mut stages_completed = 0usize;

    println!("solve stages");

    let center = run_stage(
        &mut cube,
        &mut context,
        || CenterReductionStage::western_default(),
        None,
    )
    .map_err(|error| stage_failure_message(&cube, error))?;
    print_stage_with_render(center, &cube);
    stages_completed += 1;

    let corner = run_stage(&mut cube, &mut context, CornerReductionStage::default, None)
        .map_err(|error| stage_failure_message(&cube, error))?;
    print_stage_with_render(corner, &cube);
    stages_completed += 1;

    let edge = run_stage(&mut cube, &mut context, EdgePairingStage::default, None)
        .map_err(|error| stage_failure_message(&cube, error))?;
    print_stage_with_render(edge, &cube);
    stages_completed += 1;

    let solve_elapsed = solve_start.elapsed();
    println!();

    let total_moves = context.move_stats().total;
    println!("overall solve");
    println!("  elapsed={:.3} ms", milliseconds(solve_elapsed));
    println!("  moves={total_moves}");
    println!(
        "  moves_per_second={}",
        format_rate(total_moves, solve_elapsed)
    );
    println!("  recorded_solution_moves={}", context.moves().len());
    println!("  solved={}", yes_no(cube.is_solved()));
    println!("  stages_completed={stages_completed}");

    Ok(())
}

fn run_stage<S, A, F>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    build_stage: F,
    note: Option<&'static str>,
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

    let moves_before = context.move_stats().total;
    stage.run(cube, context)?;
    let moves_after = context.move_stats().total;

    Ok(StageRun {
        phase: stage.phase(),
        name: stage.name(),
        step_count: stage.steps().len(),
        elapsed: stage_start.elapsed(),
        moves: moves_after - moves_before,
        note,
    })
}

fn print_stage(stage: StageRun) {
    println!(
        "  {} [{} | steps={}]: {:.3} ms, {} moves, {} mv/s",
        stage.name,
        stage.phase,
        stage.step_count,
        milliseconds(stage.elapsed),
        stage.moves,
        format_rate(stage.moves, stage.elapsed),
    );

    if let Some(note) = stage.note {
        println!("    note: {note}");
    }
}

fn print_stage_with_render<S: FaceletArray>(stage: StageRun, cube: &Cube<S>) {
    print_stage(stage);
    println!("  render after {}:", stage.name);
    print!("{}", cube.net_string());
    println!();
}

fn stage_failure_message<S: FaceletArray>(cube: &Cube<S>, error: SolveError) -> String {
    format!("{error}\n\npartial cube state:\n{}", cube.net_string())
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
        .checked_add(FaceId::ALL.len())
        .ok_or_else(|| "scramble plan would overflow usize".to_owned())?;
    let capacity = rounds
        .checked_mul(per_round)
        .ok_or_else(|| "scramble plan would overflow usize".to_owned())?;

    let mut rng = XorShift64::new(seed);
    let mut moves = Vec::with_capacity(capacity);

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
        return format!("{bytes} B");
    }

    let mut unit_index = 0usize;
    let mut value = bytes as f64;
    while value >= 1024.0 && unit_index + 1 < UNITS.len() {
        value /= 1024.0;
        unit_index += 1;
    }

    format!("{value:.2} {}", UNITS[unit_index])
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_defaults_when_no_options_are_provided() {
        assert_eq!(parse_args(["rubik"]).unwrap(), Command::Run(Cli::default()));
    }

    #[test]
    fn parse_help_flag() {
        assert_eq!(parse_args(["rubik", "--help"]).unwrap(), Command::Help);
    }

    #[test]
    fn parse_explicit_cli_values_and_aliases() {
        assert_eq!(
            parse_args([
                "rubik",
                "--n=7",
                "--mode",
                "optimised",
                "--backend",
                "ThreeBit",
                "--scramble-rounds",
                "9",
                "--seed",
                "0xC0FFEE",
            ])
            .unwrap(),
            Command::Run(Cli {
                side_length: 7,
                mode: ExecutionMode::Optimized,
                backend: StorageKind::ThreeBit,
                scramble_rounds: 9,
                seed: 0xC0FFEE,
            })
        );
    }

    #[test]
    fn reject_zero_side_length() {
        assert_eq!(
            parse_args(["rubik", "--n", "0"]).unwrap_err(),
            "n must be greater than 0"
        );
    }

    #[test]
    fn reject_unknown_backend() {
        assert_eq!(
            parse_args(["rubik", "--backend", "packed"]).unwrap_err(),
            "unknown backend: packed (expected one of: byte, nibble, three_bit, byte3)"
        );
    }
}
