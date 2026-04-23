use core::{fmt, fmt::Write};
use std::ops::Range;

use crate::{cube::Cube, face::FaceId, storage::FaceletArray, Facelet};

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum NetColorScheme {
    #[default]
    None,
    Standard,
    Cube,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum NetBorderStyle {
    #[default]
    Ascii,
    Unicode,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum NetTextWeight {
    #[default]
    Normal,
    Bold,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum NetBackground {
    #[default]
    None,
    Facelets,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct NetRenderOptions {
    pub colors: NetColorScheme,
    pub borders: NetBorderStyle,
    pub text_weight: NetTextWeight,
    pub background: NetBackground,
}

impl Default for NetRenderOptions {
    fn default() -> Self {
        Self::plain_ascii()
    }
}

impl NetRenderOptions {
    pub const fn plain_ascii() -> Self {
        Self {
            colors: NetColorScheme::None,
            borders: NetBorderStyle::Ascii,
            text_weight: NetTextWeight::Normal,
            background: NetBackground::None,
        }
    }

    pub const fn terminal_pretty() -> Self {
        Self {
            colors: NetColorScheme::Standard,
            borders: NetBorderStyle::Unicode,
            text_weight: NetTextWeight::Bold,
            background: NetBackground::None,
        }
    }
}

impl<S: FaceletArray> Cube<S> {
    pub fn net_string(&self) -> String {
        self.net_string_with_options(NetRenderOptions::plain_ascii())
    }

    pub fn net_string_with_options(&self, options: NetRenderOptions) -> String {
        let rows = net_segments(self.n);
        let cols = net_segments(self.n);
        let rendered_rows = net_render_rows(&rows);
        let face_inner_width = net_face_inner_width(&cols);
        let mut canvas = Vec::with_capacity(rendered_rows.len() * NET_LAYOUT.len() + 4);

        canvas.push(self.net_boundary_row(&NET_LAYOUT_EMPTY_ROW, &NET_LAYOUT[0], face_inner_width));
        for rendered_row in rendered_rows.iter().copied() {
            canvas.push(self.net_content_row(
                &NET_LAYOUT[0],
                rendered_row,
                &cols,
                face_inner_width,
            ));
        }

        canvas.push(self.net_boundary_row(&NET_LAYOUT[0], &NET_LAYOUT[1], face_inner_width));
        for rendered_row in rendered_rows.iter().copied() {
            canvas.push(self.net_content_row(
                &NET_LAYOUT[1],
                rendered_row,
                &cols,
                face_inner_width,
            ));
        }

        canvas.push(self.net_boundary_row(&NET_LAYOUT[1], &NET_LAYOUT[2], face_inner_width));
        for rendered_row in rendered_rows.iter().copied() {
            canvas.push(self.net_content_row(
                &NET_LAYOUT[2],
                rendered_row,
                &cols,
                face_inner_width,
            ));
        }
        canvas.push(self.net_boundary_row(&NET_LAYOUT[2], &NET_LAYOUT_EMPTY_ROW, face_inner_width));

        let canvas = if options.borders == NetBorderStyle::Unicode {
            unicode_border_canvas(&canvas)
        } else {
            canvas
        };

        let mut out = String::new();
        let _ = writeln!(
            out,
            "Cube(n={}, history={}, storage~{} bytes)",
            self.n,
            self.history.len(),
            self.estimated_storage_bytes(),
        );

        for line in &canvas {
            push_net_line(&mut out, line, options);
        }

        out
    }

    fn net_boundary_row(
        &self,
        above: &[Option<FaceId>; NET_LAYOUT_WIDTH],
        below: &[Option<FaceId>; NET_LAYOUT_WIDTH],
        face_inner_width: usize,
    ) -> Vec<char> {
        let mut line = vec![' '; net_canvas_width(face_inner_width)];

        for col in 0..NET_LAYOUT_WIDTH {
            if above[col].is_none() && below[col].is_none() {
                continue;
            }
            let start = net_face_start(col, face_inner_width);
            line[start] = '+';
            for slot in &mut line[start + 1..start + face_inner_width + 1] {
                *slot = '-';
            }
            line[start + face_inner_width + 1] = '+';
        }

        line
    }

    fn net_content_row(
        &self,
        faces: &[Option<FaceId>; NET_LAYOUT_WIDTH],
        row: Option<usize>,
        cols: &[Range<usize>],
        face_inner_width: usize,
    ) -> Vec<char> {
        let mut line = vec![' '; net_canvas_width(face_inner_width)];

        for (col, face) in faces.iter().copied().enumerate() {
            let Some(face) = face else {
                continue;
            };

            let start = net_face_start(col, face_inner_width);
            line[start] = '|';
            line[start + face_inner_width + 1] = '|';

            let Some(row) = row else {
                continue;
            };

            let mut x = start + 2;
            for (col_group_index, col_group) in cols.iter().enumerate() {
                if col_group_index > 0 {
                    x += NET_LAYER_GAP.len();
                }

                for (col_index, col) in col_group.clone().enumerate() {
                    if col_index > 0 {
                        x += 1;
                    }
                    line[x] = self.face(face).get(row, col).as_char();
                    x += 1;
                }
            }
        }

        line
    }
}

const NET_LAYOUT_WIDTH: usize = 4;
const NET_LAYOUT_EMPTY_ROW: [Option<FaceId>; NET_LAYOUT_WIDTH] = [None; NET_LAYOUT_WIDTH];
const NET_LAYOUT: [[Option<FaceId>; NET_LAYOUT_WIDTH]; 3] = [
    [None, Some(FaceId::U), None, None],
    [
        Some(FaceId::L),
        Some(FaceId::F),
        Some(FaceId::R),
        Some(FaceId::B),
    ],
    [None, Some(FaceId::D), None, None],
];
const NET_LAYER_GAP: &str = "   ";
const NET_FULL_FACE_LIMIT: usize = 7;
const NET_OUTER_LAYER_COUNT: usize = 2;

fn net_segments(n: usize) -> Vec<Range<usize>> {
    if n <= NET_FULL_FACE_LIMIT {
        return vec![0..n];
    }

    let middle_count = if n % 2 == 0 { 2 } else { 3 };
    let middle_start = (n - middle_count) / 2;

    vec![
        0..NET_OUTER_LAYER_COUNT,
        middle_start..middle_start + middle_count,
        n - NET_OUTER_LAYER_COUNT..n,
    ]
}

fn net_render_rows(rows: &[Range<usize>]) -> Vec<Option<usize>> {
    let separator_count = rows.len().saturating_sub(1);
    let total_rows = rows
        .iter()
        .map(|row_group| row_group.end - row_group.start)
        .sum::<usize>()
        + separator_count;
    let mut rendered_rows = Vec::with_capacity(total_rows);

    for (row_group_index, row_group) in rows.iter().enumerate() {
        if row_group_index > 0 {
            rendered_rows.push(None);
        }
        rendered_rows.extend(row_group.clone().map(Some));
    }

    rendered_rows
}

fn net_content_width(cols: &[Range<usize>]) -> usize {
    cols.iter().map(net_segment_width).sum::<usize>()
        + NET_LAYER_GAP.len() * cols.len().saturating_sub(1)
}

fn net_face_inner_width(cols: &[Range<usize>]) -> usize {
    net_content_width(cols) + 2
}

fn net_face_stride(face_inner_width: usize) -> usize {
    face_inner_width + 1
}

fn net_face_start(col: usize, face_inner_width: usize) -> usize {
    col * net_face_stride(face_inner_width)
}

fn net_canvas_width(face_inner_width: usize) -> usize {
    NET_LAYOUT_WIDTH * net_face_stride(face_inner_width) + 1
}

fn net_segment_width(segment: &Range<usize>) -> usize {
    let len = segment.end.saturating_sub(segment.start);
    len.saturating_add(len.saturating_sub(1))
}

fn unicode_border_canvas(canvas: &[Vec<char>]) -> Vec<Vec<char>> {
    let mut unicode = canvas.to_vec();

    for row in 0..canvas.len() {
        for col in 0..canvas[row].len() {
            if !is_ascii_border(canvas[row][col]) {
                continue;
            }
            unicode[row][col] = unicode_border_char(canvas, row, col);
        }
    }

    unicode
}

fn unicode_border_char(canvas: &[Vec<char>], row: usize, col: usize) -> char {
    let current = canvas[row][col];
    let up = border_connects_vertical(current)
        && row > 0
        && border_connects_vertical(canvas[row - 1][col]);
    let down = border_connects_vertical(current)
        && row + 1 < canvas.len()
        && border_connects_vertical(canvas[row + 1][col]);
    let left = border_connects_horizontal(current)
        && col > 0
        && border_connects_horizontal(canvas[row][col - 1]);
    let right = border_connects_horizontal(current)
        && col + 1 < canvas[row].len()
        && border_connects_horizontal(canvas[row][col + 1]);

    match (up, right, down, left) {
        (false, true, false, true) => '─',
        (true, false, true, false) => '│',
        (false, true, true, false) => '┌',
        (false, false, true, true) => '┐',
        (true, true, false, false) => '└',
        (true, false, false, true) => '┘',
        (false, true, true, true) => '┬',
        (true, true, false, true) => '┴',
        (true, true, true, false) => '├',
        (true, false, true, true) => '┤',
        (true, true, true, true) => '┼',
        (false, true, false, false) | (false, false, false, true) => '─',
        (true, false, false, false) | (false, false, true, false) => '│',
        _ => current,
    }
}

fn is_ascii_border(ch: char) -> bool {
    matches!(ch, '+' | '-' | '|')
}

fn border_connects_horizontal(ch: char) -> bool {
    matches!(ch, '+' | '-')
}

fn border_connects_vertical(ch: char) -> bool {
    matches!(ch, '+' | '|')
}

fn push_net_line(out: &mut String, line: &[char], options: NetRenderOptions) {
    let mut end = line.len();
    while end > 0 && line[end - 1] == ' ' {
        end -= 1;
    }

    for ch in &line[..end] {
        push_render_char(out, *ch, options);
    }
    out.push('\n');
}

fn push_render_char(out: &mut String, ch: char, options: NetRenderOptions) {
    if let Some(facelet) = facelet_for_render_char(ch) {
        push_facelet(out, facelet, options);
        return;
    }

    out.push(ch);
}

fn facelet_for_render_char(ch: char) -> Option<Facelet> {
    match ch {
        'W' => Some(Facelet::White),
        'Y' => Some(Facelet::Yellow),
        'R' => Some(Facelet::Red),
        'O' => Some(Facelet::Orange),
        'G' => Some(Facelet::Green),
        'B' => Some(Facelet::Blue),
        _ => None,
    }
}

fn push_facelet(out: &mut String, facelet: Facelet, options: NetRenderOptions) {
    let uses_style = options.text_weight == NetTextWeight::Bold
        || options.colors != NetColorScheme::None
        || options.background == NetBackground::Facelets;
    if !uses_style {
        out.push(facelet.as_char());
        return;
    }

    out.push_str("\x1b[");
    let mut wrote_style = false;

    if options.text_weight == NetTextWeight::Bold {
        push_style_code(out, &mut wrote_style, "1");
    }

    match options.background {
        NetBackground::None => {
            push_palette_fg(out, &mut wrote_style, options.colors, facelet);
        }
        NetBackground::Facelets => {
            push_style_code(out, &mut wrote_style, contrast_fg_code(facelet));
            push_palette_bg(
                out,
                &mut wrote_style,
                effective_background_palette(options.colors),
                facelet,
            );
        }
    }

    if !wrote_style {
        out.push(facelet.as_char());
        return;
    }

    out.push('m');
    out.push(facelet.as_char());
    out.push_str("\x1b[0m");
}

fn effective_background_palette(colors: NetColorScheme) -> NetColorScheme {
    match colors {
        NetColorScheme::None => NetColorScheme::Standard,
        colors => colors,
    }
}

fn push_style_code(out: &mut String, wrote_style: &mut bool, code: &str) {
    if *wrote_style {
        out.push(';');
    }
    out.push_str(code);
    *wrote_style = true;
}

fn push_palette_fg(
    out: &mut String,
    wrote_style: &mut bool,
    colors: NetColorScheme,
    facelet: Facelet,
) {
    match colors {
        NetColorScheme::None => {}
        NetColorScheme::Standard => push_style_code(out, wrote_style, standard_fg_code(facelet)),
        NetColorScheme::Cube => {
            push_palette_code(out, wrote_style, "38", cube_palette_index(facelet))
        }
    }
}

fn push_palette_bg(
    out: &mut String,
    wrote_style: &mut bool,
    colors: NetColorScheme,
    facelet: Facelet,
) {
    match colors {
        NetColorScheme::None => {}
        NetColorScheme::Standard => push_style_code(out, wrote_style, standard_bg_code(facelet)),
        NetColorScheme::Cube => {
            push_palette_code(out, wrote_style, "48", cube_palette_index(facelet))
        }
    }
}

fn push_palette_code(out: &mut String, wrote_style: &mut bool, prefix: &str, value: u8) {
    if *wrote_style {
        out.push(';');
    }
    let _ = write!(out, "{prefix};5;{value}");
    *wrote_style = true;
}

fn standard_fg_code(facelet: Facelet) -> &'static str {
    match facelet {
        Facelet::White => "97",
        Facelet::Yellow => "93",
        Facelet::Red => "91",
        Facelet::Orange => "33",
        Facelet::Green => "92",
        Facelet::Blue => "94",
    }
}

fn standard_bg_code(facelet: Facelet) -> &'static str {
    match facelet {
        Facelet::White => "107",
        Facelet::Yellow => "103",
        Facelet::Red => "101",
        Facelet::Orange => "43",
        Facelet::Green => "102",
        Facelet::Blue => "104",
    }
}

fn cube_palette_index(facelet: Facelet) -> u8 {
    match facelet {
        Facelet::White => 15,
        Facelet::Yellow => 226,
        Facelet::Red => 196,
        Facelet::Orange => 208,
        Facelet::Green => 46,
        Facelet::Blue => 27,
    }
}

fn contrast_fg_code(facelet: Facelet) -> &'static str {
    match facelet {
        Facelet::White | Facelet::Yellow | Facelet::Orange | Facelet::Green => "30",
        Facelet::Red | Facelet::Blue => "97",
    }
}

impl<S: FaceletArray> fmt::Display for Cube<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Cube(n={}, history={}, storage~{} bytes)",
            self.n,
            self.history.len(),
            self.estimated_storage_bytes()
        )?;
        for id in FaceId::ALL {
            writeln!(f, "  {}", self.face(id))?;
        }
        Ok(())
    }
}
