use std::{
    env,
    fmt::{self, Write as _},
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    str::FromStr,
    time::{Duration, Instant},
};

use rubik::{
    conventions::face_outer_move, Axis, Byte, Byte3, CenterReductionStage, CornerReductionStage,
    Cube, EdgePairingStage, FaceId, FaceletArray, Move, MoveAngle, Nibble, RandomSource,
    SolveAlgorithm, SolveContext, SolveOptions, ThreeBit, XorShift64,
};

const DEFAULT_RUN_COUNT: usize = 3;
const DEFAULT_SCRAMBLE_ROUNDS: usize = 8;
const DEFAULT_RANDOM_SEED: u64 = 42;
const DEFAULT_OUTPUT_DIR: &str = "target/benchmarks";
const STAGE_COUNT: usize = 6;
const DEFAULT_FIT_MIN_SIDE_LENGTH: usize = 1 << 8;
const FIT_MAX_EXTRAPOLATION_POWER: usize = 18;
const FIT_LINE_DASH_ARRAY: &str = "10 8";
const FIT_LINE_OPACITY: f64 = 0.9;

const SVG_WIDTH: f64 = 1440.0;
const SVG_HEIGHT: f64 = 960.0;
const PLOT_LEFT: f64 = 110.0;
const PLOT_RIGHT: f64 = 1110.0;
const PLOT_TOP: f64 = 150.0;
const PLOT_BOTTOM: f64 = 760.0;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum StorageKind {
    Byte,
    Nibble,
    ThreeBit,
    Byte3,
}

