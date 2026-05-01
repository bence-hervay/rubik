use std::{
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process,
    time::{Duration, Instant},
};

use rubik::{
    configure_optimized_thread_count, default_thread_count, optimized_thread_count, Byte, Cube,
    FaceId, Facelet, XorShift64,
};

const DEFAULT_SIDE_LENGTH: usize = 20;
const DEFAULT_MAX_K: usize = 32;
const DEFAULT_TRIALS: usize = 64;
const DEFAULT_SEED: u64 = 0x5C2A_4B1E_0000;
const DEFAULT_OUTPUT: &str = "benchmark/scramble_stats.svg";

#[derive(Clone, Debug)]
struct Cli {
    side_length: usize,
    max_k: usize,
    trials: usize,
    seed: u64,
    thread_count: usize,
    output: PathBuf,
    csv_output: Option<PathBuf>,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            side_length: DEFAULT_SIDE_LENGTH,
            max_k: DEFAULT_MAX_K,
            trials: DEFAULT_TRIALS,
            seed: DEFAULT_SEED,
            thread_count: optimized_thread_count(),
            output: PathBuf::from(DEFAULT_OUTPUT),
            csv_output: None,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Method {
    UniformRandomLayers,
    ParallelRandomLayerBatches,
    LayerSweeps,
}

impl Method {
    const ALL: [Self; 3] = [
        Self::UniformRandomLayers,
        Self::ParallelRandomLayerBatches,
        Self::LayerSweeps,
    ];

    const fn name(self) -> &'static str {
        match self {
            Self::UniformRandomLayers => "uniform_random_layers",
            Self::ParallelRandomLayerBatches => "parallel_random_layer_batches",
            Self::LayerSweeps => "layer_sweeps",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::UniformRandomLayers => "Uniform random layers",
            Self::ParallelRandomLayerBatches => "Parallel random layer batches",
            Self::LayerSweeps => "Layer sweeps",
        }
    }

    const fn color(self) -> &'static str {
        match self {
            Self::UniformRandomLayers => "#2563eb",
            Self::ParallelRandomLayerBatches => "#16a34a",
            Self::LayerSweeps => "#dc2626",
        }
    }

    const fn seed_salt(self) -> u64 {
        match self {
            Self::UniformRandomLayers => 0xB1A5_ED00_0000,
            Self::ParallelRandomLayerBatches => 0x9A7A_4B17_0000,
            Self::LayerSweeps => 0x5EED_5000_0000,
        }
    }
}

#[derive(Copy, Clone, Debug, Default)]
struct Stats {
    face_tv: f64,
    pair_tv: f64,
    elapsed: Duration,
}

#[derive(Copy, Clone, Debug)]
struct Row {
    k: usize,
    method: Method,
    stats: Stats,
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
            "-n" | "--n" => {
                cli.side_length = parse_positive_usize(&value(flag, inline_value, &mut iter)?, "n")?
            }
            "--max-k" => cli.max_k = parse_usize(&value(flag, inline_value, &mut iter)?, "max-k")?,
            "--trials" => {
                cli.trials = parse_positive_usize(&value(flag, inline_value, &mut iter)?, "trials")?
            }
            "--seed" => cli.seed = parse_u64(&value(flag, inline_value, &mut iter)?, "seed")?,
            "-t" | "--threads" | "--thread-count" => {
                cli.thread_count =
                    parse_positive_usize(&value(flag, inline_value, &mut iter)?, "threads")?
            }
            "--output" => cli.output = PathBuf::from(value(flag, inline_value, &mut iter)?),
            "--csv-output" => {
                cli.csv_output = Some(PathBuf::from(value(flag, inline_value, &mut iter)?))
            }
            _ if flag.starts_with('-') => return Err(format!("unknown argument: {flag}")),
            _ => return Err(format!("unexpected positional argument: {flag}")),
        }
    }

    Ok(Command::Run(cli))
}

