use core::fmt;

use crate::{
    facelet::Facelet,
    line::{LineBuffer, LineKind},
    matrix::Matrix,
    storage::FaceletArray,
};

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FaceId {
    U = 0,
    D = 1,
    R = 2,
    L = 3,
    F = 4,
    B = 5,
}

impl FaceId {
    pub const ALL: [Self; 6] = [Self::U, Self::D, Self::R, Self::L, Self::F, Self::B];

    pub const fn index(self) -> usize {
        self as usize
    }
}

impl fmt::Display for FaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::U => "U",
            Self::D => "D",
            Self::R => "R",
            Self::L => "L",
            Self::F => "F",
            Self::B => "B",
        };
        f.write_str(s)
    }
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub enum FaceRotation {
    #[default]
    R0 = 0,
    R90 = 1,
    R180 = 2,
    R270 = 3,
}

impl FaceRotation {
    pub const fn quarter_turns(self) -> u8 {
        self as u8
    }

    pub const fn from_quarter_turns(turns: u8) -> Self {
        match turns & 3 {
            0 => Self::R0,
            1 => Self::R90,
            2 => Self::R180,
            _ => Self::R270,
        }
    }

    pub const fn turned_cw(self) -> Self {
        Self::from_quarter_turns(self.quarter_turns() + 1)
    }

    pub const fn turned_half(self) -> Self {
        Self::from_quarter_turns(self.quarter_turns() + 2)
    }

    pub const fn turned_ccw(self) -> Self {
        Self::from_quarter_turns(self.quarter_turns() + 3)
    }

    pub const fn turned_by(self, turns: u8) -> Self {
        Self::from_quarter_turns(self.quarter_turns() + turns)
    }
}

#[derive(Clone, Debug)]
pub struct Face<S: FaceletArray> {
    id: FaceId,
    matrix: Matrix<S>,
    rotation: FaceRotation,
}

impl<S: FaceletArray> Face<S> {
    pub fn new(id: FaceId, n: usize, fill: Facelet) -> Self {
        Self {
            id,
            matrix: Matrix::new_filled(n, fill),
            rotation: FaceRotation::R0,
        }
    }

    pub fn from_matrix(id: FaceId, matrix: Matrix<S>) -> Self {
        Self {
            id,
            matrix,
            rotation: FaceRotation::R0,
        }
    }

    pub fn id(&self) -> FaceId {
        self.id
    }

    pub fn side_len(&self) -> usize {
        self.matrix.side_len()
    }

    pub fn rotation(&self) -> FaceRotation {
        self.rotation
    }

    pub fn set_rotation(&mut self, rotation: FaceRotation) {
        self.rotation = rotation;
    }

    pub fn rotate_meta_cw(&mut self) {
        self.rotation = self.rotation.turned_cw();
    }

    pub fn rotate_meta_half(&mut self) {
        self.rotation = self.rotation.turned_half();
    }

    pub fn rotate_meta_ccw(&mut self) {
        self.rotation = self.rotation.turned_ccw();
    }

    pub fn rotate_meta_by(&mut self, quarter_turns_cw: u8) {
        self.rotation = self.rotation.turned_by(quarter_turns_cw);
    }

    pub fn matrix(&self) -> &Matrix<S> {
        &self.matrix
    }

    pub fn matrix_mut(&mut self) -> &mut Matrix<S> {
        &mut self.matrix
    }

    pub fn physical_coords(&self, row: usize, col: usize) -> (usize, usize) {
        let n = self.side_len();
        assert!(row < n, "row out of bounds");
        assert!(col < n, "col out of bounds");

        match self.rotation {
            FaceRotation::R0 => (row, col),
            FaceRotation::R90 => (n - 1 - col, row),
            FaceRotation::R180 => (n - 1 - row, n - 1 - col),
            FaceRotation::R270 => (col, n - 1 - row),
        }
    }