impl StorageKind {
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
            "3bit" | "three_bit" | "threebit" => Ok(Self::ThreeBit),
            "byte3" => Ok(Self::Byte3),
            _ => Err(format!("unknown storage kind: {value}")),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct StageSpec {
    key: &'static str,
    label: &'static str,
    color: &'static str,
    stroke_width: f64,
    dash_array: Option<&'static str>,
}

const STAGES: [StageSpec; STAGE_COUNT] = [
    StageSpec {
        key: "init",
        label: "Initialize",
        color: "#2F6B9A",
        stroke_width: 2.5,
        dash_array: None,
    },
    StageSpec {
        key: "scramble",
        label: "Scramble",
        color: "#3F8F5A",
        stroke_width: 2.5,
        dash_array: None,
    },
    StageSpec {
        key: "center",
        label: "Center",
        color: "#D97706",
        stroke_width: 2.5,
        dash_array: None,
    },
    StageSpec {
        key: "corner",
        label: "Corner",
        color: "#C2410C",
        stroke_width: 2.5,
        dash_array: None,
    },
    StageSpec {
        key: "edge",
        label: "Edge",
        color: "#8B5E34",
        stroke_width: 2.5,
        dash_array: None,
    },
    StageSpec {
        key: "total",
        label: "Total",
        color: "#111827",
        stroke_width: 4.0,
        dash_array: None,
    },
];

#[derive(Copy, Clone, Debug)]
struct StageTimings {
    milliseconds: [f64; STAGE_COUNT],
}

impl StageTimings {
    fn new(
        init: Duration,
        scramble: Duration,
        center: Duration,
        corner: Duration,
        edge: Duration,
        total: Duration,
    ) -> Self {
        Self {
            milliseconds: [
                milliseconds(init),
                milliseconds(scramble),
                milliseconds(center),
                milliseconds(corner),
                milliseconds(edge),
                milliseconds(total),
            ],
        }
    }
}

#[derive(Clone, Debug)]
struct BenchmarkCase {
    power: usize,
    side_length: usize,
    runs: Vec<StageTimings>,
    mean_milliseconds: [f64; STAGE_COUNT],
}

#[derive(Copy, Clone, Debug)]
struct LogLogFit {
    intercept_log10: f64,
    coefficient: f64,
    exponent: f64,
    r_squared: f64,
    sample_count: usize,
    min_side_length: usize,
    max_side_length: usize,
}

impl LogLogFit {
    fn predict(self, side_length: usize) -> f64 {
        self.coefficient * (side_length as f64).powf(self.exponent)
    }
}

fn main() {
    let side_powers = environment_usize_list("RUBIK_CYCLE_BENCHMARK_SIDE_POWERS")
        .unwrap_or_else(|| (0..=12).collect());
    let run_count = environment_usize("RUBIK_CYCLE_BENCHMARK_RUNS", DEFAULT_RUN_COUNT);
    let scramble_rounds = environment_usize(
        "RUBIK_CYCLE_BENCHMARK_SCRAMBLE_ROUNDS",
        DEFAULT_SCRAMBLE_ROUNDS,
    );
    let random_seed = environment_u64("RUBIK_CYCLE_BENCHMARK_RANDOM_SEED", DEFAULT_RANDOM_SEED);
    let fit_min_side_length = environment_usize(
        "RUBIK_CYCLE_BENCHMARK_FIT_MIN_SIDE_LENGTH",
        DEFAULT_FIT_MIN_SIDE_LENGTH,
    );
    let storage = environment_storage_kind("RUBIK_CYCLE_BENCHMARK_BACKEND", StorageKind::Byte);
    let output_dir = env::var("RUBIK_CYCLE_BENCHMARK_OUTPUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_OUTPUT_DIR));

    assert!(
        !side_powers.is_empty(),
        "at least one side power is required"
    );
    assert!(
        run_count > 0,
        "RUBIK_CYCLE_BENCHMARK_RUNS must be greater than 0"
    );
    fs::create_dir_all(&output_dir).expect("failed to create benchmark output directory");

    let file_stem = format!("init_scramble_solve_cycle_{}_optimized", storage.as_str());
    let csv_path = output_dir.join(format!("{file_stem}.csv"));
    let svg_path = output_dir.join(format!("{file_stem}.svg"));
    let fit_path = output_dir.join(format!("{file_stem}_fits.txt"));

    println!("init/scramble/solve benchmark");
    println!("  backend={storage}");
    println!("  solve_mode=optimized");
    println!("  side_powers={side_powers:?}");
    println!("  runs_per_size={run_count}");
    println!("  scramble_rounds={scramble_rounds}");
    println!("  random_seed={random_seed}");
    println!("  fit_min_side_length={fit_min_side_length}");
    println!("  output_csv={}", csv_path.display());
    println!("  output_svg={}", svg_path.display());
    println!("  output_fits={}", fit_path.display());
    println!();

    let results = match storage {
        StorageKind::Byte => {
            run_benchmark::<Byte>(&side_powers, run_count, scramble_rounds, random_seed)
        }
        StorageKind::Nibble => {
            run_benchmark::<Nibble>(&side_powers, run_count, scramble_rounds, random_seed)
        }
        StorageKind::ThreeBit => {
            run_benchmark::<ThreeBit>(&side_powers, run_count, scramble_rounds, random_seed)
        }
        StorageKind::Byte3 => {
            run_benchmark::<Byte3>(&side_powers, run_count, scramble_rounds, random_seed)
        }
    };

    println!();
    print_summary_table(&results);

    let fits = fit_stage_curves(&results, fit_min_side_length);
    write_csv(&results, run_count, &csv_path);
    write_svg(
        &results,
        &fits,
        fit_min_side_length,
        run_count,
        storage,
        scramble_rounds,
        &svg_path,
    );
    write_fit_report(
        &results,
        &fits,
        &side_powers,
        run_count,
        storage,
        scramble_rounds,
        random_seed,
        fit_min_side_length,
        &fit_path,
    );
}

fn run_benchmark<S: FaceletArray + 'static>(
    side_powers: &[usize],
    run_count: usize,
    scramble_rounds: usize,
    random_seed: u64,
) -> Vec<BenchmarkCase> {
    let mut results = Vec::with_capacity(side_powers.len());

    for power in side_powers.iter().copied() {
        let side_length = 1usize
            .checked_shl(power as u32)
            .expect("side length power overflowed usize");
        let scramble_moves = generate_scramble_moves(
            side_length,
            scramble_rounds,
            random_seed ^ side_length as u64,
        );

        println!(
            "running n={side_length} (2^{power}), scramble_moves={}",
            scramble_moves.len()
        );

        let mut runs = Vec::with_capacity(run_count);
        for run_index in 0..run_count {
            let timings = run_cycle::<S>(side_length, &scramble_moves);
            println!(
                "  run {}/{}: total={:.3} ms",
                run_index + 1,
                run_count,
                timings.milliseconds[STAGE_COUNT - 1]
            );
            runs.push(timings);
        }

        results.push(BenchmarkCase {
            power,
            side_length,
            mean_milliseconds: mean_milliseconds(&runs),
            runs,
        });
    }

    results
}

fn run_cycle<S: FaceletArray + 'static>(
    side_length: usize,
    scramble_moves: &[Move],
) -> StageTimings {
    let cycle_start = Instant::now();

    let init_start = Instant::now();
    let mut cube = Cube::<S>::new_solved(side_length);
    let init_elapsed = init_start.elapsed();

    let scramble_start = Instant::now();
    cube.apply_moves_untracked(scramble_moves.iter().copied());
    let scramble_elapsed = scramble_start.elapsed();

    let mut context = SolveContext::new(SolveOptions::optimized());
    let center_elapsed = run_stage(
        &mut cube,
        &mut context,
        CenterReductionStage::western_default,
    );
    let corner_elapsed = run_stage(&mut cube, &mut context, CornerReductionStage::default);
    let edge_elapsed = run_stage(&mut cube, &mut context, EdgePairingStage::default);
    let total_elapsed = cycle_start.elapsed();

    assert!(
        cube.is_solved(),
        "cycle did not solve the cube for n={side_length}"
    );
    black_box(&cube);

    StageTimings::new(
        init_elapsed,
        scramble_elapsed,
        center_elapsed,
        corner_elapsed,
        edge_elapsed,
        total_elapsed,
    )
}