fn run(cli: Cli) -> Result<(), String> {
    configure_optimized_thread_count(cli.thread_count)?;

    let rows = collect_rows(&cli);
    let csv_output = cli
        .csv_output
        .clone()
        .unwrap_or_else(|| cli.output.with_extension("csv"));

    write_text(&csv_output, &render_csv(&rows))?;
    write_text(&cli.output, &render_svg(&cli, &rows))?;

    print_summary(&cli, &rows);
    println!("wrote {}", cli.output.display());
    println!("wrote {}", csv_output.display());

    Ok(())
}

fn collect_rows(cli: &Cli) -> Vec<Row> {
    let mut rows = Vec::new();

    for k in 0..=cli.max_k {
        for method in Method::ALL {
            let mut stats = Stats::default();

            for trial in 0..cli.trials {
                let seed = cli.seed
                    ^ method.seed_salt()
                    ^ ((k as u64) << 24)
                    ^ trial as u64
                    ^ ((cli.side_length as u64) << 48);
                let mut cube = Cube::<Byte>::new_solved(cli.side_length);
                let mut rng = XorShift64::new(seed);

                let start = Instant::now();
                match method {
                    Method::UniformRandomLayers => cube.scramble_uniform_random_layers(&mut rng, k),
                    Method::ParallelRandomLayerBatches => {
                        cube.scramble_parallel_random_layer_batches_untracked(
                            &mut rng,
                            k,
                            cli.thread_count,
                        );
                    }
                    Method::LayerSweeps => cube.scramble_layer_sweeps(&mut rng, k),
                }
                stats.elapsed += start.elapsed();
                stats.face_tv += face_color_tv(&cube);
                stats.pair_tv += neighbor_pair_tv(&cube);
            }

            let trials = cli.trials as f64;
            stats.face_tv /= trials;
            stats.pair_tv /= trials;
            stats.elapsed /= cli.trials as u32;
            rows.push(Row { k, method, stats });
        }
    }

    rows
}

fn face_color_tv(cube: &Cube<Byte>) -> f64 {
    let n = cube.side_len();
    let mut counts = [[0usize; 6]; 6];

    for face in FaceId::ALL {
        for row in 0..n {
            for col in 0..n {
                let color = cube.face(face).get(row, col).as_u8() as usize;
                counts[face.index()][color] += 1;
            }
        }
    }

    let total = (6 * n * n) as f64;
    let target = 1.0 / 36.0;
    let distance = counts
        .into_iter()
        .flatten()
        .map(|count| ((count as f64 / total) - target).abs())
        .sum::<f64>();

    0.5 * distance
}

fn neighbor_pair_tv(cube: &Cube<Byte>) -> f64 {
    let n = cube.side_len();
    if n < 2 {
        return 0.0;
    }

    let mut counts = [0usize; 21];
    let mut total = 0usize;

    for face in FaceId::ALL {
        for row in 0..n {
            for col in 0..n {
                if col + 1 < n {
                    let a = cube.face(face).get(row, col);
                    let b = cube.face(face).get(row, col + 1);
                    counts[pair_index(a, b)] += 1;
                    total += 1;
                }
                if row + 1 < n {
                    let a = cube.face(face).get(row, col);
                    let b = cube.face(face).get(row + 1, col);
                    counts[pair_index(a, b)] += 1;
                    total += 1;
                }
            }
        }
    }

    let total = total as f64;
    let distance = counts
        .into_iter()
        .enumerate()
        .map(|(index, count)| {
            let (a, b) = pair_colors(index);
            let target = if a == b { 1.0 / 36.0 } else { 2.0 / 36.0 };
            ((count as f64 / total) - target).abs()
        })
        .sum::<f64>();

    0.5 * distance
}

fn pair_index(a: Facelet, b: Facelet) -> usize {
    let mut a = a.as_u8() as usize;
    let mut b = b.as_u8() as usize;
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }

    let mut index = 0usize;
    for first in 0..a {
        index += 6 - first;
    }
    index + (b - a)
}

fn pair_colors(mut index: usize) -> (usize, usize) {
    for first in 0..6 {
        let row_len = 6 - first;
        if index < row_len {
            return (first, first + index);
        }
        index -= row_len;
    }
    unreachable!("pair index must be in 0..21")
}