    pub fn logical_line_as_physical(
        &self,
        kind: LineKind,
        index: usize,
    ) -> (LineKind, usize, bool) {
        let n = self.side_len();
        assert!(index < n, "line index out of bounds");

        match (kind, self.rotation) {
            (LineKind::Row, FaceRotation::R0) => (LineKind::Row, index, false),
            (LineKind::Row, FaceRotation::R90) => (LineKind::Col, index, true),
            (LineKind::Row, FaceRotation::R180) => (LineKind::Row, n - 1 - index, true),
            (LineKind::Row, FaceRotation::R270) => (LineKind::Col, n - 1 - index, false),
            (LineKind::Col, FaceRotation::R0) => (LineKind::Col, index, false),
            (LineKind::Col, FaceRotation::R90) => (LineKind::Row, n - 1 - index, false),
            (LineKind::Col, FaceRotation::R180) => (LineKind::Col, n - 1 - index, true),
            (LineKind::Col, FaceRotation::R270) => (LineKind::Row, index, true),
        }
    }

    pub fn get(&self, row: usize, col: usize) -> Facelet {
        let (pr, pc) = self.physical_coords(row, col);
        self.matrix.get(pr, pc)
    }

    pub fn set(&mut self, row: usize, col: usize, value: Facelet) {
        let (pr, pc) = self.physical_coords(row, col);
        self.matrix.set(pr, pc, value);
    }

    pub fn read_row_into(&self, row: usize, out: &mut LineBuffer) {
        let (kind, index, reversed) = self.logical_line_as_physical(LineKind::Row, row);
        self.matrix.read_line_into(kind, index, reversed, out);
    }

    pub fn write_row_from(&mut self, row: usize, src: &LineBuffer) {
        let (kind, index, reversed) = self.logical_line_as_physical(LineKind::Row, row);
        self.matrix.write_line_from(kind, index, reversed, src);
    }

    pub fn read_col_into(&self, col: usize, out: &mut LineBuffer) {
        let (kind, index, reversed) = self.logical_line_as_physical(LineKind::Col, col);
        self.matrix.read_line_into(kind, index, reversed, out);
    }

    pub fn write_col_from(&mut self, col: usize, src: &LineBuffer) {
        let (kind, index, reversed) = self.logical_line_as_physical(LineKind::Col, col);
        self.matrix.write_line_from(kind, index, reversed, src);
    }

    pub fn read_line_into(
        &self,
        kind: LineKind,
        index: usize,
        reversed: bool,
        out: &mut LineBuffer,
    ) {
        match kind {
            LineKind::Row => self.read_row_into(index, out),
            LineKind::Col => self.read_col_into(index, out),
        }

        if reversed {
            out.reverse();
        }
    }

    pub fn write_line_from(
        &mut self,
        kind: LineKind,
        index: usize,
        reversed: bool,
        src: &LineBuffer,
    ) {
        assert_eq!(src.len(), self.side_len(), "line length must match face");

        if !reversed {
            match kind {
                LineKind::Row => self.write_row_from(index, src),
                LineKind::Col => self.write_col_from(index, src),
            }
            return;
        }

        match kind {
            LineKind::Row => {
                for col in 0..self.side_len() {
                    self.set(index, col, src.as_slice()[self.side_len() - 1 - col]);
                }
            }
            LineKind::Col => {
                for row in 0..self.side_len() {
                    self.set(row, index, src.as_slice()[self.side_len() - 1 - row]);
                }
            }
        }
    }

    pub fn preview_string(&self, limit: usize) -> String {
        let limit = limit.max(1);
        let rows = crate::matrix::preview_indices(self.side_len(), limit);
        let cols = crate::matrix::preview_indices(self.side_len(), limit);
        let mut out = String::new();

        for (ri, row) in rows.iter().copied().enumerate() {
            if ri > 0 && rows[ri - 1] + 1 != row {
                out.push_str("...\n");
            }

            for (ci, col) in cols.iter().copied().enumerate() {
                if ci > 0 {
                    out.push(' ');
                }
                if ci > 0 && cols[ci - 1] + 1 != col {
                    out.push_str("... ");
                }
                out.push(self.get(row, col).as_char());
            }
            out.push('\n');
        }

        out
    }
}

impl<S: FaceletArray> fmt::Display for Face<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Face({}, n={}, rot={:?})",
            self.id,
            self.side_len(),
            self.rotation
        )
    }
}