fn run_stage<S, A, F>(cube: &mut Cube<S>, context: &mut SolveContext, build_stage: F) -> Duration
where
    S: FaceletArray,
    A: SolveAlgorithm<S>,
    F: FnOnce() -> A,
{
    let stage_start = Instant::now();
    let mut stage = build_stage();
    let stage_name = stage.name();

    assert!(
        stage.execution_mode_support().supports_optimized(),
        "{stage_name} does not support optimized mode",
    );
    assert!(
        stage.is_applicable_to_side_length(cube.side_len()),
        "{stage_name} does not support side length {}",
        cube.side_len()
    );

    stage
        .run(cube, context)
        .unwrap_or_else(|error| panic!("{stage_name} failed for n={}: {error}", cube.side_len()));

    stage_start.elapsed()
}

fn mean_milliseconds(samples: &[StageTimings]) -> [f64; STAGE_COUNT] {
    let mut sums = [0.0; STAGE_COUNT];

    for sample in samples {
        for (index, value) in sample.milliseconds.iter().copied().enumerate() {
            sums[index] += value;
        }
    }

    let sample_count = samples.len() as f64;
    sums.map(|sum| sum / sample_count)
}

fn print_summary_table(results: &[BenchmarkCase]) {
    const HEADERS: [&str; STAGE_COUNT + 2] = [
        "pow",
        "n",
        "init_ms",
        "scramble_ms",
        "center_ms",
        "corner_ms",
        "edge_ms",
        "total_ms",
    ];
    let mut rows = Vec::with_capacity(results.len());

    for result in results {
        rows.push([
            result.power.to_string(),
            result.side_length.to_string(),
            format!("{:.3}", result.mean_milliseconds[0]),
            format!("{:.3}", result.mean_milliseconds[1]),
            format!("{:.3}", result.mean_milliseconds[2]),
            format!("{:.3}", result.mean_milliseconds[3]),
            format!("{:.3}", result.mean_milliseconds[4]),
            format!("{:.3}", result.mean_milliseconds[5]),
        ]);
    }

    let mut widths = HEADERS.map(str::len);
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    for (index, header) in HEADERS.iter().enumerate() {
        if index > 0 {
            print!(" | ");
        }
        print!("{:>width$}", header, width = widths[index]);
    }
    println!();

    for (index, width) in widths.iter().copied().enumerate() {
        if index > 0 {
            print!("-+-");
        }
        print!("{:-<width$}", "", width = width);
    }
    println!();

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                print!(" | ");
            }
            print!("{:>width$}", cell, width = widths[index]);
        }
        println!();
    }
}