fn render_csv(rows: &[Row]) -> String {
    let mut out = String::from("k,method,face_color_tv,neighbor_pair_tv,elapsed_ms\n");
    for row in rows {
        let _ = writeln!(
            out,
            "{},{},{:.8},{:.8},{:.6}",
            row.k,
            row.method.name(),
            row.stats.face_tv,
            row.stats.pair_tv,
            row.stats.elapsed.as_secs_f64() * 1000.0,
        );
    }
    out
}

fn render_svg(cli: &Cli, rows: &[Row]) -> String {
    let width = 1120.0;
    let height = 720.0;
    let panel_w = 470.0;
    let panel_h = 430.0;
    let left_x = 80.0;
    let right_x = 610.0;
    let top_y = 130.0;
    let max_k = cli.max_k.max(1) as f64;
    let face_max = metric_max(rows, |stats| stats.face_tv).max(0.05);
    let pair_max = metric_max(rows, |stats| stats.pair_tv).max(0.05);

    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">"#
    );
    out.push_str(
        r#"<style>
text{font-family:Arial,Helvetica,sans-serif;fill:#111827}
.subtitle{fill:#4b5563;font-size:14px}
.axis{stroke:#111827;stroke-width:1.2}
.grid{stroke:#e5e7eb;stroke-width:1}
.tick{fill:#374151;font-size:12px}
.title{font-size:22px;font-weight:700}
.panel-title{font-size:16px;font-weight:700}
.legend{font-size:13px}
</style>
"#,
    );
    let _ = writeln!(
        out,
        r#"<text x="80" y="42" class="title">Scramble mixing statistics, n={}</text>"#,
        cli.side_length
    );
    let _ = writeln!(
        out,
        r#"<text x="80" y="66" class="subtitle">Average over {} trials. Lower total variation distance means closer to random/uniform.</text>"#,
        cli.trials
    );
    let _ = writeln!(
        out,
        r#"<text x="80" y="88" class="subtitle">Each method uses 3*n*k move attempts; batched layers choose independent depths on one random axis per batch.</text>"#
    );

    render_panel(
        &mut out,
        rows,
        PlotSpec {
            x: left_x,
            y: top_y,
            width: panel_w,
            height: panel_h,
            max_k,
            max_y: face_max,
            title: "Face color distribution",
            y_label: "TV distance from uniform face/color bins",
            metric: |stats| stats.face_tv,
        },
    );
    render_panel(
        &mut out,
        rows,
        PlotSpec {
            x: right_x,
            y: top_y,
            width: panel_w,
            height: panel_h,
            max_k,
            max_y: pair_max,
            title: "Neighbor color-pair distribution",
            y_label: "TV distance from random unordered pairs",
            metric: |stats| stats.pair_tv,
        },
    );

    let legend_y = 620.0;
    for (index, method) in Method::ALL.into_iter().enumerate() {
        let x = 390.0 + index as f64 * 220.0;
        let _ = writeln!(
            out,
            r#"<line x1="{x}" y1="{legend_y}" x2="{}" y2="{legend_y}" stroke="{}" stroke-width="3"/>"#,
            x + 38.0,
            method.color()
        );
        let _ = writeln!(
            out,
            r#"<text x="{}" y="{}" class="legend">{}</text>"#,
            x + 48.0,
            legend_y + 4.0,
            method.label()
        );
    }

    out.push_str("</svg>\n");
    out
}

struct PlotSpec {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    max_k: f64,
    max_y: f64,
    title: &'static str,
    y_label: &'static str,
    metric: fn(Stats) -> f64,
}

fn render_panel(out: &mut String, rows: &[Row], spec: PlotSpec) {
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" class="panel-title">{}</text>"#,
        spec.x,
        spec.y - 42.0,
        spec.title
    );
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" class="subtitle">{}</text>"#,
        spec.x,
        spec.y - 22.0,
        spec.y_label
    );

    for tick in 0..=4 {
        let ratio = tick as f64 / 4.0;
        let y = spec.y + spec.height - spec.height * ratio;
        let value = spec.max_y * ratio;
        let _ = writeln!(
            out,
            r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" class="grid"/>"#,
            spec.x,
            spec.x + spec.width
        );
        let _ = writeln!(
            out,
            r#"<text x="{}" y="{}" text-anchor="end" class="tick">{value:.2}</text>"#,
            spec.x - 8.0,
            y + 4.0,
        );
    }

    for tick in 0..=4 {
        let ratio = tick as f64 / 4.0;
        let x = spec.x + spec.width * ratio;
        let value = (spec.max_k * ratio).round() as usize;
        let _ = writeln!(
            out,
            r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" class="grid"/>"#,
            spec.y,
            spec.y + spec.height
        );
        let _ = writeln!(
            out,
            r#"<text x="{x}" y="{}" text-anchor="middle" class="tick">{value}</text>"#,
            spec.y + spec.height + 22.0
        );
    }

    let _ = writeln!(
        out,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="axis"/>"#,
        spec.x,
        spec.y + spec.height,
        spec.x + spec.width,
        spec.y + spec.height
    );
    let _ = writeln!(
        out,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="axis"/>"#,
        spec.x,
        spec.y,
        spec.x,
        spec.y + spec.height
    );
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" text-anchor="middle" class="tick">k</text>"#,
        spec.x + spec.width / 2.0,
        spec.y + spec.height + 50.0
    );

    for method in Method::ALL {
        let mut points = String::new();
        for row in rows.iter().filter(|row| row.method == method) {
            let x = spec.x + spec.width * row.k as f64 / spec.max_k;
            let y = spec.y + spec.height - spec.height * (spec.metric)(row.stats) / spec.max_y;
            let _ = write!(points, "{x:.2},{y:.2} ");
        }
        let _ = writeln!(
            out,
            r#"<polyline points="{points}" fill="none" stroke="{}" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round"/>"#,
            method.color()
        );
    }
}

fn metric_max(rows: &[Row], metric: fn(Stats) -> f64) -> f64 {
    rows.iter().map(|row| metric(row.stats)).fold(0.0, f64::max)
}

fn print_summary(cli: &Cli, rows: &[Row]) {
    println!(
        "scramble stats: n={}, k=0..{}, trials={}, threads={}",
        cli.side_length, cli.max_k, cli.trials, cli.thread_count
    );
    println!("method                  final_face_tv  final_pair_tv  final_elapsed_ms");
    for method in Method::ALL {
        if let Some(row) = rows
            .iter()
            .find(|row| row.method == method && row.k == cli.max_k)
        {
            println!(
                "{:<23} {:>13.5} {:>13.5} {:>16.4}",
                method.name(),
                row.stats.face_tv,
                row.stats.pair_tv,
                row.stats.elapsed.as_secs_f64() * 1000.0,
            );
        }
    }
}

fn write_text(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(path, content).map_err(|error| format!("failed to write {}: {error}", path.display()))
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

fn parse_positive_usize(value: &str, name: &str) -> Result<usize, String> {
    let parsed = parse_usize(value, name)?;
    if parsed == 0 {
        return Err(format!("{name} must be greater than zero"));
    }
    Ok(parsed)
}

fn parse_usize(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a non-negative integer"))
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
Usage: cargo run --release --bin scramble_stats -- [options]

Options:
  -n, --n <N>              Cube side length. Default: {DEFAULT_SIDE_LENGTH}
      --max-k <K>          Largest k value to measure. Default: {DEFAULT_MAX_K}
      --trials <N>         Trials per k and method. Default: {DEFAULT_TRIALS}
      --seed <N>           Base seed, decimal or 0x-prefixed hex. Default: 0x{DEFAULT_SEED:016X}
  -t, --threads <N>        Optimized worker threads. Default: available CPUs ({})
      --output <PATH>      SVG graph output. Default: {DEFAULT_OUTPUT}
      --csv-output <PATH>  CSV output. Default: output path with .csv extension
  -h, --help              Print this help.
",
        default_thread_count()
    )
}
