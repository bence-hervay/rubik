use std::{
    env,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process,
};

use rubik::{balanced_outer_layer_probability, RandomSource, XorShift64};

const DEFAULT_SIZES: [usize; 4] = [500, 1000, 2000, 4000];
const DEFAULT_PROBABILITY_PERCENTS: [usize; 19] = [
    5, 10, 15, 20, 25, 30, 35, 40, 45, 50, 55, 60, 65, 70, 75, 80, 85, 90, 95,
];
const DEFAULT_TRIALS: usize = 16;
const DEFAULT_SEED: u64 = 0xB1A5_5EED_2026;
const DEFAULT_OUTPUT: &str = "benchmark/scramble_probability_sweep.svg";
const DEFAULT_REPORT: &str = "benchmark/scramble_probability_sweep.txt";

#[derive(Clone, Debug)]
struct Cli {
    sizes: Vec<usize>,
    probability_percents: Vec<usize>,
    trials: usize,
    seed: u64,
    output: PathBuf,
    csv_output: Option<PathBuf>,
    report_output: PathBuf,
}

impl Default for Cli {
    fn default() -> Self {
        Self {
            sizes: DEFAULT_SIZES.to_vec(),
            probability_percents: DEFAULT_PROBABILITY_PERCENTS.to_vec(),
            trials: DEFAULT_TRIALS,
            seed: DEFAULT_SEED,
            output: PathBuf::from(DEFAULT_OUTPUT),
            csv_output: None,
            report_output: PathBuf::from(DEFAULT_REPORT),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct Row {
    side_length: usize,
    probability_percent: usize,
    probability: f64,
    balanced_probability: f64,
    ratio_to_balanced: f64,
    lambda_outer_per_k: f64,
    lambda_inner_per_k: f64,
    analytic_iterations: usize,
    expected_unseen_rate_before: f64,
    expected_unseen_rate_at: f64,
    empirical_unseen_rate_before: f64,
    empirical_unseen_rate_at: f64,
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
            "--sizes" => cli.sizes = parse_usize_list(&value(flag, inline_value, &mut iter)?)?,
            "--probabilities" => {
                cli.probability_percents =
                    parse_probability_percent_list(&value(flag, inline_value, &mut iter)?)?
            }
            "--trials" => {
                cli.trials = parse_positive_usize(&value(flag, inline_value, &mut iter)?, "trials")?
            }
            "--seed" => cli.seed = parse_u64(&value(flag, inline_value, &mut iter)?, "seed")?,
            "--output" => cli.output = PathBuf::from(value(flag, inline_value, &mut iter)?),
            "--csv-output" => {
                cli.csv_output = Some(PathBuf::from(value(flag, inline_value, &mut iter)?))
            }
            "--report-output" => {
                cli.report_output = PathBuf::from(value(flag, inline_value, &mut iter)?)
            }
            _ if flag.starts_with('-') => return Err(format!("unknown argument: {flag}")),
            _ => return Err(format!("unexpected positional argument: {flag}")),
        }
    }

    Ok(Command::Run(cli))
}

fn run(cli: Cli) -> Result<(), String> {
    let rows = collect_rows(&cli);
    let csv_output = cli
        .csv_output
        .clone()
        .unwrap_or_else(|| cli.output.with_extension("csv"));

    write_text(&csv_output, &render_csv(&rows, cli.trials))?;
    write_text(&cli.output, &render_svg(&cli, &rows))?;
    write_text(&cli.report_output, &render_report(&cli, &rows))?;

    print_summary(&rows);
    println!("wrote {}", cli.output.display());
    println!("wrote {}", csv_output.display());
    println!("wrote {}", cli.report_output.display());

    Ok(())
}

fn collect_rows(cli: &Cli) -> Vec<Row> {
    let mut rows = Vec::new();
    let target = blue_threshold_unseen_rate();

    for &side_length in &cli.sizes {
        let balanced_probability = balanced_outer_layer_probability(side_length);

        for &probability_percent in &cli.probability_percents {
            let probability = probability_percent as f64 / 100.0;
            let ratio_to_balanced = probability / balanced_probability;
            let (lambda_outer_per_k, lambda_inner_per_k) =
                layer_hit_expectations_per_k(side_length, probability);
            let analytic_iterations =
                analytic_iterations_to_threshold(side_length, probability, target);
            let before = analytic_iterations.saturating_sub(1);

            rows.push(Row {
                side_length,
                probability_percent,
                probability,
                balanced_probability,
                ratio_to_balanced,
                lambda_outer_per_k,
                lambda_inner_per_k,
                analytic_iterations,
                expected_unseen_rate_before: expected_unseen_layer_rate(
                    side_length,
                    probability,
                    before,
                ),
                expected_unseen_rate_at: expected_unseen_layer_rate(
                    side_length,
                    probability,
                    analytic_iterations,
                ),
                empirical_unseen_rate_before: simulate_unseen_layer_rate(
                    side_length,
                    probability,
                    before,
                    cli.trials,
                    seed_for(cli.seed, side_length, probability_percent, before),
                ),
                empirical_unseen_rate_at: simulate_unseen_layer_rate(
                    side_length,
                    probability,
                    analytic_iterations,
                    cli.trials,
                    seed_for(
                        cli.seed,
                        side_length,
                        probability_percent,
                        analytic_iterations,
                    ),
                ),
            });
        }
    }

    rows
}

fn blue_threshold_unseen_rate() -> f64 {
    (-4.0f64).exp()
}

fn layer_hit_expectations_per_k(side_length: usize, probability: f64) -> (f64, f64) {
    if side_length <= 2 {
        return (1.0, f64::INFINITY);
    }

    let n = side_length as f64;
    let outer = n * probability / 2.0;
    let inner = n * (1.0 - probability) / (side_length - 2) as f64;
    (outer, inner)
}

fn analytic_iterations_to_threshold(
    side_length: usize,
    probability: f64,
    target_rate: f64,
) -> usize {
    for k in 0..=512 {
        if expected_unseen_layer_rate(side_length, probability, k) <= target_rate {
            return k;
        }
    }

    512
}

fn expected_unseen_layer_rate(side_length: usize, probability: f64, k: usize) -> f64 {
    assert!(side_length > 0, "cube side length must be > 0");

    let moves = (3 * side_length * k) as f64;
    if side_length == 1 {
        return (1.0 - 1.0 / 3.0f64).powf(moves);
    }
    if side_length == 2 {
        return (1.0 - 1.0 / 6.0f64).powf(moves);
    }

    let outer_unseen = (1.0 - probability / 6.0).powf(moves);
    let inner_pick_probability = (1.0 - probability) / (3.0 * (side_length - 2) as f64);
    let inner_unseen = (1.0 - inner_pick_probability).powf(moves);
    let outer_layers = 6.0;
    let inner_layers = 3.0 * (side_length - 2) as f64;

    (outer_layers * outer_unseen + inner_layers * inner_unseen) / (3 * side_length) as f64
}

fn simulate_unseen_layer_rate(
    side_length: usize,
    probability: f64,
    k: usize,
    trials: usize,
    seed: u64,
) -> f64 {
    if trials == 0 {
        return 0.0;
    }

    let layer_count = 3 * side_length;
    let move_count = 3 * side_length * k;
    let mut seen = vec![0u32; layer_count];
    let mut total_unseen = 0usize;

    for trial in 0..trials {
        let stamp = trial as u32 + 1;
        let mut rng = XorShift64::new(seed ^ ((trial as u64) << 32));

        for _ in 0..move_count {
            let axis = (rng.next_u64() % 3) as usize;
            let layer = random_biased_layer(side_length, probability, &mut rng);
            seen[axis * side_length + layer] = stamp;
        }

        total_unseen += seen
            .iter()
            .filter(|&&stamp_seen| stamp_seen != stamp)
            .count();
    }

    total_unseen as f64 / (trials * layer_count) as f64
}

fn random_biased_layer<R: RandomSource>(
    side_length: usize,
    probability: f64,
    rng: &mut R,
) -> usize {
    if side_length <= 2 || random_unit_interval(rng) < probability {
        if rng.next_u64() & 1 == 0 {
            0
        } else {
            side_length - 1
        }
    } else {
        1 + (rng.next_u64() as usize % (side_length - 2))
    }
}

fn random_unit_interval<R: RandomSource>(rng: &mut R) -> f64 {
    const DENOMINATOR: f64 = u64::MAX as f64 + 1.0;
    rng.next_u64() as f64 / DENOMINATOR
}

fn seed_for(base: u64, side_length: usize, probability_percent: usize, k: usize) -> u64 {
    base ^ ((side_length as u64) << 32) ^ ((probability_percent as u64) << 16) ^ k as u64
}

fn render_csv(rows: &[Row], trials: usize) -> String {
    let mut out = String::from(
        "side_length,outer_probability_percent,outer_probability,balanced_probability_percent,ratio_to_balanced,lambda_outer_per_k,lambda_inner_per_k,analytic_iterations_to_blue_threshold,expected_unseen_rate_before,expected_unseen_rate_at,empirical_unseen_rate_before,empirical_unseen_rate_at,empirical_trials\n",
    );

    for row in rows {
        let _ = writeln!(
            out,
            "{},{},{:.6},{:.6},{:.3},{:.6},{:.6},{},{:.8},{:.8},{:.8},{:.8},{}",
            row.side_length,
            row.probability_percent,
            row.probability,
            100.0 * row.balanced_probability,
            row.ratio_to_balanced,
            row.lambda_outer_per_k,
            row.lambda_inner_per_k,
            row.analytic_iterations,
            row.expected_unseen_rate_before,
            row.expected_unseen_rate_at,
            row.empirical_unseen_rate_before,
            row.empirical_unseen_rate_at,
            trials,
        );
    }

    out
}

fn render_report(cli: &Cli, rows: &[Row]) -> String {
    let mut out = String::new();
    let target = blue_threshold_unseen_rate();
    let _ = writeln!(out, "Outer-layer probability sweep");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Target: same expected untouched-layer rate as n=20, p=10%, k=4: {:.4}%.\n",
        100.0 * target
    );
    let _ = writeln!(
        out,
        "Analytic balance: choose p = 2/n so one outer layer and one inner layer get the same expected hits per k."
    );
    let _ = writeln!(
        out,
        "Requested grid: every 5 percentage points from 5% through 95%."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "n, analytic p=2/n, best grid p, required k, expected unseen at k, empirical unseen at k"
    );

