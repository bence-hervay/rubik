use core::fmt;

use crate::{
    facelet::Facelet,
    line::{LineBuffer, LineKind, LineTraversal},
    matrix::Matrix,
    moves::MoveAngle,
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

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct FaceAngle(u8);

impl FaceAngle {
    pub const fn new(turns: u8) -> Self {
        Self(turns & 3)
    }

    pub const fn as_u8(self) -> u8 {
        self.0
    }

    pub const fn quarter_turns(self) -> u8 {
        self.0
    }

    pub const fn turned_by(self, angle: MoveAngle) -> Self {
        Self::new(self.0 + angle.as_u8())
    }
}

impl fmt::Display for FaceAngle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Debug)]
pub struct Face<S: FaceletArray> {
    id: FaceId,
    matrix: Matrix<S>,
    rotation: FaceAngle,
}

impl<S: FaceletArray> Face<S> {
    pub fn new(id: FaceId, n: usize, fill: Facelet) -> Self {
        Self {
            id,
            matrix: Matrix::new_filled(n, fill),
            rotation: FaceAngle::default(),
        }
    }

    pub fn from_matrix(id: FaceId, matrix: Matrix<S>) -> Self {
        Self {
            id,
            matrix,
            rotation: FaceAngle::default(),
        }
    }

    pub fn id(&self) -> FaceId {
        self.id
    }

    pub fn side_len(&self) -> usize {
        self.matrix.side_len()
    }

    pub fn rotation(&self) -> FaceAngle {
        self.rotation
    }

    pub fn set_rotation(&mut self, rotation: FaceAngle) {
        self.rotation = rotation;
    }

    pub fn rotate_meta_by(&mut self, angle: MoveAngle) {
        self.rotation = self.rotation.turned_by(angle);
    }

    pub fn matrix(&self) -> &Matrix<S> {
        &self.matrix
    }

    #[inline]
    pub fn matrix_mut(&mut self) -> &mut Matrix<S> {
        &mut self.matrix
    }

    pub fn physical_coords(&self, row: usize, col: usize) -> (usize, usize) {
        let n = self.side_len();
        assert!(row < n, "row out of bounds");
        assert!(col < n, "col out of bounds");

        match self.rotation.as_u8() {
            0 => (row, col),
            1 => (n - 1 - col, row),
            2 => (n - 1 - row, n - 1 - col),
            3 => (col, n - 1 - row),
            _ => unreachable!("face angle is always normalized"),
        }
    }

    #[inline(always)]
    pub fn logical_line_as_physical(
        &self,
        kind: LineKind,
        index: usize,
    ) -> (LineKind, usize, bool) {
        let n = self.side_len();
        assert!(index < n, "line index out of bounds");

        match (kind, self.rotation.as_u8()) {
            (LineKind::Row, 0) => (LineKind::Row, index, false),
            (LineKind::Row, 1) => (LineKind::Col, index, true),
            (LineKind::Row, 2) => (LineKind::Row, n - 1 - index, true),
            (LineKind::Row, 3) => (LineKind::Col, n - 1 - index, false),
            (LineKind::Col, 0) => (LineKind::Col, index, false),
            (LineKind::Col, 1) => (LineKind::Row, n - 1 - index, false),
            (LineKind::Col, 2) => (LineKind::Col, n - 1 - index, true),
            (LineKind::Col, 3) => (LineKind::Row, index, true),
            _ => unreachable!("face angle is always normalized"),
        }
    }

    #[inline(always)]
    pub(crate) fn line_traversal(
        &self,
        kind: LineKind,
        index: usize,
        reversed: bool,
    ) -> LineTraversal {
        let n = self.side_len();
        let (physical_kind, physical_index, physical_reversed) =
            self.logical_line_as_physical(kind, index);
        let reversed = physical_reversed ^ reversed;

        match physical_kind {
            LineKind::Row => {
                let col = if reversed { n - 1 } else { 0 };
                let start = physical_index
                    .checked_mul(n)
                    .and_then(|row_start| row_start.checked_add(col))
                    .expect("line start index overflowed usize");
                let step = if reversed { -1 } else { 1 };
                LineTraversal::new(start, step)
            }
            LineKind::Col => {
                let row = if reversed { n - 1 } else { 0 };
                let start = row
                    .checked_mul(n)
                    .and_then(|row_start| row_start.checked_add(physical_index))
                    .expect("line start index overflowed usize");
                let step = isize::try_from(n).expect("line step overflowed isize");
                let step = if reversed { -step } else { step };
                LineTraversal::new(start, step)
            }
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

    pub fn preview_string(&self) -> String {
        let mut out = String::new();

        for row in 0..self.side_len() {
            for col in 0..self.side_len() {
                if col > 0 {
                    out.push(' ');
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
            "Face({}, n={}, rotation={})",
            self.id,
            self.side_len(),
            self.rotation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::FaceAngle;
    use crate::MoveAngle;

    #[test]
    fn face_angle_is_normalized_modulo_four() {
        assert_eq!(FaceAngle::new(0).as_u8(), 0);
        assert_eq!(FaceAngle::new(1).as_u8(), 1);
        assert_eq!(FaceAngle::new(2).as_u8(), 2);
        assert_eq!(FaceAngle::new(3).as_u8(), 3);
        assert_eq!(FaceAngle::new(4).as_u8(), 0);
        assert_eq!(FaceAngle::new(7).as_u8(), 3);
    }

    #[test]
    fn face_angle_turning_is_addition_modulo_four() {
        assert_eq!(
            FaceAngle::new(0).turned_by(MoveAngle::Positive),
            FaceAngle::new(1)
        );
        assert_eq!(
            FaceAngle::new(1).turned_by(MoveAngle::Positive),
            FaceAngle::new(2)
        );
        assert_eq!(
            FaceAngle::new(2).turned_by(MoveAngle::Positive),
            FaceAngle::new(3)
        );
        assert_eq!(
            FaceAngle::new(3).turned_by(MoveAngle::Positive),
            FaceAngle::new(0)
        );
    }
}
