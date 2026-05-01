#![allow(dead_code)]

use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub const BACKENDS: [&str; 4] = ["byte", "nibble", "three_bit", "third_byte"];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Stage {
    Init,
    Scramble,
    Corner,
    Edge,
    Center,
    Total,
}

impl Stage {
    pub const MEASURED: [Self; 5] = [
        Self::Init,
        Self::Scramble,
        Self::Corner,
        Self::Edge,
        Self::Center,
    ];
    pub const PLOTTED: [Self; 6] = [
        Self::Init,
        Self::Scramble,
        Self::Corner,
        Self::Edge,
        Self::Center,
        Self::Total,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Scramble => "scramble",
            Self::Corner => "corner",
            Self::Edge => "edge",
            Self::Center => "center",
            Self::Total => "total",
        }
    }

    const fn color(self) -> &'static str {
        match self {
            Self::Init => "#4c78a8",
            Self::Scramble => "#f58518",
            Self::Corner => "#54a24b",
            Self::Edge => "#e45756",
            Self::Center => "#72b7b2",
            Self::Total => "#111827",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct StageTimes {
    init: f64,
    scramble: f64,
    corner: f64,
    edge: f64,
    center: f64,
    total: f64,
}

impl StageTimes {
    pub fn get(&self, stage: Stage) -> f64 {
        match stage {
            Stage::Init => self.init,
            Stage::Scramble => self.scramble,
            Stage::Corner => self.corner,
            Stage::Edge => self.edge,
            Stage::Center => self.center,
            Stage::Total => self.total,
        }
    }

    fn set(&mut self, stage: Stage, value: f64) {
        match stage {
            Stage::Init => self.init = value,
            Stage::Scramble => self.scramble = value,
            Stage::Corner => self.corner = value,
            Stage::Edge => self.edge = value,
            Stage::Center => self.center = value,
            Stage::Total => self.total = value,
        }
    }

    fn finish_total(&mut self) {
        self.total = Stage::MEASURED.iter().map(|stage| self.get(*stage)).sum();
    }
}

#[derive(Copy, Clone, Debug)]
struct PowerFit {
    intercept: f64,
    slope: f64,
}

impl PowerFit {
    fn predict(self, size: usize) -> f64 {
        (self.intercept + self.slope * (size as f64).ln()).exp()
    }
}

pub struct RunConfig<'a> {
    pub runner: &'a Path,
    pub size: usize,
    pub mode: &'a str,
    pub backend: &'a str,
    pub scramble_rounds: usize,
    pub seed: u64,
    pub thread_count: usize,
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn ensure_runner(runner: &str, build: bool) -> Result<PathBuf, String> {
    let root = repo_root();

    if build {
        run_checked(
            Command::new("cargo")
                .arg("build")
                .arg("--release")
                .arg("--bin")
                .arg("run_pipeline_no_render")
                .current_dir(&root),
        )?;
    }

    let path = resolve_path(runner);
    if !path.is_file() {
        return Err(format!(
            "runner does not exist: {}; run without --no-build or pass --runner",
            path.display()
        ));
    }

    Ok(path)
}

pub fn run_pipeline(config: RunConfig<'_>) -> Result<StageTimes, String> {
    let root = repo_root();
    let output = run_checked(
        Command::new(config.runner)
            .arg("--n")
            .arg(config.size.to_string())
            .arg("--mode")
            .arg(config.mode)
            .arg("--backend")
            .arg(config.backend)
            .arg("--scramble-rounds")
            .arg(config.scramble_rounds.to_string())
            .arg("--seed")
            .arg(config.seed.to_string())
            .arg("--threads")
            .arg(config.thread_count.to_string())
            .current_dir(root),
    )?;

    parse_pipeline_times(&output)
}

pub fn average_runs(runs: &[StageTimes]) -> Result<StageTimes, String> {
    if runs.is_empty() {
        return Err("cannot average zero runs".to_owned());
    }

    let mut averaged = StageTimes::default();
    for stage in Stage::PLOTTED {
        let total: f64 = runs.iter().map(|run| run.get(stage)).sum();
        averaged.set(stage, total / runs.len() as f64);
    }
    Ok(averaged)
}

pub fn parse_positive_usize(value: &str, name: &str) -> Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{name} must be greater than zero"));
    }
    Ok(parsed)
}

pub fn parse_non_negative_usize(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("{name} must be a non-negative integer"))
}

pub fn parse_non_negative_u64(value: &str, name: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{name} must be a non-negative integer"))
}