    for &side_length in &cli.sizes {
        if let Some(row) = best_grid_row(rows, side_length) {
            let _ = writeln!(
                out,
                "{}, {:.4}%, {}%, {}, {:.4}%, {:.4}%",
                row.side_length,
                100.0 * row.balanced_probability,
                row.probability_percent,
                row.analytic_iterations,
                100.0 * row.expected_unseen_rate_at,
                100.0 * row.empirical_unseen_rate_at,
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Within the requested 5%-95% grid, ties are broken by lower expected untouched-layer rate."
    );
    let _ = writeln!(
        out,
        "The unrestricted analytic optimum is below 5% for all listed large cube sizes, so the grid optimum lands on 5%."
    );

    out
}

fn best_grid_row(rows: &[Row], side_length: usize) -> Option<Row> {
    rows.iter()
        .copied()
        .filter(|row| row.side_length == side_length)
        .min_by(|a, b| {
            a.analytic_iterations
                .cmp(&b.analytic_iterations)
                .then_with(|| {
                    a.expected_unseen_rate_at
                        .total_cmp(&b.expected_unseen_rate_at)
                })
                .then_with(|| a.probability_percent.cmp(&b.probability_percent))
        })
}

fn render_svg(cli: &Cli, rows: &[Row]) -> String {
    let width = 1120.0;
    let height = 900.0;
    let left = 86.0;
    let top = 132.0;
    let panel_w = 930.0;
    let panel_h = 330.0;
    let empirical_top = 560.0;
    let empirical_h = 180.0;
    let x_min = *cli.probability_percents.iter().min().unwrap_or(&5) as f64;
    let x_max = *cli.probability_percents.iter().max().unwrap_or(&95) as f64;
    let max_k = rows
        .iter()
        .map(|row| row.analytic_iterations)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let target = blue_threshold_unseen_rate();
    let empirical_max = rows
        .iter()
        .map(|row| row.empirical_unseen_rate_at)
        .fold(target, f64::max)
        * 1.15;

    let mut out = String::new();
    let _ = writeln!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" width="{width}" height="{height}">"#
    );
    out.push_str(
        r#"<style>
text{font-family:Arial,Helvetica,sans-serif;fill:#111827}
.title{font-size:22px;font-weight:700}
.subtitle{fill:#4b5563;font-size:14px}
.panel-title{font-size:16px;font-weight:700}
.axis{stroke:#111827;stroke-width:1.2}
.grid{stroke:#e5e7eb;stroke-width:1}
.tick{fill:#374151;font-size:12px}
.legend{font-size:13px}
</style>
"#,
    );
    let _ = writeln!(
        out,
        r#"<text x="{left}" y="42" class="title">Outer-layer probability sweep</text>"#
    );
    let _ = writeln!(
        out,
        r#"<text x="{left}" y="66" class="subtitle">Probabilities are exactly 5%, 10%, ..., 95%. Target untouched-layer rate: {:.4}%.</text>"#,
        100.0 * target
    );
    let _ = writeln!(
        out,
        r#"<text x="{left}" y="88" class="subtitle">Required k is derived analytically and checked with {} randomized layer-hit trials per point.</text>"#,
        cli.trials
    );

    render_required_k_panel(
        &mut out,
        rows,
        Panel {
            x: left,
            y: top,
            width: panel_w,
            height: panel_h,
            x_min,
            x_max,
            max_y: max_k,
        },
        &cli.sizes,
    );

    render_empirical_rate_panel(
        &mut out,
        rows,
        Panel {
            x: left,
            y: empirical_top,
            width: panel_w,
            height: empirical_h,
            x_min,
            x_max,
            max_y: empirical_max,
        },
        &cli.sizes,
        target,
    );

    let legend_y = empirical_top + empirical_h + 70.0;
    for (index, side_length) in cli.sizes.iter().enumerate() {
        let x = left + index as f64 * 205.0;
        let _ = writeln!(
            out,
            r#"<line x1="{x}" y1="{legend_y}" x2="{}" y2="{legend_y}" stroke="{}" stroke-width="3"/>"#,
            x + 38.0,
            color_for_size_index(index)
        );
        let _ = writeln!(
            out,
            r#"<text x="{}" y="{}" class="legend">n={side_length}</text>"#,
            x + 48.0,
            legend_y + 4.0
        );
    }

    let note_y = legend_y + 58.0;
    let _ = writeln!(
        out,
        r#"<text x="{left}" y="{note_y}" class="subtitle">Analytic optimum p=2/n is below this grid for n=500..4000; inside the requested grid, lower p spends fewer moves over-hitting outer layers.</text>"#
    );

    out.push_str("</svg>\n");
    out
}

#[derive(Copy, Clone)]
struct Panel {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    x_min: f64,
    x_max: f64,
    max_y: f64,
}

fn render_required_k_panel(out: &mut String, rows: &[Row], panel: Panel, sizes: &[usize]) {
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" class="panel-title">Iterations needed to match the blue-method threshold</text>"#,
        panel.x,
        panel.y - 34.0
    );
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" class="subtitle">Lower is faster; ties prefer the lower probability because it gives more middle-layer coverage.</text>"#,
        panel.x,
        panel.y - 14.0
    );

    for tick in 0..=5 {
        let ratio = tick as f64 / 5.0;
        let y = panel.y + panel.height - panel.height * ratio;
        let value = panel.max_y * ratio;
        let _ = writeln!(
            out,
            r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" class="grid"/>"#,
            panel.x,
            panel.x + panel.width
        );
        let _ = writeln!(
            out,
            r#"<text x="{}" y="{}" text-anchor="end" class="tick">{value:.0}</text>"#,
            panel.x - 8.0,
            y + 4.0
        );
    }

    for probability_percent in [5, 20, 35, 50, 65, 80, 95] {
        let x = x_for_probability(panel, probability_percent as f64);
        let _ = writeln!(
            out,
            r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" class="grid"/>"#,
            panel.y,
            panel.y + panel.height
        );
        let _ = writeln!(
            out,
            r#"<text x="{x}" y="{}" text-anchor="middle" class="tick">{probability_percent}%</text>"#,
            panel.y + panel.height + 22.0
        );
    }

    let _ = writeln!(
        out,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="axis"/>"#,
        panel.x,
        panel.y + panel.height,
        panel.x + panel.width,
        panel.y + panel.height
    );
    let _ = writeln!(
        out,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="axis"/>"#,
        panel.x,
        panel.y,
        panel.x,
        panel.y + panel.height
    );
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" text-anchor="middle" class="tick">outer-layer probability</text>"#,
        panel.x + panel.width / 2.0,
        panel.y + panel.height + 50.0
    );
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" transform="rotate(-90 {} {})" text-anchor="middle" class="tick">required k</text>"#,
        panel.x - 56.0,
        panel.y + panel.height / 2.0,
        panel.x - 56.0,
        panel.y + panel.height / 2.0
    );

    for (index, side_length) in sizes.iter().enumerate() {
        let mut points = String::new();
        for row in rows.iter().filter(|row| row.side_length == *side_length) {
            let x = x_for_probability(panel, row.probability_percent as f64);
            let y = panel.y + panel.height
                - panel.height * row.analytic_iterations as f64 / panel.max_y;
            let _ = write!(points, "{x:.2},{y:.2} ");
        }
        let _ = writeln!(
            out,
            r#"<polyline points="{points}" fill="none" stroke="{}" stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round"/>"#,
            color_for_size_index(index)
        );
    }
}

fn render_empirical_rate_panel(
    out: &mut String,
    rows: &[Row],
    panel: Panel,
    sizes: &[usize],
    target: f64,
) {
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" class="panel-title">Empirical untouched-layer rate at required k</text>"#,
        panel.x,
        panel.y - 34.0
    );
    let _ = writeln!(
        out,
        r#"<text x="{}" y="{}" class="subtitle">Randomized layer-hit validation; dashed line is the blue-method threshold.</text>"#,
        panel.x,
        panel.y - 14.0
    );

    for tick in 0..=3 {
        let ratio = tick as f64 / 3.0;
        let y = panel.y + panel.height - panel.height * ratio;
        let value = 100.0 * panel.max_y * ratio;
        let _ = writeln!(
            out,
            r#"<line x1="{}" y1="{y}" x2="{}" y2="{y}" class="grid"/>"#,
            panel.x,
            panel.x + panel.width
        );
        let _ = writeln!(
            out,
            r#"<text x="{}" y="{}" text-anchor="end" class="tick">{value:.2}%</text>"#,
            panel.x - 8.0,
            y + 4.0
        );
    }

    for probability_percent in [5, 20, 35, 50, 65, 80, 95] {
        let x = x_for_probability(panel, probability_percent as f64);
        let _ = writeln!(
            out,
            r#"<line x1="{x}" y1="{}" x2="{x}" y2="{}" class="grid"/>"#,
            panel.y,
            panel.y + panel.height
        );
        let _ = writeln!(
            out,
            r#"<text x="{x}" y="{}" text-anchor="middle" class="tick">{probability_percent}%</text>"#,
            panel.y + panel.height + 22.0
        );
    }

    let target_y = panel.y + panel.height - panel.height * target / panel.max_y;
    let _ = writeln!(
        out,
        r##"<line x1="{}" y1="{target_y}" x2="{}" y2="{target_y}" stroke="#6b7280" stroke-width="1.5" stroke-dasharray="5 5"/>"##,
        panel.x,
        panel.x + panel.width
    );
    let _ = writeln!(
        out,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="axis"/>"#,
        panel.x,
        panel.y + panel.height,
        panel.x + panel.width,
        panel.y + panel.height
    );
    let _ = writeln!(
        out,
        r#"<line x1="{}" y1="{}" x2="{}" y2="{}" class="axis"/>"#,
        panel.x,
        panel.y,
        panel.x,
        panel.y + panel.height
    );

    for (index, side_length) in sizes.iter().enumerate() {
        let mut points = String::new();
        for row in rows.iter().filter(|row| row.side_length == *side_length) {
            let x = x_for_probability(panel, row.probability_percent as f64);
            let y =
                panel.y + panel.height - panel.height * row.empirical_unseen_rate_at / panel.max_y;
            let _ = write!(points, "{x:.2},{y:.2} ");
        }
        let _ = writeln!(
            out,
            r#"<polyline points="{points}" fill="none" stroke="{}" stroke-width="2.2" stroke-linejoin="round" stroke-linecap="round"/>"#,
            color_for_size_index(index)
        );
    }
}

fn x_for_probability(panel: Panel, probability_percent: f64) -> f64 {
    if (panel.x_max - panel.x_min).abs() < f64::EPSILON {
        return panel.x + panel.width / 2.0;
    }

    panel.x + panel.width * (probability_percent - panel.x_min) / (panel.x_max - panel.x_min)
}

fn color_for_size_index(index: usize) -> &'static str {
    match index % 6 {
        0 => "#2563eb",
        1 => "#dc2626",
        2 => "#059669",
        3 => "#7c3aed",
        4 => "#d97706",
        _ => "#0891b2",
    }
}

