use core::{fmt, fmt::Write};
use std::ops::Range;

use crate::{cube::Cube, face::FaceId, storage::FaceletArray};

impl<S: FaceletArray> Cube<S> {
    pub fn net_string(&self) -> String {
        let rows = net_segments(self.n);
        let cols = net_segments(self.n);
        let rendered_rows = net_render_rows(&rows);
        let face_inner_width = net_face_inner_width(&cols);
        let mut out = String::new();

        let _ = writeln!(
            out,
            "Cube(n={}, history={}, storage~{} bytes)",
            self.n,
            self.history.len(),
            self.estimated_storage_bytes(),
        );

        self.push_net_boundary_row(
            &mut out,
            &NET_LAYOUT_EMPTY_ROW,
            &NET_LAYOUT[0],
            face_inner_width,
        );
        for rendered_row in rendered_rows.iter().copied() {
            self.push_net_content_row(
                &mut out,
                &NET_LAYOUT[0],
                rendered_row,
                &cols,
                face_inner_width,
            );
        }

        self.push_net_boundary_row(&mut out, &NET_LAYOUT[0], &NET_LAYOUT[1], face_inner_width);
        for rendered_row in rendered_rows.iter().copied() {
            self.push_net_content_row(
                &mut out,
                &NET_LAYOUT[1],
                rendered_row,
                &cols,
                face_inner_width,
            );
        }

        self.push_net_boundary_row(&mut out, &NET_LAYOUT[1], &NET_LAYOUT[2], face_inner_width);
        for rendered_row in rendered_rows.iter().copied() {
            self.push_net_content_row(
                &mut out,
                &NET_LAYOUT[2],
                rendered_row,
                &cols,
                face_inner_width,
            );
        }
        self.push_net_boundary_row(
            &mut out,
            &NET_LAYOUT[2],
            &NET_LAYOUT_EMPTY_ROW,
            face_inner_width,
        );

        out
    }

    fn push_net_boundary_row(
        &self,
        out: &mut String,
        above: &[Option<FaceId>; NET_LAYOUT_WIDTH],
        below: &[Option<FaceId>; NET_LAYOUT_WIDTH],
        face_inner_width: usize,
    ) {
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

        push_net_line(out, &line);
    }

    fn push_net_content_row(
        &self,
        out: &mut String,
        faces: &[Option<FaceId>; NET_LAYOUT_WIDTH],
        row: Option<usize>,
        cols: &[Range<usize>],
        face_inner_width: usize,
    ) {
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

        push_net_line(out, &line);
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

fn push_net_line(out: &mut String, line: &[char]) {
    let mut end = line.len();
    while end > 0 && line[end - 1] == ' ' {
        end -= 1;
    }

    for ch in &line[..end] {
        out.push(*ch);
    }
    out.push('\n');
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