pub fn parse_sizes(value: &str) -> Result<Vec<usize>, String> {
    let mut sizes = Vec::new();
    for part in split_list(value) {
        sizes.push(parse_positive_usize(part, "sizes")?);
    }
    if sizes.is_empty() {
        return Err("sizes must contain at least one value".to_owned());
    }
    Ok(sizes)
}

pub fn parse_backends(value: &str) -> Result<Vec<String>, String> {
    let backends: Vec<String> = split_list(value).map(str::to_owned).collect();
    if backends.is_empty() {
        return Err("backends must contain at least one value".to_owned());
    }

    let unknown: Vec<&str> = backends
        .iter()
        .map(String::as_str)
        .filter(|backend| !BACKENDS.contains(backend))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "unknown backend(s): {}; expected one of: {}",
            unknown.join(", "),
            BACKENDS.join(", ")
        ));
    }

    Ok(backends)
}

pub fn default_sizes() -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut size = 1usize;
    while size <= 2048 {
        sizes.push(size);
        size *= 2;
    }
    sizes
}

pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers.iter().map(|header| header.len()).collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.len());
        }
    }

    for (index, header) in headers.iter().enumerate() {
        if index > 0 {
            print!("  ");
        }
        print!("{header:<width$}", width = widths[index]);
    }
    println!();

    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            print!("  ");
        }
        print!("{}", "-".repeat(*width));
    }
    println!();

    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                print!("  ");
            }
            print!("{cell:>width$}", width = widths[index]);
        }
        println!();
    }
}

pub fn format_ms(value: f64) -> String {
    if value == 0.0 {
        "0.000".to_owned()
    } else if value < 0.001 {
        format!("{value:.6}")
    } else if value < 10.0 {
        format!("{value:.3}")
    } else if value < 1000.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.0}")
    }
}

fn csv_ms(value: f64) -> String {
    format!("{value:.6}")
}

fn csv_row(csv: &mut String, cells: &[String]) {
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            csv.push(',');
        }
        csv.push_str(&csv_escape(cell));
    }
    csv.push('\n');
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

pub fn write_svg(path: &Path, svg: &str) -> Result<(), String> {
    write_text_file(path, svg)
}

pub fn write_csv(path: &Path, csv: &str) -> Result<(), String> {
    write_text_file(path, csv)
}

pub fn default_csv_output_path(output: &Path) -> PathBuf {
    output.with_extension("csv")
}

pub fn render_stages_csv(
    by_size: &[(usize, StageTimes)],
    backend: &str,
    mode: &str,
    attempts: usize,
    scramble_rounds: usize,
    seed: u64,
    thread_count: usize,
    fit_threshold: usize,
    extrapolate_to: usize,
) -> String {
    let mut csv = String::new();
    csv.push_str(
        "backend,mode,attempts,scramble_rounds,base_seed,threads,fit_threshold,extrapolate_to,n,init_ms,scramble_ms,corner_ms,edge_ms,center_ms,total_ms\n",
    );

    for (size, times) in by_size {
        csv_row(
            &mut csv,
            &[
                backend.to_owned(),
                mode.to_owned(),
                attempts.to_string(),
                scramble_rounds.to_string(),
                seed.to_string(),
                thread_count.to_string(),
                fit_threshold.to_string(),
                extrapolate_to.to_string(),
                size.to_string(),
                csv_ms(times.get(Stage::Init)),
                csv_ms(times.get(Stage::Scramble)),
                csv_ms(times.get(Stage::Corner)),
                csv_ms(times.get(Stage::Edge)),
                csv_ms(times.get(Stage::Center)),
                csv_ms(times.get(Stage::Total)),
            ],
        );
    }

    csv
}

pub fn render_backends_csv(
    by_backend: &[(String, StageTimes)],
    size: usize,
    mode: &str,
    trials: usize,
    scramble_rounds: usize,
    seed: u64,
    thread_count: usize,
) -> String {
    let mut csv = String::new();
    csv.push_str(
        "n,mode,trials,scramble_rounds,base_seed,threads,backend,init_ms,scramble_ms,corner_ms,edge_ms,center_ms,total_ms\n",
    );

    for (backend, times) in by_backend {
        csv_row(
            &mut csv,
            &[
                size.to_string(),
                mode.to_owned(),
                trials.to_string(),
                scramble_rounds.to_string(),
                seed.to_string(),
                thread_count.to_string(),
                backend.clone(),
                csv_ms(times.get(Stage::Init)),
                csv_ms(times.get(Stage::Scramble)),
                csv_ms(times.get(Stage::Corner)),
                csv_ms(times.get(Stage::Edge)),
                csv_ms(times.get(Stage::Center)),
                csv_ms(times.get(Stage::Total)),
            ],
        );
    }

    csv
}