fn print_summary(rows: &[Row]) {
    println!("outer-layer probability sweep");
    println!("n     p*=2/n     best_grid_p  required_k  expected_unseen  empirical_unseen");

    let mut sizes = rows
        .iter()
        .map(|row| row.side_length)
        .collect::<Vec<usize>>();
    sizes.sort_unstable();
    sizes.dedup();

    for side_length in sizes {
        if let Some(row) = best_grid_row(rows, side_length) {
            println!(
                "{:<5} {:>8.4}% {:>11}% {:>11} {:>14.4}% {:>15.4}%",
                row.side_length,
                100.0 * row.balanced_probability,
                row.probability_percent,
                row.analytic_iterations,
                100.0 * row.expected_unseen_rate_at,
                100.0 * row.empirical_unseen_rate_at,
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

fn parse_usize_list(value: &str) -> Result<Vec<usize>, String> {
    let mut values = Vec::new();
    for part in value.split(',') {
        values.push(parse_positive_usize(part.trim(), "sizes")?);
    }
    if values.is_empty() {
        return Err("sizes must not be empty".to_owned());
    }
    Ok(values)
}

fn parse_probability_percent_list(value: &str) -> Result<Vec<usize>, String> {
    let mut values = Vec::new();
    for part in value.split(',') {
        let parsed = parse_positive_usize(part.trim(), "probabilities")?;
        if parsed >= 100 {
            return Err("probabilities must be percentages in 1..=99".to_owned());
        }
        values.push(parsed);
    }
    if values.is_empty() {
        return Err("probabilities must not be empty".to_owned());
    }
    Ok(values)
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
Usage: cargo run --release --bin scramble_probability_sweep -- [options]

Options:
      --sizes <LIST>          Comma-separated cube side lengths. Default: 500,1000,2000,4000
      --probabilities <LIST>  Comma-separated integer percentages. Default: 5,10,...,95
      --trials <N>            Randomized validation trials per point. Default: {DEFAULT_TRIALS}
      --seed <N>              Base seed, decimal or 0x-prefixed hex. Default: 0x{DEFAULT_SEED:016X}
      --output <PATH>         SVG graph output. Default: {DEFAULT_OUTPUT}
      --csv-output <PATH>     CSV output. Default: output path with .csv extension
      --report-output <PATH>  Text summary output. Default: {DEFAULT_REPORT}
  -h, --help                  Print this help.
"
    )
}
