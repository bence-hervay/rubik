use crate::{face::FaceId, facelet::Facelet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineBuffer {
    data: Vec<Facelet>,
}

impl LineBuffer {
    pub fn with_len(len: usize, fill: Facelet) -> Self {
        Self {
            data: vec![fill; len],
        }
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn as_slice(&self) -> &[Facelet] {
        &self.data
    }

    pub fn as_mut_slice(&mut self) -> &mut [Facelet] {
        &mut self.data
    }

    pub fn reverse(&mut self) {
        self.data.reverse();
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum LineKind {
    Row,
    Col,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StripSpec {
    pub face: FaceId,
    pub kind: LineKind,
    pub index: usize,
    pub reversed: bool,
}

impl StripSpec {
    pub const fn row(face: FaceId, index: usize, reversed: bool) -> Self {
        Self {
            face,
            kind: LineKind::Row,
            index,
            reversed,
        }
    }

    pub const fn col(face: FaceId, index: usize, reversed: bool) -> Self {
        Self {
            face,
            kind: LineKind::Col,
            index,
            reversed,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MoveScratch {
    pub a: LineBuffer,
    pub b: LineBuffer,
    pub c: LineBuffer,
    pub d: LineBuffer,
}

impl MoveScratch {
    pub fn new(line_len: usize) -> Self {
        let blank = Facelet::White;
        Self {
            a: LineBuffer::with_len(line_len, blank),
            b: LineBuffer::with_len(line_len, blank),
            c: LineBuffer::with_len(line_len, blank),
            d: LineBuffer::with_len(line_len, blank),
        }
    }

    pub fn line_len(&self) -> usize {
        self.a.len()
    }
}