fn write_text_file(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
    }

    fs::write(path, contents)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))
}

pub fn render_stages_svg(
    by_size: &[(usize, StageTimes)],
    backend: &str,
    mode: &str,
    attempts: usize,
    thread_count: usize,
    fit_threshold: usize,
    extrapolate_to: usize,
) -> String {
    let width = 1200.0;
    let height = 760.0;
    let margin_left = 88.0;
    let margin_right = 270.0;
    let margin_top = 72.0;
    let margin_bottom = 82.0;
    let plot_width = width - margin_left - margin_right;
    let plot_height = height - margin_top - margin_bottom;
    let measured_max = by_size.iter().map(|(size, _)| *size).max().unwrap_or(1);
    let x_min = by_size.iter().map(|(size, _)| *size).min().unwrap_or(1);
    let x_max = extrapolate_to.max(measured_max);

    let mut fits = Vec::new();
    for stage in Stage::MEASURED {
        let points: Vec<(usize, f64)> = by_size
            .iter()
            .filter(|(size, times)| *size >= fit_threshold && times.get(stage) > 0.0)
            .map(|(size, times)| (*size, times.get(stage)))
            .collect();
        if let Some(fit) = power_law_fit(&points) {
            fits.push((stage, fit));
        }
    }

    let fit_start = by_size
        .iter()
        .map(|(size, _)| *size)
        .filter(|size| *size >= fit_threshold)
        .min();
    let all_stage_fits = Stage::MEASURED
        .iter()
        .all(|stage| find_fit(&fits, *stage).is_some());

    let mut y_values: Vec<f64> = by_size
        .iter()
        .flat_map(|(_, times)| Stage::PLOTTED.map(|stage| times.get(stage)))
        .filter(|value| *value > 0.0)
        .collect();

    if let Some(start) = fit_start {
        for (_, fit) in &fits {
            for size in [start, measured_max, extrapolate_to] {
                y_values.push(fit.predict(size));
            }
        }

        if all_stage_fits {
            for size in powers_between(start, extrapolate_to) {
                y_values.push(total_fit_prediction(&fits, size));
            }
        }
    }

    if y_values.is_empty() {
        y_values.push(1.0);
    }

    let y_min_value = y_values.iter().copied().fold(f64::INFINITY, f64::min);
    let y_max_value = y_values.iter().copied().fold(0.0, f64::max);
    let (y_min, y_max) = time_axis_bounds(y_min_value, y_max_value);

    let x_pos = |size: usize| {
        let left = ((size as f64) / (x_min as f64)).ln();
        let span = ((x_max as f64) / (x_min as f64)).ln();
        margin_left + plot_width * if span == 0.0 { 0.0 } else { left / span }
    };
    let y_pos = |value: f64| {
        let bottom = (value / y_min).ln();
        let span = (y_max / y_min).ln();
        margin_top + plot_height * (1.0 - if span == 0.0 { 0.0 } else { bottom / span })
    };

    let mut svg = String::new();
    writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1200\" height=\"760\" viewBox=\"0 0 1200 760\">"
    )
    .unwrap();
    writeln!(
        svg,
        "<rect width=\"1200\" height=\"760\" fill=\"#fbfaf7\"/>"
    )
    .unwrap();
    svg_text(
        &mut svg,
        42.0,
        38.0,
        &format!(
            "Stage runtime scaling - backend={backend}, mode={mode}, attempts={attempts}, threads={thread_count}"
        ),
        TextOptions::title(),
    );
    svg_text(
        &mut svg,
        42.0,
        58.0,
        &format!(
            "Solid points: measured averages. Solid fit: n >= {fit_threshold}. Dashed: extrapolated to {extrapolate_to}."
        ),
        TextOptions::small_muted(),
    );
    writeln!(
        svg,
        "<rect x=\"{margin_left}\" y=\"{margin_top}\" width=\"{plot_width}\" height=\"{plot_height}\" fill=\"#ffffff\" stroke=\"#d1d5db\"/>"
    )
    .unwrap();

    for tick in powers_between(x_min, x_max) {
        let x = x_pos(tick);
        writeln!(
            svg,
            "<line x1=\"{x:.1}\" y1=\"{margin_top}\" x2=\"{x:.1}\" y2=\"{}\" stroke=\"#e5e7eb\"/>",
            margin_top + plot_height
        )
        .unwrap();
        writeln!(
            svg,
            "<line x1=\"{x:.1}\" y1=\"{}\" x2=\"{x:.1}\" y2=\"{}\" stroke=\"#374151\"/>",
            margin_top + plot_height,
            margin_top + plot_height + 6.0
        )
        .unwrap();
        svg_text(
            &mut svg,
            x,
            margin_top + plot_height + 24.0,
            &tick.to_string(),
            TextOptions::axis().anchor("middle"),
        );
    }

    for (tick, label) in time_axis_ticks(y_min, y_max) {
        let y = y_pos(tick);
        writeln!(
            svg,
            "<line x1=\"{margin_left}\" y1=\"{y:.1}\" x2=\"{}\" y2=\"{y:.1}\" stroke=\"#e5e7eb\"/>",
            margin_left + plot_width
        )
        .unwrap();
        writeln!(
            svg,
            "<line x1=\"{}\" y1=\"{y:.1}\" x2=\"{margin_left}\" y2=\"{y:.1}\" stroke=\"#374151\"/>",
            margin_left - 6.0
        )
        .unwrap();
        svg_text(
            &mut svg,
            margin_left - 10.0,
            y + 4.0,
            &label,
            TextOptions::axis().anchor("end"),
        );
    }

    writeln!(
        svg,
        "<line x1=\"{margin_left}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#111827\"/>",
        margin_top + plot_height,
        margin_left + plot_width,
        margin_top + plot_height
    )
    .unwrap();
    writeln!(
        svg,
        "<line x1=\"{margin_left}\" y1=\"{margin_top}\" x2=\"{margin_left}\" y2=\"{}\" stroke=\"#111827\"/>",
        margin_top + plot_height
    )
    .unwrap();
    svg_text(
        &mut svg,
        margin_left + plot_width / 2.0,
        height - 28.0,
        "cube side length n",
        TextOptions::label().anchor("middle"),
    );
    svg_text(
        &mut svg,
        20.0,
        margin_top + plot_height / 2.0,
        "time",
        TextOptions::label()
            .anchor("middle")
            .extra("transform=\"rotate(-90 20 375)\""),
    );

    if let Some(start) = fit_start {
        for stage in Stage::MEASURED {
            let Some(fit) = find_fit(&fits, stage) else {
                continue;
            };
            polyline(
                &mut svg,
                &[
                    (start, fit.predict(start)),
                    (measured_max, fit.predict(measured_max)),
                ],
                &x_pos,
                &y_pos,
                stage.color(),
                None,
            );
            if extrapolate_to > measured_max {
                polyline(
                    &mut svg,
                    &[
                        (measured_max, fit.predict(measured_max)),
                        (extrapolate_to, fit.predict(extrapolate_to)),
                    ],
                    &x_pos,
                    &y_pos,
                    stage.color(),
                    Some("7 5"),
                );
            }
        }

        if all_stage_fits {
            let points: Vec<(usize, f64)> = powers_between(measured_max, extrapolate_to)
                .into_iter()
                .map(|size| (size, total_fit_prediction(&fits, size)))
                .collect();
            polyline(
                &mut svg,
                &points,
                &x_pos,
                &y_pos,
                Stage::Total.color(),
                Some("7 5"),
            );
        }
    }

    for stage in Stage::PLOTTED {
        let points: Vec<(usize, f64)> = by_size
            .iter()
            .map(|(size, times)| (*size, times.get(stage)))
            .collect();
        polyline(&mut svg, &points, &x_pos, &y_pos, stage.color(), None);
        for (size, value) in points {
            if value <= 0.0 {
                continue;
            }
            let x = x_pos(size);
            let y = y_pos(value);
            if stage == Stage::Total {
                writeln!(
                    svg,
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"7.6\" height=\"7.6\" fill=\"{}\"/>",
                    x - 3.8,
                    y - 3.8,
                    stage.color()
                )
                .unwrap();
            } else {
                writeln!(
                    svg,
                    "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"3.4\" fill=\"{}\"/>",
                    stage.color()
                )
                .unwrap();
            }
        }
    }

    let legend_x = width - margin_right + 36.0;
    let legend_y = margin_top + 18.0;
    svg_text(
        &mut svg,
        legend_x,
        legend_y - 14.0,
        "series",
        TextOptions::legend_title(),
    );
    for (index, stage) in Stage::PLOTTED.iter().enumerate() {
        let y = legend_y + index as f64 * 25.0;
        writeln!(
            svg,
            "<line x1=\"{legend_x}\" y1=\"{y:.1}\" x2=\"{}\" y2=\"{y:.1}\" stroke=\"{}\" stroke-width=\"3\"/>",
            legend_x + 24.0,
            stage.color()
        )
        .unwrap();
        svg_text(
            &mut svg,
            legend_x + 34.0,
            y + 4.0,
            stage.name(),
            TextOptions::default(),
        );
    }

    let fit_legend_y = legend_y + Stage::PLOTTED.len() as f64 * 25.0 + 22.0;
    svg_text(
        &mut svg,
        legend_x,
        fit_legend_y,
        "fit exponents",
        TextOptions::legend_title(),
    );
    for (index, stage) in Stage::MEASURED.iter().enumerate() {
        let label = match find_fit(&fits, *stage) {
            Some(fit) => format!("{}: O(n^{:.2})", stage.name(), fit.slope),
            None => format!("{}: no fit", stage.name()),
        };
        svg_text(
            &mut svg,
            legend_x,
            fit_legend_y + 22.0 + index as f64 * 19.0,
            &label,
            TextOptions::axis(),
        );
    }

    writeln!(svg, "</svg>").unwrap();
    svg
}

