use crate::{face::FaceId, facelet::Facelet, moves::MoveAngle, storage::FaceletArray};

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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct LineTraversal {
    pub start: isize,
    pub step: isize,
}

impl LineTraversal {
    #[inline(always)]
    pub fn new(start: usize, step: isize) -> Self {
        Self {
            start: isize::try_from(start).expect("line start index overflowed isize"),
            step,
        }
    }
}

pub fn cycle_four_line_arrays<S: FaceletArray>(
    line0: &mut S,
    line1: &mut S,
    line2: &mut S,
    line3: &mut S,
    angle: MoveAngle,
) {
    let len = line0.len();
    assert_eq!(line1.len(), len, "line lengths must match");
    assert_eq!(line2.len(), len, "line lengths must match");
    assert_eq!(line3.len(), len, "line lengths must match");

    cycle_four_lines(
        line0,
        LineTraversal::new(0, 1),
        line1,
        LineTraversal::new(0, 1),
        line2,
        LineTraversal::new(0, 1),
        line3,
        LineTraversal::new(0, 1),
        len,
        angle,
    );
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn cycle_four_lines<S: FaceletArray>(
    storage0: &mut S,
    traversal0: LineTraversal,
    storage1: &mut S,
    traversal1: LineTraversal,
    storage2: &mut S,
    traversal2: LineTraversal,
    storage3: &mut S,
    traversal3: LineTraversal,
    len: usize,
    angle: MoveAngle,
) {
    let mut p0 = traversal0.start;
    let mut p1 = traversal1.start;
    let mut p2 = traversal2.start;
    let mut p3 = traversal3.start;

    for _ in 0..len {
        let i0 = p0 as usize;
        let i1 = p1 as usize;
        let i2 = p2 as usize;
        let i3 = p3 as usize;

        unsafe {
            // Traversals come from validated strips; raw values are only moved between storages.
            let v0 = storage0.get_unchecked_raw(i0);
            let v1 = storage1.get_unchecked_raw(i1);
            let v2 = storage2.get_unchecked_raw(i2);
            let v3 = storage3.get_unchecked_raw(i3);

            match angle {
                MoveAngle::Positive => {
                    storage0.set_unchecked_raw(i0, v3);
                    storage1.set_unchecked_raw(i1, v0);
                    storage2.set_unchecked_raw(i2, v1);
                    storage3.set_unchecked_raw(i3, v2);
                }
                MoveAngle::Double => {
                    storage0.set_unchecked_raw(i0, v2);
                    storage1.set_unchecked_raw(i1, v3);
                    storage2.set_unchecked_raw(i2, v0);
                    storage3.set_unchecked_raw(i3, v1);
                }
                MoveAngle::Negative => {
                    storage0.set_unchecked_raw(i0, v1);
                    storage1.set_unchecked_raw(i1, v2);
                    storage2.set_unchecked_raw(i2, v3);
                    storage3.set_unchecked_raw(i3, v0);
                }
            }
        }

        p0 += traversal0.step;
        p1 += traversal1.step;
        p2 += traversal2.step;
        p3 += traversal3.step;
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

#[cfg(test)]
mod tests {
    use super::cycle_four_line_arrays;
    use crate::{Byte, Facelet, FaceletArray, MoveAngle};

    fn storage(values: &[Facelet]) -> Byte {
        let mut storage = Byte::with_len(values.len(), Facelet::White);
        for (index, value) in values.iter().copied().enumerate() {
            storage.set(index, value);
        }
        storage
    }

    fn assert_storage_eq(storage: &Byte, expected: &[Facelet]) {
        assert_eq!(storage.len(), expected.len());
        for (index, value) in expected.iter().copied().enumerate() {
            assert_eq!(storage.get(index), value, "mismatch at index {index}");
        }
    }

    #[test]
    fn cycle_four_line_arrays_applies_positive_turn() {
        let mut a = storage(&[Facelet::White, Facelet::Yellow]);
        let mut b = storage(&[Facelet::Red, Facelet::Orange]);
        let mut c = storage(&[Facelet::Green, Facelet::Blue]);
        let mut d = storage(&[Facelet::Yellow, Facelet::White]);

        cycle_four_line_arrays(&mut a, &mut b, &mut c, &mut d, MoveAngle::Positive);

        assert_storage_eq(&a, &[Facelet::Yellow, Facelet::White]);
        assert_storage_eq(&b, &[Facelet::White, Facelet::Yellow]);
        assert_storage_eq(&c, &[Facelet::Red, Facelet::Orange]);
        assert_storage_eq(&d, &[Facelet::Green, Facelet::Blue]);
    }

    #[test]
    fn cycle_four_line_arrays_applies_double_turn() {
        let mut a = storage(&[Facelet::White, Facelet::Yellow]);
        let mut b = storage(&[Facelet::Red, Facelet::Orange]);
        let mut c = storage(&[Facelet::Green, Facelet::Blue]);
        let mut d = storage(&[Facelet::Yellow, Facelet::White]);

        cycle_four_line_arrays(&mut a, &mut b, &mut c, &mut d, MoveAngle::Double);

        assert_storage_eq(&a, &[Facelet::Green, Facelet::Blue]);
        assert_storage_eq(&b, &[Facelet::Yellow, Facelet::White]);
        assert_storage_eq(&c, &[Facelet::White, Facelet::Yellow]);
        assert_storage_eq(&d, &[Facelet::Red, Facelet::Orange]);
    }

    #[test]
    fn cycle_four_line_arrays_applies_negative_turn() {
        let mut a = storage(&[Facelet::White, Facelet::Yellow]);
        let mut b = storage(&[Facelet::Red, Facelet::Orange]);
        let mut c = storage(&[Facelet::Green, Facelet::Blue]);
        let mut d = storage(&[Facelet::Yellow, Facelet::White]);

        cycle_four_line_arrays(&mut a, &mut b, &mut c, &mut d, MoveAngle::Negative);

        assert_storage_eq(&a, &[Facelet::Red, Facelet::Orange]);
        assert_storage_eq(&b, &[Facelet::Green, Facelet::Blue]);
        assert_storage_eq(&c, &[Facelet::Yellow, Facelet::White]);
        assert_storage_eq(&d, &[Facelet::White, Facelet::Yellow]);
    }
}
