use core::{fmt, fmt::Write};

use super::*;

impl<S: FaceletArray> Cube<S> {
    pub fn net_string(&self) -> String {
        let rows = net_layers(self.n);
        let cols = net_layers(self.n);
        let face_width = net_face_width(&cols);
        let middle_indent = " ".repeat(face_width + NET_FACE_GAP.len());
        let mut out = String::new();

        let _ = writeln!(
            out,
            "Cube(n={}, history={}, storage~{} bytes)",
            self.n,
            self.history.len(),
            self.estimated_storage_bytes(),
        );

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            |out| out.push_str(&middle_indent),
            &[FaceId::U],
        );
        out.push('\n');

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            |_| {},
            &[FaceId::L, FaceId::F, FaceId::R, FaceId::B],
        );
        out.push('\n');

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            |out| out.push_str(&middle_indent),
            &[FaceId::D],
        );

        out
    }

    fn push_net_face_block(
        &self,
        out: &mut String,
        rows: &[NetLayer],
        cols: &[NetLayer],
        mut push_prefix: impl FnMut(&mut String),
        faces: &[FaceId],
    ) {
        for row in rows.iter().copied() {
            push_prefix(out);
            for (face_index, face) in faces.iter().copied().enumerate() {
                if face_index > 0 {
                    out.push_str(NET_FACE_GAP);
                }
                self.push_net_face_row(out, face, row, cols);
            }
            out.push('\n');
        }
    }

    fn push_net_face_row(&self, out: &mut String, face: FaceId, row: NetLayer, cols: &[NetLayer]) {
        for (col_index, col) in cols.iter().copied().enumerate() {
            if col_index > 0 {
                out.push(' ');
            }
            match (row, col) {
                (NetLayer::Index(row), NetLayer::Index(col)) => {
                    out.push(self.face(face).get(row, col).as_char());
                }
                (NetLayer::Separator, _) | (_, NetLayer::Separator) => out.push('-'),
            }
        }
    }
}

const NET_FACE_GAP: &str = "   ";

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum NetLayer {
    Index(usize),
    Separator,
}

fn net_layers(n: usize) -> Vec<NetLayer> {
    if n <= 8 {
        return (0..n).map(NetLayer::Index).collect();
    }

    let mut layers = Vec::with_capacity(9);
    layers.extend((0..4).map(NetLayer::Index));
    layers.push(NetLayer::Separator);
    layers.extend((n - 4..n).map(NetLayer::Index));
    layers
}

fn net_face_width(cols: &[NetLayer]) -> usize {
    cols.len().saturating_add(cols.len().saturating_sub(1))
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
