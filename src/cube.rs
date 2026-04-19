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
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Cube(n={}, history={}, storage~{} bytes, limit={})",
            self.n,
            self.history.len(),
            self.estimated_storage_bytes(),
            limit
        );

        for id in FaceId::ALL {
            let _ = writeln!(out, "{id}:");
            out.push_str(&self.face(id).preview_string(limit));
        }

        out
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
                        assert!(
                            cube.is_solved(),
                            "inverse failed for n={n}, move={mv:?}"
                        );
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
}
