use crate::{
    facelet::Facelet, line::LineBuffer, storage::FaceletArray, threading::default_thread_count,
};

#[derive(Clone, Debug)]
pub struct Matrix<S: FaceletArray> {
    n: usize,
    data: S,
}

impl<S: FaceletArray> Matrix<S> {
    pub fn new_filled(n: usize, fill: Facelet) -> Self {
        Self::new_filled_with_threads(n, fill, default_thread_count())
    }

    pub fn new_filled_with_threads(n: usize, fill: Facelet, thread_count: usize) -> Self {
        assert!(n > 0, "matrix side length must be > 0");
        assert!(thread_count > 0, "thread count must be greater than zero");

        let len = n
            .checked_mul(n)
            .expect("matrix cell count overflowed usize");

        Self {
            n,
            data: S::with_len_with_threads(len, fill, thread_count),
        }
    }

    pub fn from_storage(n: usize, data: S) -> Self {
        assert!(n > 0, "matrix side length must be > 0");
        let len = n
            .checked_mul(n)
            .expect("matrix cell count overflowed usize");
        assert_eq!(len, data.len(), "storage length must equal n*n");

        Self { n, data }
    }

    pub fn side_len(&self) -> usize {
        self.n
    }

    pub fn len(&self) -> usize {
        self.n * self.n
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn storage(&self) -> &S {
        &self.data
    }

    #[inline]
    pub fn storage_mut(&mut self) -> &mut S {
        &mut self.data
    }

    pub fn index_of(&self, row: usize, col: usize) -> usize {
        assert!(row < self.n, "row out of bounds");
        assert!(col < self.n, "col out of bounds");
        row * self.n + col
    }

    pub fn get(&self, row: usize, col: usize) -> Facelet {
        let idx = self.index_of(row, col);
        self.data.get(idx)
    }

    pub fn set(&mut self, row: usize, col: usize, value: Facelet) {
        let idx = self.index_of(row, col);
        self.data.set(idx, value);
    }

    pub fn fill(&mut self, value: Facelet) {
        self.data.fill(value);
    }

    pub fn read_row_into(&self, row: usize, out: &mut LineBuffer) {
        assert_eq!(out.len(), self.n, "line length must match matrix side");
        let start = self.index_of(row, 0);
        self.data.read_block(start, out.as_mut_slice());
    }

    pub fn write_row_from(&mut self, row: usize, src: &LineBuffer) {
        assert_eq!(src.len(), self.n, "line length must match matrix side");
        let start = self.index_of(row, 0);
        self.data.write_block(start, src.as_slice());
    }

    pub fn read_col_into(&self, col: usize, out: &mut LineBuffer) {
        assert!(col < self.n, "col out of bounds");
        assert_eq!(out.len(), self.n, "line length must match matrix side");

        for row in 0..self.n {
            out.as_mut_slice()[row] = self.get(row, col);
        }
    }

    pub fn write_col_from(&mut self, col: usize, src: &LineBuffer) {
        assert!(col < self.n, "col out of bounds");
        assert_eq!(src.len(), self.n, "line length must match matrix side");

        for row in 0..self.n {
            self.set(row, col, src.as_slice()[row]);
        }
    }

    pub fn read_line_into(
        &self,
        kind: crate::line::LineKind,
        index: usize,
        reversed: bool,
        out: &mut LineBuffer,
    ) {
        match kind {
            crate::line::LineKind::Row => self.read_row_into(index, out),
            crate::line::LineKind::Col => self.read_col_into(index, out),
        }

        if reversed {
            out.reverse();
        }
    }

    pub fn write_line_from(
        &mut self,
        kind: crate::line::LineKind,
        index: usize,
        reversed: bool,
        src: &LineBuffer,
    ) {
        assert_eq!(src.len(), self.n, "line length must match matrix side");

        match (kind, reversed) {
            (crate::line::LineKind::Row, false) => self.write_row_from(index, src),
            (crate::line::LineKind::Col, false) => self.write_col_from(index, src),
            (crate::line::LineKind::Row, true) => {
                assert!(index < self.n, "row out of bounds");
                for col in 0..self.n {
                    self.set(index, col, src.as_slice()[self.n - 1 - col]);
                }
            }
            (crate::line::LineKind::Col, true) => {
                assert!(index < self.n, "col out of bounds");
                for row in 0..self.n {
                    self.set(row, index, src.as_slice()[self.n - 1 - row]);
                }
            }
        }
    }

    pub fn preview_string(&self) -> String {
        let mut out = String::new();

        for row in 0..self.n {
            for col in 0..self.n {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{line::LineKind, Byte, Byte3, Nibble, ThreeBit};

    fn assert_line_io_round_trip<S: FaceletArray>() {
        let mut matrix = Matrix::<S>::new_filled_with_threads(3, Facelet::White, 1);
        matrix.set(0, 0, Facelet::White);
        matrix.set(0, 1, Facelet::Yellow);
        matrix.set(0, 2, Facelet::Red);
        matrix.set(1, 0, Facelet::Orange);
        matrix.set(1, 1, Facelet::Green);
        matrix.set(1, 2, Facelet::Blue);
        matrix.set(2, 0, Facelet::Red);
        matrix.set(2, 1, Facelet::Blue);
        matrix.set(2, 2, Facelet::White);

        let mut line = LineBuffer::with_len(3, Facelet::White);
        matrix.read_line_into(LineKind::Row, 1, false, &mut line);
        assert_eq!(
            line.as_slice(),
            &[Facelet::Orange, Facelet::Green, Facelet::Blue]
        );

        matrix.read_line_into(LineKind::Col, 2, true, &mut line);
        assert_eq!(
            line.as_slice(),
            &[Facelet::White, Facelet::Blue, Facelet::Red]
        );

        let src = LineBuffer::with_len(3, Facelet::Yellow);
        matrix.write_line_from(LineKind::Row, 2, false, &src);
        assert_eq!(matrix.get(2, 0), Facelet::Yellow);
        assert_eq!(matrix.get(2, 1), Facelet::Yellow);
        assert_eq!(matrix.get(2, 2), Facelet::Yellow);

        let mut reversed = LineBuffer::with_len(3, Facelet::White);
        reversed
            .as_mut_slice()
            .copy_from_slice(&[Facelet::Green, Facelet::Blue, Facelet::Orange]);
        matrix.write_line_from(LineKind::Col, 0, true, &reversed);
        assert_eq!(matrix.get(0, 0), Facelet::Orange);
        assert_eq!(matrix.get(1, 0), Facelet::Blue);
        assert_eq!(matrix.get(2, 0), Facelet::Green);
    }

    #[test]
    fn matrix_line_io_works_across_storage_backends() {
        assert_line_io_round_trip::<Byte>();
        assert_line_io_round_trip::<Byte3>();
        assert_line_io_round_trip::<Nibble>();
        assert_line_io_round_trip::<ThreeBit>();
    }

    #[test]
    fn matrix_from_storage_fill_and_preview_follow_row_major_layout() {
        let mut storage = Byte::with_len_with_threads(4, Facelet::White, 1);
        storage.set(0, Facelet::White);
        storage.set(1, Facelet::Yellow);
        storage.set(2, Facelet::Red);
        storage.set(3, Facelet::Orange);

        let mut matrix = Matrix::from_storage(2, storage);
        assert_eq!(matrix.get(0, 0), Facelet::White);
        assert_eq!(matrix.get(0, 1), Facelet::Yellow);
        assert_eq!(matrix.get(1, 0), Facelet::Red);
        assert_eq!(matrix.get(1, 1), Facelet::Orange);
        assert_eq!(matrix.preview_string(), "W Y\nR O\n");

        matrix.fill(Facelet::Blue);
        assert_eq!(matrix.preview_string(), "B B\nB B\n");
    }
}
