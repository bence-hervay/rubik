#[path = "benchmark_common/mod.rs"]
mod benchmark_common;

use std::{env, path::PathBuf, process};

use rubik::{default_thread_count, optimized_thread_count};

use benchmark_common::{
    average_runs, default_csv_output_path, ensure_runner, format_ms, parse_backends,
    parse_non_negative_u64, parse_non_negative_usize, parse_positive_usize, print_table,
    render_backends_csv, render_backends_svg, run_pipeline, write_csv, write_svg, RunConfig, Stage,
    BACKENDS,
};

const DEFAULT_SIZE: usize = 2048;
const DEFAULT_MODE: &str = "optimized";
const DEFAULT_TRIALS: usize = 5;
const DEFAULT_SCRAMBLE_ROUNDS: usize = rubik::DEFAULT_SCRAMBLE_ROUNDS;
const DEFAULT_SEED: u64 = 42;
const DEFAULT_OUTPUT: &str = "backends.svg";
const DEFAULT_RUNNER: &str = "target/release/run_pipeline_no_render";

#[derive(Debug)]
struct Cli {
    size: usize,
    backends: Vec<String>,
    mode: String,
    trials: usize,
    scramble_rounds: usize,
    seed: u64,
    thread_count: usize,
    output: PathBuf,
    csv_output: Option<PathBuf>,
    runner: String,
    build: bool,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            size: DEFAULT_SIZE,
            backends: BACKENDS.map(str::to_owned).to_vec(),
            mode: DEFAULT_MODE.to_owned(),
            trials: DEFAULT_TRIALS,
            scramble_rounds: DEFAULT_SCRAMBLE_ROUNDS,
            seed: DEFAULT_SEED,
            thread_count: optimized_thread_count(),
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
            "-n" | "--size" => {
                cli.size = parse_positive_usize(&value(flag, inline_value, &mut iter)?, "size")?;
            }
            "--backends" => cli.backends = parse_backends(&value(flag, inline_value, &mut iter)?)?,
            "--mode" => cli.mode = parse_mode(&value(flag, inline_value, &mut iter)?)?,
            "--trials" => {
                cli.trials =
                    parse_positive_usize(&value(flag, inline_value, &mut iter)?, "trials")?;
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
            "-t" | "--threads" | "--thread-count" => {
                cli.thread_count =
                    parse_positive_usize(&value(flag, inline_value, &mut iter)?, "threads")?;
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

    Ok(Command::Run(cli))
}

fn run(cli: Cli) -> Result<(), String> {
    let runner = ensure_runner(&cli.runner, cli.build)?;
    let mut by_backend = Vec::new();

    for backend in &cli.backends {
        let mut runs = Vec::new();
        for trial in 0..cli.trials {
            let seed = cli.seed + trial as u64;
            println!(
                "running n={} backend={} mode={} trial={}/{} seed={} threads={}",
                cli.size,
                backend,
                cli.mode,
                trial + 1,
                cli.trials,
                seed,
                cli.thread_count
            );
            runs.push(run_pipeline(RunConfig {
                runner: &runner,
                size: cli.size,
                mode: &cli.mode,
                backend,
                scramble_rounds: cli.scramble_rounds,
                seed,
                thread_count: cli.thread_count,
            })?);
        }
        by_backend.push((backend.clone(), average_runs(&runs)?));
    }

    let mut headers = vec!["backend"];
    headers.extend(Stage::PLOTTED.iter().map(|stage| stage.name()));
    let rows: Vec<Vec<String>> = by_backend
        .iter()
        .map(|(backend, times)| {
            let mut row = vec![backend.clone()];
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

    let svg = render_backends_svg(
        &by_backend,
        cli.size,
        &cli.mode,
        cli.trials,
        cli.thread_count,
    );
    write_svg(&cli.output, &svg)?;

    let csv_output = cli
        .csv_output
        .clone()
        .unwrap_or_else(|| default_csv_output_path(&cli.output));
    let csv = render_backends_csv(
        &by_backend,
        cli.size,
        &cli.mode,
        cli.trials,
        cli.scramble_rounds,
        cli.seed,
        cli.thread_count,
    );
    write_csv(&csv_output, &csv)?;

    println!("\nwrote {}", cli.output.display());
    println!("wrote {}", csv_output.display());

    Ok(())
}

fn usage() -> String {
    format!(
        "\
Usage: cargo run --release --bin backends_benchmark -- [options]

Options:
  -n, --size <N>                 Cube side length. Default: {DEFAULT_SIZE}
  --backends <LIST>              Comma- or space-separated backend names. Default: {}
  --mode <MODE>                  standard | optimized. Default: {DEFAULT_MODE}
  --trials <N>                   Runs per backend. Default: {DEFAULT_TRIALS}
  --scramble-rounds <N>          Uniform random layer rounds passed to run_pipeline_no_render. Default: {DEFAULT_SCRAMBLE_ROUNDS}
  --seed <N>                     Base scramble seed. Trial i uses seed+i. Default: {DEFAULT_SEED}
  -t, --threads <N>              Optimized worker threads passed to run_pipeline_no_render. Default: available CPUs ({})
  --output <PATH>                SVG output path. Default: {DEFAULT_OUTPUT}
  --csv-output <PATH>            CSV output path. Default: SVG output with .csv extension
  --runner <PATH>                Path to run_pipeline_no_render. Default: {DEFAULT_RUNNER}
  --no-build                     Do not build the release runner before benchmarking
  -h, --help                     Print this help.
",
        BACKENDS.join(","),
        default_thread_count()
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