pub fn render_backends_svg(
    by_backend: &[(String, StageTimes)],
    size: usize,
    mode: &str,
    trials: usize,
    thread_count: usize,
) -> String {
    let width = 1200.0;
    let height = 840.0;
    let margin_left = 70.0;
    let margin_right = 36.0;
    let margin_top = 78.0;
    let margin_bottom = 62.0;
    let gap_x = 54.0;
    let gap_y = 54.0;
    let cols = 2usize;
    let panel_width = (width - margin_left - margin_right - gap_x) / cols as f64;
    let panel_height = (height - margin_top - margin_bottom - gap_y * 2.0) / 3.0;

    let mut svg = String::new();
    writeln!(
        svg,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1200\" height=\"840\" viewBox=\"0 0 1200 840\">"
    )
    .unwrap();
    writeln!(
        svg,
        "<rect width=\"1200\" height=\"840\" fill=\"#fbfaf7\"/>"
    )
    .unwrap();
    svg_text(
        &mut svg,
        42.0,
        38.0,
        &format!(
            "Backend runtime comparison - n={size}, mode={mode}, trials={trials}, threads={thread_count}"
        ),
        TextOptions::title(),
    );
    svg_text(
        &mut svg,
        42.0,
        58.0,
        "Each panel has its own linear millisecond scale.",
        TextOptions::small_muted(),
    );

    for (stage_index, stage) in Stage::PLOTTED.iter().enumerate() {
        let col = stage_index % cols;
        let row = stage_index / cols;
        let x0 = margin_left + col as f64 * (panel_width + gap_x);
        let y0 = margin_top + row as f64 * (panel_height + gap_y);
        let max_value = by_backend
            .iter()
            .map(|(_, times)| times.get(*stage))
            .fold(0.0, f64::max);
        let y_max = nice_ceiling(max_value);
        let bar_gap = 16.0;
        let bar_width = ((panel_width - bar_gap * (by_backend.len() + 1) as f64)
            / by_backend.len() as f64)
            .max(16.0);

        writeln!(
            svg,
            "<rect x=\"{x0:.1}\" y=\"{y0:.1}\" width=\"{panel_width:.1}\" height=\"{panel_height:.1}\" fill=\"#ffffff\" stroke=\"#d1d5db\"/>"
        )
        .unwrap();
        svg_text(
            &mut svg,
            x0 + 10.0,
            y0 + 22.0,
            stage.name(),
            TextOptions::panel_title(),
        );

        for tick_index in 0..=4 {
            let value = y_max * tick_index as f64 / 4.0;
            let y = y0 + panel_height
                - (panel_height - 42.0) * if y_max == 0.0 { 0.0 } else { value / y_max };
            writeln!(
                svg,
                "<line x1=\"{x0:.1}\" y1=\"{y:.1}\" x2=\"{}\" y2=\"{y:.1}\" stroke=\"#e5e7eb\"/>",
                x0 + panel_width
            )
            .unwrap();
            svg_text(
                &mut svg,
                x0 - 8.0,
                y + 4.0,
                &format_ms(value),
                TextOptions::tick().anchor("end"),
            );
        }

        for (backend_index, (backend, times)) in by_backend.iter().enumerate() {
            let value = times.get(*stage);
            let bar_height = (panel_height - 42.0) * if y_max == 0.0 { 0.0 } else { value / y_max };
            let x = x0 + bar_gap + backend_index as f64 * (bar_width + bar_gap);
            let y = y0 + panel_height - bar_height;
            writeln!(
                svg,
                "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{bar_width:.1}\" height=\"{bar_height:.1}\" fill=\"{}\"/>",
                backend_color(backend)
            )
            .unwrap();
            svg_text(
                &mut svg,
                x + bar_width / 2.0,
                (y - 6.0).max(y0 + 38.0),
                &format_ms(value),
                TextOptions::tick().anchor("middle"),
            );
            svg_text(
                &mut svg,
                x + bar_width / 2.0,
                y0 + panel_height + 18.0,
                backend,
                TextOptions::tick().anchor("middle").extra(&format!(
                    "transform=\"rotate(28 {:.1} {:.1})\"",
                    x + bar_width / 2.0,
                    y0 + panel_height + 18.0
                )),
            );
        }
    }

    writeln!(svg, "</svg>").unwrap();
    svg
}

