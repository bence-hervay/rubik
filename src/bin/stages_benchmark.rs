#[path = "benchmark_common/mod.rs"]
mod benchmark_common;

use std::{env, path::PathBuf, process};

use benchmark_common::{
    average_runs, default_csv_output_path, default_sizes, ensure_runner, format_ms, parse_backends,
    parse_non_negative_u64, parse_non_negative_usize, parse_positive_usize, parse_sizes,
    print_table, render_stages_csv, render_stages_svg, run_pipeline, write_csv, write_svg,
    RunConfig, Stage,
};

const DEFAULT_BACKEND: &str = "byte";
const DEFAULT_MODE: &str = "optimized";
const DEFAULT_ATTEMPTS: usize = 3;
const DEFAULT_FIT_THRESHOLD: usize = 512;
const DEFAULT_EXTRAPOLATE_TO: usize = 1 << 18;
const DEFAULT_SCRAMBLE_ROUNDS: usize = 8;
const DEFAULT_SEED: u64 = 42;
const DEFAULT_OUTPUT: &str = "stages.svg";
const DEFAULT_RUNNER: &str = "target/release/run_pipeline_no_render";

#[derive(Debug)]
struct Cli {
    backend: String,
    mode: String,
    attempts: usize,
    sizes: Vec<usize>,
    fit_threshold: usize,
    extrapolate_to: usize,
    scramble_rounds: usize,
    seed: u64,
    output: PathBuf,
    csv_output: Option<PathBuf>,
    runner: String,
    build: bool,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            backend: DEFAULT_BACKEND.to_owned(),
            mode: DEFAULT_MODE.to_owned(),
            attempts: DEFAULT_ATTEMPTS,
            sizes: default_sizes(),
            fit_threshold: DEFAULT_FIT_THRESHOLD,
            extrapolate_to: DEFAULT_EXTRAPOLATE_TO,
            scramble_rounds: DEFAULT_SCRAMBLE_ROUNDS,
            seed: DEFAULT_SEED,
            output: PathBuf::from(DEFAULT_OUTPUT),
            csv_output: None,
            runner: DEFAULT_RUNNER.to_owned(),
            build: true,
        }
    }
}

