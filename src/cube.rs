use core::fmt;
use core::fmt::Write;

use crate::{
    face::{Face, FaceId},
    facelet::Facelet,
    geometry,
    history::MoveHistory,
    line::{LineBuffer, MoveScratch, StripSpec},
    moves::{Axis, Move, TurnAmount},
    random::RandomSource,
    storage::FaceletArray,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ColorScheme {
    pub u: Facelet,
    pub d: Facelet,
    pub r: Facelet,
    pub l: Facelet,
    pub f: Facelet,
    pub b: Facelet,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            u: Facelet::White,
            d: Facelet::Yellow,
            r: Facelet::Red,
            l: Facelet::Orange,
            f: Facelet::Green,
            b: Facelet::Blue,
        }
    }
}

impl ColorScheme {
    pub const fn color_of(self, face: FaceId) -> Facelet {
        match face {
            FaceId::U => self.u,
            FaceId::D => self.d,
            FaceId::R => self.r,
            FaceId::L => self.l,
            FaceId::F => self.f,
            FaceId::B => self.b,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Cube<S: FaceletArray> {
    n: usize,
    faces: [Face<S>; 6],
    history: MoveHistory,
    scratch: MoveScratch,
}

impl<S: FaceletArray> Cube<S> {
    pub fn new_solved(n: usize) -> Self {
        Self::new_with_scheme(n, ColorScheme::default())
    }

    pub fn new_with_scheme(n: usize, scheme: ColorScheme) -> Self {
        assert!(n > 0, "cube side length must be > 0");

        Self {
            n,
            faces: [
                Face::new(FaceId::U, n, scheme.u),
                Face::new(FaceId::D, n, scheme.d),
                Face::new(FaceId::R, n, scheme.r),
                Face::new(FaceId::L, n, scheme.l),
                Face::new(FaceId::F, n, scheme.f),
                Face::new(FaceId::B, n, scheme.b),
            ],
            history: MoveHistory::new(),
            scratch: MoveScratch::new(n),
        }
    }

    pub fn side_len(&self) -> usize {
        self.n
    }

    pub fn face(&self, id: FaceId) -> &Face<S> {
        &self.faces[id.index()]
    }

    pub fn face_mut(&mut self, id: FaceId) -> &mut Face<S> {
        &mut self.faces[id.index()]
    }

    pub fn faces(&self) -> &[Face<S>; 6] {
        &self.faces
    }

    pub fn history(&self) -> &MoveHistory {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn apply_move(&mut self, mv: Move) {
        self.apply_move_untracked(mv);
        self.history.push(mv);
    }

    pub fn apply_move_untracked(&mut self, mv: Move) {
        self.validate_move(mv);
        let specs = self.plan_move(mv);
        self.rotate_outer_face_meta(mv);
        self.apply_side_cycle(specs, mv.amount);
    }

    pub fn apply_moves<I>(&mut self, moves: I)
    where
        I: IntoIterator<Item = Move>,
    {
        for mv in moves {
            self.apply_move(mv);
        }
    }

    pub fn apply_moves_untracked<I>(&mut self, moves: I)
    where
        I: IntoIterator<Item = Move>,
    {
        for mv in moves {
            self.apply_move_untracked(mv);
        }
    }

    pub fn plan_move(&self, mv: Move) -> [StripSpec; 4] {
        self.validate_move(mv);
        geometry::plan_positive_quarter_turn(mv.axis, mv.depth, self.n)
    }

    pub fn rotate_outer_face_meta(&mut self, mv: Move) {
        self.validate_move(mv);

        if mv.depth == self.n - 1 {
            let face = geometry::positive_axis_face(mv.axis);
            self.faces[face.index()].rotate_meta_by(mv.amount.quarter_turns());
        }

        if mv.depth == 0 {
            let face = geometry::negative_axis_face(mv.axis);
            self.faces[face.index()].rotate_meta_by(mv.amount.inverse().quarter_turns());
        }
    }

    pub fn random_move<R: RandomSource>(&self, rng: &mut R) -> Move {
        let axis = match (rng.next_u64() % 3) as u8 {
            0 => Axis::X,
            1 => Axis::Y,
            _ => Axis::Z,
        };

        let depth = (rng.next_u64() as usize) % self.n;

        let amount = match (rng.next_u64() % 3) as u8 {
            0 => TurnAmount::Cw,
            1 => TurnAmount::Half,
            _ => TurnAmount::Ccw,
        };

        Move::new(axis, depth, amount)
    }

    pub fn scramble<R: RandomSource>(&mut self, rng: &mut R, count: usize) {
        for _ in 0..count {
            let mv = self.random_move(rng);
            self.apply_move(mv);
        }
    }

    pub fn is_solved(&self) -> bool {
        for id in FaceId::ALL {
            let face = self.face(id);
            let target = face.get(0, 0);

            for row in 0..self.n {
                for col in 0..self.n {
                    if face.get(row, col) != target {
                        return false;
                    }
                }
            }
        }

        true
    }

    pub fn estimated_storage_bytes(&self) -> usize {
        let facelets = self
            .n
            .checked_mul(self.n)
            .and_then(|cells| cells.checked_mul(6))
            .expect("cube facelet count overflowed usize");
        facelets
            .checked_mul(S::bits_per_facelet())
            .and_then(|bits| bits.checked_add(7))
            .map(|bits| bits / 8)
            .expect("cube storage estimate overflowed usize")
    }

    pub fn preview_net_string(&self, limit: usize) -> String {
        let limit = limit.max(1);
        let rows = crate::matrix::preview_indices(self.n, limit);
        let cols = crate::matrix::preview_indices(self.n, limit);
        let face_width = preview_face_width(&cols);
        let middle_indent = " ".repeat(face_width + NET_FACE_GAP.len());
        let mut previous_row = None;
        let mut out = String::new();

        let _ = writeln!(
            out,
            "Cube(n={}, history={}, storage~{} bytes, limit={})",
            self.n,
            self.history.len(),
            self.estimated_storage_bytes(),
            limit
        );

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            face_width,
            |out| out.push_str(&middle_indent),
            &[FaceId::U],
            &mut previous_row,
        );
        out.push('\n');
        previous_row = None;

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            face_width,
            |_| {},
            &[FaceId::L, FaceId::F, FaceId::R, FaceId::B],
            &mut previous_row,
        );
        out.push('\n');
        previous_row = None;

        self.push_net_face_block(
            &mut out,
            &rows,
            &cols,
            face_width,
            |out| out.push_str(&middle_indent),
            &[FaceId::D],
            &mut previous_row,
        );

        out
    }

    fn push_net_face_block(
        &self,
        out: &mut String,
        rows: &[usize],
        cols: &[usize],
        face_width: usize,
        mut push_prefix: impl FnMut(&mut String),
        faces: &[FaceId],
        previous_row: &mut Option<usize>,
    ) {
        for row in rows.iter().copied() {
            if previous_row.is_some_and(|previous| previous + 1 != row) {
                push_prefix(out);
                push_face_gap_row(out, face_width, faces.len());
                out.push('\n');
            }

            push_prefix(out);
            for (face_index, face) in faces.iter().copied().enumerate() {
                if face_index > 0 {
                    out.push_str(NET_FACE_GAP);
                }
                self.push_net_face_row(out, face, row, cols);
            }
            out.push('\n');
            *previous_row = Some(row);
        }
    }

    fn push_net_face_row(&self, out: &mut String, face: FaceId, row: usize, cols: &[usize]) {
        for (col_index, col) in cols.iter().copied().enumerate() {
            if col_index > 0 {
                out.push(' ');
            }
            if col_index > 0 && cols[col_index - 1] + 1 != col {
                out.push_str("... ");
            }
            out.push(self.face(face).get(row, col).as_char());
        }
    }

    fn validate_move(&self, mv: Move) {
        assert!(mv.depth < self.n, "move depth out of bounds");
    }

    fn read_spec(faces: &[Face<S>; 6], spec: StripSpec, out: &mut LineBuffer) {
        faces[spec.face.index()].read_line_into(spec.kind, spec.index, spec.reversed, out);
    }

    fn write_spec(faces: &mut [Face<S>; 6], spec: StripSpec, src: &LineBuffer) {
        faces[spec.face.index()].write_line_from(spec.kind, spec.index, spec.reversed, src);
    }

    fn apply_side_cycle(&mut self, specs: [StripSpec; 4], amount: TurnAmount) {
        let faces = &mut self.faces;
        let scratch = &mut self.scratch;

        Self::read_spec(faces, specs[0], &mut scratch.a);
        Self::read_spec(faces, specs[1], &mut scratch.b);
        Self::read_spec(faces, specs[2], &mut scratch.c);
        Self::read_spec(faces, specs[3], &mut scratch.d);

        match amount {
            TurnAmount::Cw => {
                Self::write_spec(faces, specs[1], &scratch.a);
                Self::write_spec(faces, specs[2], &scratch.b);
                Self::write_spec(faces, specs[3], &scratch.c);
                Self::write_spec(faces, specs[0], &scratch.d);
            }
            TurnAmount::Half => {
                Self::write_spec(faces, specs[2], &scratch.a);
                Self::write_spec(faces, specs[3], &scratch.b);
                Self::write_spec(faces, specs[0], &scratch.c);
                Self::write_spec(faces, specs[1], &scratch.d);
            }
            TurnAmount::Ccw => {
                Self::write_spec(faces, specs[3], &scratch.a);
                Self::write_spec(faces, specs[0], &scratch.b);
                Self::write_spec(faces, specs[1], &scratch.c);
                Self::write_spec(faces, specs[2], &scratch.d);
            }
        }
    }
}

const NET_FACE_GAP: &str = "   ";

fn preview_face_width(cols: &[usize]) -> usize {
    let skipped_col_breaks = cols
        .windows(2)
        .filter(|pair| pair[0] + 1 != pair[1])
        .count();
    cols.len()
        .saturating_add(cols.len().saturating_sub(1))
        .saturating_add(skipped_col_breaks * 4)
}

fn push_face_gap_row(out: &mut String, face_width: usize, face_count: usize) {
    for face_index in 0..face_count {
        if face_index > 0 {
            out.push_str(NET_FACE_GAP);
        }

        let left_padding = face_width.saturating_sub(3) / 2;
        let right_padding = face_width.saturating_sub(3 + left_padding);
        out.push_str(&" ".repeat(left_padding));
        out.push_str("...");
        out.push_str(&" ".repeat(right_padding));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ByteArray, NibbleArray, Packed3Array};

    fn every_move_inverse_restores<S: FaceletArray>() {
        for n in 1..6 {
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                for depth in 0..n {
                    for amount in [TurnAmount::Cw, TurnAmount::Half, TurnAmount::Ccw] {
                        let mv = Move::new(axis, depth, amount);
                        let mut cube = Cube::<S>::new_solved(n);
                        cube.apply_move_untracked(mv);
                        cube.apply_move_untracked(mv.inverse());
                        assert!(cube.is_solved(), "inverse failed for n={n}, move={mv:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn inverse_restores_byte_array() {
        every_move_inverse_restores::<ByteArray>();
    }

    #[test]
    fn inverse_restores_nibble_array() {
        every_move_inverse_restores::<NibbleArray>();
    }

    #[test]
    fn inverse_restores_packed3_array() {
        every_move_inverse_restores::<Packed3Array>();
    }

    #[test]
    fn four_quarter_turns_restore() {
        for n in 1..6 {
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                for depth in 0..n {
                    let mv = Move::new(axis, depth, TurnAmount::Cw);
                    let mut cube = Cube::<ByteArray>::new_solved(n);
                    for _ in 0..4 {
                        cube.apply_move_untracked(mv);
                    }
                    assert!(cube.is_solved(), "four turns failed for n={n}, move={mv:?}");
                }
            }
        }
    }

    #[test]
    fn tracked_moves_enter_history() {
        let mut cube = Cube::<ByteArray>::new_solved(3);
        cube.apply_move(Move::new(Axis::Z, 2, TurnAmount::Cw));
        assert_eq!(cube.history().len(), 1);
    }

    #[test]
    fn preview_net_uses_traditional_geometry() {
        let cube = Cube::<ByteArray>::new_solved(2);

        assert_eq!(
            cube.preview_net_string(2),
            concat!(
                "Cube(n=2, history=0, storage~24 bytes, limit=2)\n",
                "      W W\n",
                "      W W\n",
                "\n",
                "O O   G G   R R   B B\n",
                "O O   G G   R R   B B\n",
                "\n",
                "      Y Y\n",
                "      Y Y\n",
            )
        );
    }

    #[test]
    fn preview_net_keeps_unfolded_face_orientations() {
        let mut cube = Cube::<ByteArray>::new_solved(3);

        for row in 0..3 {
            for col in 0..3 {
                cube.face_mut(FaceId::U)
                    .set(row, col, Facelet::from_u8(row as u8));
                cube.face_mut(FaceId::D)
                    .set(row, col, Facelet::from_u8((2 - row) as u8));
                cube.face_mut(FaceId::F)
                    .set(row, col, Facelet::from_u8(col as u8));
                cube.face_mut(FaceId::B)
                    .set(row, col, Facelet::from_u8((2 - col) as u8));
                cube.face_mut(FaceId::L)
                    .set(row, col, Facelet::from_u8((row + col) as u8));
                cube.face_mut(FaceId::R)
                    .set(row, col, Facelet::from_u8((row + 2 - col) as u8));
            }
        }

        assert_eq!(
            cube.preview_net_string(3),
            concat!(
                "Cube(n=3, history=0, storage~54 bytes, limit=3)\n",
                "        W W W\n",
                "        Y Y Y\n",
                "        R R R\n",
                "\n",
                "W Y R   W Y R   R Y W   R Y W\n",
                "Y R O   W Y R   O R Y   R Y W\n",
                "R O G   W Y R   G O R   R Y W\n",
                "\n",
                "        R R R\n",
                "        Y Y Y\n",
                "        W W W\n",
            )
        );
    }
}