fn parse_pipeline_times(output: &str) -> Result<StageTimes, String> {
    let mut current_stage = None;
    let mut times = StageTimes::default();
    let mut seen = [false; 5];

    for line in output.lines().map(str::trim) {
        if let Some(stage) = finished_stage(line) {
            current_stage = Some(stage);
            continue;
        }

        let Some(stage) = current_stage else {
            continue;
        };
        let Some(value) = parse_time_ms(line)? else {
            continue;
        };

        times.set(stage, value);
        match stage {
            Stage::Init => seen[0] = true,
            Stage::Scramble => seen[1] = true,
            Stage::Corner => seen[2] = true,
            Stage::Edge => seen[3] = true,
            Stage::Center => seen[4] = true,
            Stage::Total => {}
        }
        current_stage = None;
    }

    let missing: Vec<&str> = Stage::MEASURED
        .iter()
        .zip(seen)
        .filter_map(|(stage, seen)| (!seen).then_some(stage.name()))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "could not parse stage timing(s) {} from run_pipeline_no_render output",
            missing.join(", ")
        ));
    }

    times.finish_total();
    Ok(times)
}

fn parse_time_ms(line: &str) -> Result<Option<f64>, String> {
    let Some(value) = line.strip_prefix("Time:") else {
        return Ok(None);
    };
    let Some(ms) = value.trim().strip_suffix("ms") else {
        return Ok(None);
    };

    ms.parse::<f64>()
        .map(Some)
        .map_err(|_| format!("invalid millisecond timing: {line}"))
}