fn main() {
    match parse_args(env::args().skip(1)) {
        Ok(Command::Help) => print!("{}", usage()),
        Ok(Command::Run(cli)) => {
            if let Err(error) = run(cli) {
                eprintln!("error: {error}");
                process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("error: {error}\n\n{}", usage());
            process::exit(2);
        }
    }
}

enum Command {
    Help,
    Run(Cli),
}

fn parse_args<I>(args: I) -> Result<Command, String>
where
    I: IntoIterator<Item = String>,
{
    let mut cli = Cli::default();
    let mut iter = args.into_iter();

    while let Some(arg) = iter.next() {
        let (flag, inline_value) = split_arg(&arg);
        match flag {
            "-h" | "--help" => return Ok(Command::Help),
            "--backend" => cli.backend = value(flag, inline_value, &mut iter)?,
            "--mode" => cli.mode = parse_mode(&value(flag, inline_value, &mut iter)?)?,
            "--attempts" => {
                cli.attempts =
                    parse_positive_usize(&value(flag, inline_value, &mut iter)?, "attempts")?;
            }
            "--sizes" => cli.sizes = parse_sizes(&value(flag, inline_value, &mut iter)?)?,
            "--fit-threshold" => {
                cli.fit_threshold =
                    parse_positive_usize(&value(flag, inline_value, &mut iter)?, "fit threshold")?;
            }
            "--extrapolate-to" => {
                cli.extrapolate_to =
                    parse_positive_usize(&value(flag, inline_value, &mut iter)?, "extrapolate-to")?;
            }
            "--scramble-rounds" => {
                cli.scramble_rounds = parse_non_negative_usize(
                    &value(flag, inline_value, &mut iter)?,
                    "scramble rounds",
                )?;
            }
            "--seed" => {
                cli.seed = parse_non_negative_u64(&value(flag, inline_value, &mut iter)?, "seed")?;
            }
            "--output" => cli.output = PathBuf::from(value(flag, inline_value, &mut iter)?),
            "--csv-output" => {
                cli.csv_output = Some(PathBuf::from(value(flag, inline_value, &mut iter)?));
            }
            "--runner" => cli.runner = value(flag, inline_value, &mut iter)?,
            "--no-build" => {
                reject_inline_value(flag, inline_value)?;
                cli.build = false;
            }
            _ if flag.starts_with('-') => return Err(format!("unknown argument: {flag}")),
            _ => return Err(format!("unexpected positional argument: {flag}")),
        }
    }

    let backends = parse_backends(&cli.backend)?;
    if backends.len() != 1 {
        return Err("--backend expects exactly one backend name".to_owned());
    }
    cli.backend = backends[0].clone();

    if cli.extrapolate_to < *cli.sizes.iter().max().expect("default sizes are non-empty") {
        return Err("--extrapolate-to must be at least the largest measured size".to_owned());
    }

    Ok(Command::Run(cli))
}

fn run(cli: Cli) -> Result<(), String> {
    let runner = ensure_runner(&cli.runner, cli.build)?;
    let mut by_size = Vec::new();

    for size in &cli.sizes {
        let mut runs = Vec::new();
        for attempt in 0..cli.attempts {
            let seed = cli.seed + attempt as u64;
            println!(
                "running n={} backend={} mode={} attempt={}/{} seed={}",
                size,
                cli.backend,
                cli.mode,
                attempt + 1,
                cli.attempts,
                seed
            );
            runs.push(run_pipeline(RunConfig {
                runner: &runner,
                size: *size,
                mode: &cli.mode,
                backend: &cli.backend,
                scramble_rounds: cli.scramble_rounds,
                seed,
            })?);
        }
        by_size.push((*size, average_runs(&runs)?));
    }

    let mut headers = vec!["n"];
    headers.extend(Stage::PLOTTED.iter().map(|stage| stage.name()));
    let rows: Vec<Vec<String>> = by_size
        .iter()
        .map(|(size, times)| {
            let mut row = vec![size.to_string()];
            row.extend(
                Stage::PLOTTED
                    .iter()
                    .map(|stage| format_ms(times.get(*stage))),
            );
            row
        })
        .collect();
    println!();
    print_table(&headers, &rows);

    let svg = render_stages_svg(
        &by_size,
        &cli.backend,
        &cli.mode,
        cli.attempts,
        cli.fit_threshold,
        cli.extrapolate_to,
    );
    write_svg(&cli.output, &svg)?;

    let csv_output = cli
        .csv_output
        .clone()
        .unwrap_or_else(|| default_csv_output_path(&cli.output));
    let csv = render_stages_csv(
        &by_size,
        &cli.backend,
        &cli.mode,
        cli.attempts,
        cli.scramble_rounds,
        cli.seed,
        cli.fit_threshold,
        cli.extrapolate_to,
    );
    write_csv(&csv_output, &csv)?;

    println!("\nwrote {}", cli.output.display());
    println!("wrote {}", csv_output.display());

    Ok(())
}

fn usage() -> String {
    format!(
        "\
Usage: cargo run --release --bin stages_benchmark -- [options]

Options:
  --backend <BACKEND>           byte | nibble | three_bit | third_byte. Default: {DEFAULT_BACKEND}
  --mode <MODE>                 standard | optimized. Default: {DEFAULT_MODE}
  --attempts <N>                Runs per size. Default: {DEFAULT_ATTEMPTS}
  --sizes <LIST>                Comma- or space-separated side lengths. Default: powers of two from 1 to 2048
  --fit-threshold <N>           Fit only measured sizes >= N. Default: {DEFAULT_FIT_THRESHOLD}
  --extrapolate-to <N>          Draw fitted extrapolation through this side length. Default: {DEFAULT_EXTRAPOLATE_TO}
  --scramble-rounds <N>         Scramble rounds passed to run_pipeline_no_render. Default: {DEFAULT_SCRAMBLE_ROUNDS}
  --seed <N>                    Base scramble seed. Attempt i uses seed+i. Default: {DEFAULT_SEED}
  --output <PATH>               SVG output path. Default: {DEFAULT_OUTPUT}
  --csv-output <PATH>           CSV output path. Default: SVG output with .csv extension
  --runner <PATH>               Path to run_pipeline_no_render. Default: {DEFAULT_RUNNER}
  --no-build                    Do not build the release runner before benchmarking
  -h, --help                    Print this help.
"
    )
}

fn split_arg(arg: &str) -> (&str, Option<String>) {
    match arg.split_once('=') {
        Some((flag, value)) => (flag, Some(value.to_owned())),
        None => (arg, None),
    }
}

fn value<I>(flag: &str, inline_value: Option<String>, iter: &mut I) -> Result<String, String>
where
    I: Iterator<Item = String>,
{
    inline_value
        .or_else(|| iter.next())
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn reject_inline_value(flag: &str, inline_value: Option<String>) -> Result<(), String> {
    if inline_value.is_some() {
        Err(format!("{flag} does not take a value"))
    } else {
        Ok(())
    }
}

fn parse_mode(value: &str) -> Result<String, String> {
    match value {
        "standard" | "optimized" => Ok(value.to_owned()),
        _ => Err(format!(
            "unknown mode: {value} (expected one of: standard, optimized)"
        )),
    }
}
