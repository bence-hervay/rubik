use crate::{facelet::Facelet, line::LineBuffer, storage::FaceletArray};

#[derive(Clone, Debug)]
pub struct Matrix<S: FaceletArray> {
    n: usize,
    data: S,
}

impl<S: FaceletArray> Matrix<S> {
    pub fn new_filled(n: usize, fill: Facelet) -> Self {
        assert!(n > 0, "matrix side length must be > 0");

        let len = n
            .checked_mul(n)
            .expect("matrix cell count overflowed usize");

        Self {
            n,
            data: S::with_len(len, fill),
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

    pub fn preview_string(&self, limit: usize) -> String {
        let limit = limit.max(1);
        let rows = preview_indices(self.n, limit);
        let cols = preview_indices(self.n, limit);
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

pub(crate) fn preview_indices(n: usize, limit: usize) -> Vec<usize> {
    if n <= limit {
        return (0..n).collect();
    }

    let head = limit.div_ceil(2);
    let tail = limit - head;
    let mut indices = Vec::with_capacity(limit);
    indices.extend(0..head);
    indices.extend(n - tail..n);
    indices
}