fn finished_stage(line: &str) -> Option<Stage> {
    match line {
        "Finished Initialization" => Some(Stage::Init),
        "Finished Scramble" => Some(Stage::Scramble),
        "Finished Corner Reduction" => Some(Stage::Corner),
        "Finished Edge Pairing" => Some(Stage::Edge),
        "Finished Center Reduction" => Some(Stage::Center),
        _ => None,
    }
}

fn run_checked(command: &mut Command) -> Result<String, String> {
    let description = format!("{command:?}");
    let output = command
        .output()
        .map_err(|error| format!("failed to run {description}: {error}"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        let suffix = tail(&text, 4000);
        return Err(format!(
            "command failed with exit code {:?}: {description}\n{suffix}",
            output.status.code()
        ));
    }

    Ok(text)
}

fn resolve_path(value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        repo_root().join(path)
    }
}

fn split_list(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|ch: char| ch == ',' || ch.is_whitespace())
        .filter(|part| !part.is_empty())
}

fn power_law_fit(points: &[(usize, f64)]) -> Option<PowerFit> {
    let usable: Vec<(f64, f64)> = points
        .iter()
        .filter(|(size, value)| *size > 0 && *value > 0.0)
        .map(|(size, value)| ((*size as f64).ln(), value.ln()))
        .collect();
    if usable.len() < 2 {
        return None;
    }

    let x_mean = usable.iter().map(|(x, _)| x).sum::<f64>() / usable.len() as f64;
    let y_mean = usable.iter().map(|(_, y)| y).sum::<f64>() / usable.len() as f64;
    let denominator = usable
        .iter()
        .map(|(x, _)| (x - x_mean).powi(2))
        .sum::<f64>();
    if denominator == 0.0 {
        return None;
    }

    let numerator = usable
        .iter()
        .map(|(x, y)| (x - x_mean) * (y - y_mean))
        .sum::<f64>();
    let slope = numerator / denominator;
    Some(PowerFit {
        intercept: y_mean - slope * x_mean,
        slope,
    })
}