fn write_csv(results: &[BenchmarkCase], run_count: usize, path: &Path) {
    let mut csv = String::new();
    csv.push_str("power,side_length,stage,mean_ms");
    for run_index in 0..run_count {
        let _ = write!(csv, ",run{}_ms", run_index + 1);
    }
    csv.push('\n');

    for result in results {
        for (stage_index, stage) in STAGES.iter().enumerate() {
            let _ = write!(
                csv,
                "{},{},{},{:.6}",
                result.power, result.side_length, stage.key, result.mean_milliseconds[stage_index]
            );
            for run in &result.runs {
                let _ = write!(csv, ",{:.6}", run.milliseconds[stage_index]);
            }
            csv.push('\n');
        }
    }

    fs::write(path, csv)
        .unwrap_or_else(|error| panic!("failed to write CSV to {}: {error}", path.display()));
}

fn write_svg(
    results: &[BenchmarkCase],
    fits: &[Option<LogLogFit>; STAGE_COUNT],
    fit_min_side_length: usize,
    run_count: usize,
    storage: StorageKind,
    scramble_rounds: usize,
    path: &Path,
) {
    let x_min = results
        .first()
        .map(|result| result.side_length as f64)
        .expect("benchmark results must not be empty");
    let x_max = results
        .last()
        .map(|result| result.side_length as f64)
        .expect("benchmark results must not be empty");

    let mut min_y = f64::INFINITY;
    let mut max_y = 0.0f64;
    for result in results {
        for value in result.mean_milliseconds {
            let clamped = clamp_plot_value(value);
            min_y = min_y.min(clamped);
            max_y = max_y.max(clamped);
        }
    }
    for fit in fits.iter().flatten() {
        let start = fit.min_side_length.max(results[0].side_length);
        let end = results
            .last()
            .map(|result| result.side_length)
            .expect("benchmark results must not be empty");
        for side_length in [start, end] {
            let clamped = clamp_plot_value(fit.predict(side_length));
            min_y = min_y.min(clamped);
            max_y = max_y.max(clamped);
        }
    }

    let y_min_log = min_y.log10().floor();
    let mut y_max_log = max_y.log10().ceil();
    if (y_max_log - y_min_log).abs() < f64::EPSILON {
        y_max_log += 1.0;
    }

    let x_min_log = x_min.log10();
    let x_max_log = x_max.log10();
    let plot_width = PLOT_RIGHT - PLOT_LEFT;
    let plot_height = PLOT_BOTTOM - PLOT_TOP;

    let x_log_span = (x_max_log - x_min_log).max(f64::EPSILON);
    let map_x = |value: f64| -> f64 {
        if results.len() == 1 {
            return (PLOT_LEFT + PLOT_RIGHT) / 2.0;
        }

        let t = (value.log10() - x_min_log) / x_log_span;
        PLOT_LEFT + t * plot_width
    };
    let map_y = |value: f64| -> f64 {
        let clamped = clamp_plot_value(value);
        let t = (clamped.log10() - y_min_log) / (y_max_log - y_min_log);
        PLOT_BOTTOM - t * plot_height
    };

    let legend_x = 1160.0;
    let legend_y = 180.0;
    let legend_row_height = 34.0;

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{SVG_WIDTH}" height="{SVG_HEIGHT}" viewBox="0 0 {SVG_WIDTH} {SVG_HEIGHT}" role="img" aria-labelledby="title desc">"##
    );
    svg.push_str(
        r##"<title id="title">Rubik init/scramble/solve benchmark</title>
<desc id="desc">Log-log benchmark plot of mean stage times across powers-of-two cube side lengths.</desc>
<defs>
  <style>
    text { font-family: "Iosevka", "IBM Plex Sans", "Segoe UI", sans-serif; fill: #111827; }
    .title { font-size: 30px; font-weight: 700; }
    .subtitle { font-size: 15px; fill: #4B5563; }
    .axis-label { font-size: 16px; font-weight: 600; }
    .tick { font-size: 12px; fill: #374151; }
    .legend { font-size: 14px; }
    .grid { stroke: #E5DED3; stroke-width: 1; }
    .axis { stroke: #3F3F46; stroke-width: 1.5; }
    .plot-bg { fill: #FCFBF8; stroke: #DDD6CA; stroke-width: 1; }
  </style>
</defs>
<rect width="100%" height="100%" fill="#F6F3EE"/>
<rect x="36" y="36" width="1368" height="888" rx="24" fill="#FFFDF9" stroke="#E7E0D4"/>
"##,
    );

    let _ = writeln!(
        svg,
        r#"<text class="title" x="70" y="92">Rubik init/scramble/solve cycle benchmark</text>"#
    );
    let _ = writeln!(
        svg,
        r#"<text class="subtitle" x="70" y="122">Backend: {storage} • Solve mode: optimized only • Mean of {run_count} sequential runs • Scramble rounds: {scramble_rounds} • Dashed line: log-log fit for n &gt;= {fit_min_side_length}</text>"#
    );
    let _ = writeln!(
        svg,
        r#"<rect class="plot-bg" x="{PLOT_LEFT}" y="{PLOT_TOP}" width="{plot_width}" height="{plot_height}" rx="16"/>"#
    );

    for result in results {
        let x = map_x(result.side_length as f64);
        let _ = writeln!(
            svg,
            r#"<line class="grid" x1="{x:.3}" y1="{PLOT_TOP}" x2="{x:.3}" y2="{PLOT_BOTTOM}"/>"#
        );
        let _ = writeln!(
            svg,
            r#"<text class="tick" x="{x:.3}" y="790" text-anchor="middle">{}</text>"#,
            result.side_length
        );
    }

    let start_decade = y_min_log as i32;
    let end_decade = y_max_log as i32;
    for decade in start_decade..=end_decade {
        let value = 10f64.powi(decade);
        let y = map_y(value);
        let _ = writeln!(
            svg,
            r#"<line class="grid" x1="{PLOT_LEFT}" y1="{y:.3}" x2="{PLOT_RIGHT}" y2="{y:.3}"/>"#
        );
        let _ = writeln!(
            svg,
            r#"<text class="tick" x="96" y="{:.3}" text-anchor="end" dominant-baseline="middle">{}</text>"#,
            y,
            format_time_label(value)
        );
    }

    let _ = writeln!(
        svg,
        r#"<line class="axis" x1="{PLOT_LEFT}" y1="{PLOT_BOTTOM}" x2="{PLOT_RIGHT}" y2="{PLOT_BOTTOM}"/>"#
    );
    let _ = writeln!(
        svg,
        r#"<line class="axis" x1="{PLOT_LEFT}" y1="{PLOT_TOP}" x2="{PLOT_LEFT}" y2="{PLOT_BOTTOM}"/>"#
    );
    let _ = writeln!(
        svg,
        r#"<text class="axis-label" x="{:.3}" y="850" text-anchor="middle">Cube side length n (log scale)</text>"#,
        (PLOT_LEFT + PLOT_RIGHT) / 2.0
    );
    let _ = writeln!(
        svg,
        r#"<text class="axis-label" x="34" y="{:.3}" transform="rotate(-90 34 {:.3})" text-anchor="middle">Mean stage time (log scale)</text>"#,
        (PLOT_TOP + PLOT_BOTTOM) / 2.0,
        (PLOT_TOP + PLOT_BOTTOM) / 2.0
    );

    for (stage_index, stage) in STAGES.iter().enumerate() {
        let mut points = String::new();
        for result in results {
            let x = map_x(result.side_length as f64);
            let y = map_y(result.mean_milliseconds[stage_index]);
            let _ = write!(points, "{x:.3},{y:.3} ");
        }

        let dash = stage
            .dash_array
            .map(|value| format!(r#" stroke-dasharray="{value}""#))
            .unwrap_or_default();
        let _ = writeln!(
            svg,
            r#"<polyline fill="none" stroke="{}" stroke-width="{}"{} stroke-linecap="round" stroke-linejoin="round" points="{}"/>"#,
            stage.color,
            stage.stroke_width,
            dash,
            points.trim_end()
        );

        if let Some(fit) = fits[stage_index] {
            let fit_start = fit.min_side_length.max(results[0].side_length);
            let fit_end = results
                .last()
                .map(|result| result.side_length)
                .expect("benchmark results must not be empty");
            let _ = writeln!(
                svg,
                r#"<line x1="{:.3}" y1="{:.3}" x2="{:.3}" y2="{:.3}" stroke="{}" stroke-width="{}" stroke-dasharray="{}" stroke-linecap="round" opacity="{}"/>"#,
                map_x(fit_start as f64),
                map_y(fit.predict(fit_start)),
                map_x(fit_end as f64),
                map_y(fit.predict(fit_end)),
                stage.color,
                stage.stroke_width,
                FIT_LINE_DASH_ARRAY,
                FIT_LINE_OPACITY,
            );
        }

        for result in results {
            let x = map_x(result.side_length as f64);
            let y = map_y(result.mean_milliseconds[stage_index]);
            let _ = writeln!(
                svg,
                r##"<circle cx="{x:.3}" cy="{y:.3}" r="4.2" fill="#FFFDF9" stroke="{}" stroke-width="2"/>"##,
                stage.color
            );
        }

        let legend_row = legend_y + legend_row_height * stage_index as f64;
        let legend_dash = stage
            .dash_array
            .map(|value| format!(r#" stroke-dasharray="{value}""#))
            .unwrap_or_default();
        let _ = writeln!(
            svg,
            r#"<line x1="{legend_x}" y1="{legend_row}" x2="{:.3}" y2="{legend_row}" stroke="{}" stroke-width="{}"{} stroke-linecap="round"/>"#,
            legend_x + 34.0,
            stage.color,
            stage.stroke_width,
            legend_dash
        );
        let _ = writeln!(
            svg,
            r##"<circle cx="{:.3}" cy="{legend_row}" r="4.2" fill="#FFFDF9" stroke="{}" stroke-width="2"/>"##,
            legend_x + 17.0,
            stage.color
        );
        let _ = writeln!(
            svg,
            r#"<text class="legend" x="{:.3}" y="{:.3}" dominant-baseline="middle">{}</text>"#,
            legend_x + 48.0,
            legend_row,
            stage.label
        );
    }

    let _ = writeln!(
        svg,
        r#"<text class="subtitle" x="{legend_x}" y="142">Mean milliseconds per stage</text>"#
    );
    let _ = writeln!(
        svg,
        r#"<text class="subtitle" x="{legend_x}" y="164">Solid = measured mean, dashed = fit</text>"#
    );
    svg.push_str("</svg>\n");

    fs::write(path, svg)
        .unwrap_or_else(|error| panic!("failed to write SVG to {}: {error}", path.display()));
}

fn clamp_plot_value(value: f64) -> f64 {
    value.max(0.000_001)
}

fn fit_stage_curves(
    results: &[BenchmarkCase],
    min_side_length: usize,
) -> [Option<LogLogFit>; STAGE_COUNT] {
    std::array::from_fn(|stage_index| fit_stage_curve(results, stage_index, min_side_length))
}

fn fit_stage_curve(
    results: &[BenchmarkCase],
    stage_index: usize,
    min_side_length: usize,
) -> Option<LogLogFit> {
    let mut samples = Vec::new();

    for result in results {
        if result.side_length < min_side_length {
            continue;
        }

        samples.push((
            result.side_length,
            clamp_plot_value(result.mean_milliseconds[stage_index]),
        ));
    }

    if samples.len() < 2 {
        return None;
    }

    let sample_count = samples.len() as f64;
    let mean_x = samples
        .iter()
        .map(|(side_length, _)| (*side_length as f64).log10())
        .sum::<f64>()
        / sample_count;
    let mean_y = samples
        .iter()
        .map(|(_, milliseconds)| milliseconds.log10())
        .sum::<f64>()
        / sample_count;

    let mut sum_xx = 0.0;
    let mut sum_xy = 0.0;
    for (side_length, milliseconds) in &samples {
        let x = (*side_length as f64).log10() - mean_x;
        let y = milliseconds.log10() - mean_y;
        sum_xx += x * x;
        sum_xy += x * y;
    }

    if sum_xx <= f64::EPSILON {
        return None;
    }

    let exponent = sum_xy / sum_xx;
    let intercept_log10 = mean_y - exponent * mean_x;
    let coefficient = 10f64.powf(intercept_log10);

    let mut sse = 0.0;
    let mut sst = 0.0;
    for (side_length, milliseconds) in &samples {
        let x = (*side_length as f64).log10();
        let y = milliseconds.log10();
        let predicted = intercept_log10 + exponent * x;
        sse += (y - predicted) * (y - predicted);
        sst += (y - mean_y) * (y - mean_y);
    }

    Some(LogLogFit {
        intercept_log10,
        coefficient,
        exponent,
        r_squared: if sst <= f64::EPSILON {
            1.0
        } else {
            1.0 - sse / sst
        },
        sample_count: samples.len(),
        min_side_length: samples[0].0,
        max_side_length: samples[samples.len() - 1].0,
    })
}

fn write_fit_report(
    results: &[BenchmarkCase],
    fits: &[Option<LogLogFit>; STAGE_COUNT],
    side_powers: &[usize],
    run_count: usize,
    storage: StorageKind,
    scramble_rounds: usize,
    random_seed: u64,
    fit_min_side_length: usize,
    path: &Path,
) {
    let mut report = String::new();
    let _ = writeln!(report, "Rubik init/scramble/solve cycle benchmark fits");
    let _ = writeln!(report, "backend={storage}");
    let _ = writeln!(report, "solve_mode=optimized");
    let _ = writeln!(report, "side_powers={side_powers:?}");
    let _ = writeln!(report, "runs_per_size={run_count}");
    let _ = writeln!(report, "scramble_rounds={scramble_rounds}");
    let _ = writeln!(report, "random_seed={random_seed}");
    let _ = writeln!(report, "fit_model=t_ms(n)=coefficient*n^exponent");
    let _ = writeln!(report, "fit_method=least-squares line fit in log10 space");
    let _ = writeln!(report, "fit_domain=n>={fit_min_side_length}");
    let _ = writeln!(report);
    let _ = writeln!(report, "Formulas");

    for (stage_index, stage) in STAGES.iter().enumerate() {
        let _ = writeln!(report, "{}:", stage.label);
        if let Some(fit) = fits[stage_index] {
            let _ = writeln!(
                report,
                "  log10(t_ms) = {:.8} + {:.8} * log10(n)",
                fit.intercept_log10, fit.exponent
            );
            let _ = writeln!(
                report,
                "  t_ms(n) = {:.8e} * n^{:.8}",
                fit.coefficient, fit.exponent
            );
            let _ = writeln!(report, "  coefficient = {:.8e}", fit.coefficient);
            let _ = writeln!(report, "  exponent = {:.8}", fit.exponent);
            let _ = writeln!(report, "  r_squared_log10 = {:.8}", fit.r_squared);
            let _ = writeln!(report, "  sample_count = {}", fit.sample_count);
            let _ = writeln!(
                report,
                "  fitted_range = {}..={}",
                fit.min_side_length, fit.max_side_length
            );
        } else {
            let _ = writeln!(report, "  unavailable");
        }
        let _ = writeln!(report);
    }

    let _ = writeln!(report, "Extrapolated milliseconds from fitted formulas");
    report.push_str("power,side_length");
    for stage in STAGES {
        let _ = write!(report, ",{}_fit_ms", stage.key);
    }
    report.push('\n');

    let start_power = results
        .iter()
        .find(|result| result.side_length >= fit_min_side_length)
        .map(|result| result.power)
        .unwrap_or_else(|| fit_min_side_length.trailing_zeros() as usize);
    for power in start_power..=FIT_MAX_EXTRAPOLATION_POWER {
        let side_length = 1usize
            .checked_shl(power as u32)
            .expect("fit extrapolation power overflowed usize");
        let _ = write!(report, "{power},{side_length}");
        for fit in fits {
            if let Some(fit) = fit {
                let _ = write!(report, ",{:.6}", fit.predict(side_length));
            } else {
                report.push_str(",NA");
            }
        }
        report.push('\n');
    }

    fs::write(path, report).unwrap_or_else(|error| {
        panic!("failed to write fit report to {}: {error}", path.display())
    });
}

fn format_time_label(milliseconds: f64) -> String {
    if milliseconds >= 1_000.0 {
        format!("{:.0}s", milliseconds / 1_000.0)
    } else if milliseconds >= 1.0 {
        format!("{milliseconds:.0}ms")
    } else if milliseconds >= 0.001 {
        format!("{:.0}us", milliseconds * 1_000.0)
    } else {
        format!("{:.0}ns", milliseconds * 1_000_000.0)
    }
}

fn generate_scramble_moves(side_length: usize, rounds: usize, seed: u64) -> Vec<Move> {
    let per_round = side_length
        .checked_add(FaceId::ALL.len())
        .expect("scramble plan would overflow usize");
    let capacity = rounds
        .checked_mul(per_round)
        .expect("scramble plan would overflow usize");
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

fn environment_storage_kind(name: &str, default: StorageKind) -> StorageKind {
    env::var(name)
        .ok()
        .map(|value| {
            value
                .parse::<StorageKind>()
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

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}