fn find_fit(fits: &[(Stage, PowerFit)], stage: Stage) -> Option<PowerFit> {
    fits.iter()
        .find_map(|(fit_stage, fit)| (*fit_stage == stage).then_some(*fit))
}

fn total_fit_prediction(fits: &[(Stage, PowerFit)], size: usize) -> f64 {
    Stage::MEASURED
        .iter()
        .filter_map(|stage| find_fit(fits, *stage))
        .map(|fit| fit.predict(size))
        .sum()
}

fn powers_between(start: usize, end: usize) -> Vec<usize> {
    if start == 0 || end == 0 {
        return Vec::new();
    }

    let mut values = Vec::new();
    let mut size = 1usize;
    while size < start {
        size *= 2;
    }
    while size <= end {
        values.push(size);
        size *= 2;
    }
    values.push(start);
    values.push(end);
    values.sort_unstable();
    values.dedup();
    values
}

fn time_axis_bounds(min_value: f64, max_value: f64) -> (f64, f64) {
    let mut min = min_value.max(f64::MIN_POSITIVE);
    let mut max = max_value.max(min);

    if min == max {
        min /= 2.0;
        max *= 2.0;
    }

    (time_tick_floor(min), time_tick_ceil(max))
}

fn time_axis_ticks(min: f64, max: f64) -> Vec<(f64, String)> {
    let mut ticks = Vec::new();

    if min < 1.0 {
        let mut power = min.log10().floor() as i32;
        while 10f64.powi(power) < 1.0 && 10f64.powi(power) <= max * 1.000_001 {
            let value = 10f64.powi(power);
            if value >= min / 1.000_001 {
                ticks.push((value, format_submillisecond_tick(value)));
            }
            power += 1;
        }
    }

    for value in canonical_time_ticks(max) {
        if value >= min / 1.000_001 && value <= max * 1.000_001 {
            ticks.push((value, format_time_tick(value)));
        }
    }

    ticks
}

fn time_tick_floor(value: f64) -> f64 {
    if value < 1.0 {
        return 10f64.powf(value.log10().floor());
    }

    let mut floor = 1.0;
    for tick in canonical_time_ticks(value) {
        if tick > value * 1.000_001 {
            break;
        }
        floor = tick;
    }
    floor
}

fn time_tick_ceil(value: f64) -> f64 {
    if value < 1.0 {
        return 10f64.powf(value.log10().ceil());
    }

    canonical_time_ticks(value)
        .into_iter()
        .find(|tick| *tick >= value / 1.000_001)
        .unwrap_or(value)
}

fn canonical_time_ticks(max: f64) -> Vec<f64> {
    const SECOND_MS: f64 = 1_000.0;
    const MINUTE_MS: f64 = 60.0 * SECOND_MS;
    const HOUR_MS: f64 = 60.0 * MINUTE_MS;
    const DAY_MS: f64 = 24.0 * HOUR_MS;

    let mut ticks = vec![
        1.0,
        10.0,
        100.0,
        SECOND_MS,
        10.0 * SECOND_MS,
        MINUTE_MS,
        10.0 * MINUTE_MS,
        HOUR_MS,
        10.0 * HOUR_MS,
        DAY_MS,
    ];

    let mut days = 10.0;
    while days * DAY_MS <= max * 1.000_001 {
        ticks.push(days * DAY_MS);
        days *= 10.0;
    }

    if ticks.last().copied().unwrap_or(DAY_MS) < max {
        ticks.push(days * DAY_MS);
    }

    ticks
}

fn format_submillisecond_tick(value: f64) -> String {
    format!("{} us", format_microseconds(value * 1_000.0))
}

fn format_microseconds(value: f64) -> String {
    if value >= 1.0 {
        format!("{value:.0}")
    } else if value >= 0.001 {
        let formatted = format!("{value:.3}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    } else {
        format!("{value:.6}")
    }
}

fn format_time_tick(value: f64) -> String {
    const SECOND_MS: f64 = 1_000.0;
    const MINUTE_MS: f64 = 60.0 * SECOND_MS;
    const HOUR_MS: f64 = 60.0 * MINUTE_MS;
    const DAY_MS: f64 = 24.0 * HOUR_MS;

    if value < SECOND_MS {
        format!("{:.0} ms", value)
    } else if value < MINUTE_MS {
        format!("{:.0} s", value / SECOND_MS)
    } else if value < HOUR_MS {
        format!("{:.0} m", value / MINUTE_MS)
    } else if value < DAY_MS {
        format!("{:.0} h", value / HOUR_MS)
    } else {
        format!("{:.0} d", value / DAY_MS)
    }
}

fn polyline(
    svg: &mut String,
    points: &[(usize, f64)],
    x_pos: &impl Fn(usize) -> f64,
    y_pos: &impl Fn(f64) -> f64,
    color: &str,
    dash: Option<&str>,
) {
    let usable: Vec<(usize, f64)> = points
        .iter()
        .copied()
        .filter(|(size, value)| *size > 0 && *value > 0.0)
        .collect();
    if usable.len() < 2 {
        return;
    }

    let mut encoded = String::new();
    for (index, (size, value)) in usable.iter().enumerate() {
        if index > 0 {
            encoded.push(' ');
        }
        write!(encoded, "{:.1},{:.1}", x_pos(*size), y_pos(*value)).unwrap();
    }

    let dash = dash
        .map(|dash| format!(" stroke-dasharray=\"{dash}\""))
        .unwrap_or_default();
    writeln!(
        svg,
        "<polyline points=\"{encoded}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2.2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{dash}/>"
    )
    .unwrap();
}

#[derive(Clone, Copy)]
struct TextOptions<'a> {
    size: usize,
    fill: &'a str,
    anchor: &'a str,
    weight: &'a str,
    extra: &'a str,
}

impl<'a> TextOptions<'a> {
    const fn default() -> Self {
        Self {
            size: 12,
            fill: "#111827",
            anchor: "start",
            weight: "400",
            extra: "",
        }
    }

    const fn title() -> Self {
        Self {
            size: 18,
            weight: "700",
            ..Self::default()
        }
    }

    const fn small_muted() -> Self {
        Self {
            size: 12,
            fill: "#4b5563",
            ..Self::default()
        }
    }

    const fn axis() -> Self {
        Self {
            size: 11,
            fill: "#374151",
            ..Self::default()
        }
    }

    const fn tick() -> Self {
        Self {
            size: 10,
            fill: "#374151",
            ..Self::default()
        }
    }

    const fn label() -> Self {
        Self {
            size: 13,
            ..Self::default()
        }
    }

    const fn legend_title() -> Self {
        Self {
            size: 13,
            weight: "700",
            ..Self::default()
        }
    }

    const fn panel_title() -> Self {
        Self {
            size: 14,
            weight: "700",
            ..Self::default()
        }
    }

    const fn anchor(mut self, anchor: &'a str) -> Self {
        self.anchor = anchor;
        self
    }

    const fn extra(mut self, extra: &'a str) -> Self {
        self.extra = extra;
        self
    }
}

fn svg_text(svg: &mut String, x: f64, y: f64, text: &str, options: TextOptions<'_>) {
    writeln!(
        svg,
        "<text x=\"{x:.1}\" y=\"{y:.1}\" font-family=\"monospace\" font-size=\"{}\" fill=\"{}\" text-anchor=\"{}\" font-weight=\"{}\" {}>{}</text>",
        options.size,
        options.fill,
        options.anchor,
        options.weight,
        options.extra,
        escape(text)
    )
    .unwrap();
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn nice_ceiling(value: f64) -> f64 {
    if value <= 0.0 {
        return 1.0;
    }

    let magnitude = 10f64.powf(value.log10().floor());
    let scaled = value / magnitude;
    let nice = if scaled <= 1.0 {
        1.0
    } else if scaled <= 2.0 {
        2.0
    } else if scaled <= 5.0 {
        5.0
    } else {
        10.0
    };
    nice * magnitude
}

fn backend_color(backend: &str) -> &'static str {
    match backend {
        "byte" => "#4c78a8",
        "nibble" => "#f58518",
        "three_bit" => "#54a24b",
        "third_byte" => "#e45756",
        _ => "#6b7280",
    }
}

fn tail(value: &str, max_chars: usize) -> String {
    let mut chars: Vec<char> = value.chars().rev().take(max_chars).collect();
    chars.reverse();
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submillisecond_time_axis_ticks_use_microseconds() {
        let labels: Vec<String> = time_axis_ticks(0.001, 100.0)
            .into_iter()
            .map(|(_, label)| label)
            .collect();

        assert_eq!(
            labels,
            ["1 us", "10 us", "100 us", "1 ms", "10 ms", "100 ms"]
        );
    }
}
