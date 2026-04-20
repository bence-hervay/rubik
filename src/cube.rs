use core::fmt;
use core::fmt::Write;

use crate::{
    face::{Face, FaceId},
    facelet::Facelet,
    geometry,
    history::MoveHistory,
    line::{cycle_four_lines, StripSpec},
    moves::{Axis, Move, MoveAngle},
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
        self.apply_side_cycle(specs, mv.angle);
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
            self.faces[face.index()].rotate_meta_by(mv.angle);
        }

        if mv.depth == 0 {
            let face = geometry::negative_axis_face(mv.axis);
            self.faces[face.index()].rotate_meta_by(mv.angle);
        }
    }

    pub fn random_move<R: RandomSource>(&self, rng: &mut R) -> Move {
        let axis = match (rng.next_u64() % 3) as u8 {
            0 => Axis::X,
            1 => Axis::Y,
            _ => Axis::Z,
        };

        let depth = (rng.next_u64() as usize) % self.n;

        let angle = match (rng.next_u64() % 3) as u8 {
            0 => MoveAngle::Positive,
            1 => MoveAngle::Double,
            _ => MoveAngle::Negative,
        };

        Move::new(axis, depth, angle)
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
        let cells_per_face = self
            .n
            .checked_mul(self.n)
            .expect("cube face cell count overflowed usize");
        S::storage_bytes_for_len(cells_per_face)
            .checked_mul(6)
            .expect("cube storage estimate overflowed usize")
    }

    pub fn net_string(&self) -> String {
        let rows = (0..self.n).collect::<Vec<_>>();
        let cols = (0..self.n).collect::<Vec<_>>();
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
        rows: &[usize],
        cols: &[usize],
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

    fn push_net_face_row(&self, out: &mut String, face: FaceId, row: usize, cols: &[usize]) {
        for (col_index, col) in cols.iter().copied().enumerate() {
            if col_index > 0 {
                out.push(' ');
            }
            out.push(self.face(face).get(row, col).as_char());
        }
    }

    fn validate_move(&self, mv: Move) {
        assert!(mv.depth < self.n, "move depth out of bounds");
    }

    #[inline]
    fn apply_side_cycle(&mut self, specs: [StripSpec; 4], angle: MoveAngle) {
        let traversals = [
            self.faces[specs[0].face.index()].line_traversal(
                specs[0].kind,
                specs[0].index,
                specs[0].reversed,
            ),
            self.faces[specs[1].face.index()].line_traversal(
                specs[1].kind,
                specs[1].index,
                specs[1].reversed,
            ),
            self.faces[specs[2].face.index()].line_traversal(
                specs[2].kind,
                specs[2].index,
                specs[2].reversed,
            ),
            self.faces[specs[3].face.index()].line_traversal(
                specs[3].kind,
                specs[3].index,
                specs[3].reversed,
            ),
        ];
        let face_indices = [
            specs[0].face.index(),
            specs[1].face.index(),
            specs[2].face.index(),
            specs[3].face.index(),
        ];
        let (face0, face1, face2, face3) = faces4_mut(&mut self.faces, face_indices);

        cycle_four_lines(
            face0.matrix_mut().storage_mut(),
            traversals[0],
            face1.matrix_mut().storage_mut(),
            traversals[1],
            face2.matrix_mut().storage_mut(),
            traversals[2],
            face3.matrix_mut().storage_mut(),
            traversals[3],
            self.n,
            angle,
        );
    }
}

#[inline]
fn faces4_mut<S: FaceletArray>(
    faces: &mut [Face<S>; 6],
    indices: [usize; 4],
) -> (&mut Face<S>, &mut Face<S>, &mut Face<S>, &mut Face<S>) {
    for i in 0..indices.len() {
        assert!(indices[i] < faces.len(), "face index out of bounds");
        for j in i + 1..indices.len() {
            assert_ne!(
                indices[i], indices[j],
                "move side strips must use distinct faces"
            );
        }
    }

    let ptr = faces.as_mut_ptr();
    unsafe {
        // The index checks above guarantee these mutable references do not alias.
        (
            &mut *ptr.add(indices[0]),
            &mut *ptr.add(indices[1]),
            &mut *ptr.add(indices[2]),
            &mut *ptr.add(indices[3]),
        )
    }
}

const NET_FACE_GAP: &str = "   ";

fn net_face_width(cols: &[usize]) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Byte, Byte3, FaceAngle, Nibble, RandomSource, ThreeBit, XorShift64};

    fn basic_singmaster_turn(side_length: usize, notation: &str) -> Move {
        let last = side_length - 1;

        match notation {
            "U" => Move::new(Axis::Y, last, MoveAngle::Positive),
            "U'" => Move::new(Axis::Y, last, MoveAngle::Negative),
            "U2" => Move::new(Axis::Y, last, MoveAngle::Double),
            "D" => Move::new(Axis::Y, 0, MoveAngle::Negative),
            "D'" => Move::new(Axis::Y, 0, MoveAngle::Positive),
            "D2" => Move::new(Axis::Y, 0, MoveAngle::Double),
            "R" => Move::new(Axis::X, last, MoveAngle::Positive),
            "R'" => Move::new(Axis::X, last, MoveAngle::Negative),
            "R2" => Move::new(Axis::X, last, MoveAngle::Double),
            "L" => Move::new(Axis::X, 0, MoveAngle::Negative),
            "L'" => Move::new(Axis::X, 0, MoveAngle::Positive),
            "L2" => Move::new(Axis::X, 0, MoveAngle::Double),
            "F" => Move::new(Axis::Z, last, MoveAngle::Positive),
            "F'" => Move::new(Axis::Z, last, MoveAngle::Negative),
            "F2" => Move::new(Axis::Z, last, MoveAngle::Double),
            "B" => Move::new(Axis::Z, 0, MoveAngle::Negative),
            "B'" => Move::new(Axis::Z, 0, MoveAngle::Positive),
            "B2" => Move::new(Axis::Z, 0, MoveAngle::Double),
            _ => panic!("unsupported basic Singmaster turn: {notation}"),
        }
    }

    fn random_moves(side_length: usize, count: usize, seed: u64) -> Vec<Move> {
        let mut rng = XorShift64::new(seed);
        let mut moves = Vec::with_capacity(count);

        for _ in 0..count {
            let axis = match rng.next_u64() % 3 {
                0 => Axis::X,
                1 => Axis::Y,
                _ => Axis::Z,
            };
            let depth = (rng.next_u64() as usize) % side_length;
            let angle = match rng.next_u64() % 3 {
                0 => MoveAngle::Positive,
                1 => MoveAngle::Double,
                _ => MoveAngle::Negative,
            };
            moves.push(Move::new(axis, depth, angle));
        }

        moves
    }

    fn assert_cubes_match<A: FaceletArray, B: FaceletArray>(actual: &Cube<A>, expected: &Cube<B>) {
        assert_eq!(actual.side_len(), expected.side_len());

        for face in FaceId::ALL {
            assert_eq!(
                actual.face(face).rotation(),
                expected.face(face).rotation(),
                "face rotation mismatch on {face}"
            );

            for row in 0..actual.side_len() {
                for col in 0..actual.side_len() {
                    assert_eq!(
                        actual.face(face).get(row, col),
                        expected.face(face).get(row, col),
                        "facelet mismatch on {face} at ({row}, {col})"
                    );
                }
            }
        }
    }

    fn every_move_inverse_restores<S: FaceletArray>() {
        for n in 1..6 {
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                for depth in 0..n {
                    for angle in MoveAngle::ALL {
                        let mv = Move::new(axis, depth, angle);
                        let mut cube = Cube::<S>::new_solved(n);
                        cube.apply_move_untracked(mv);
                        cube.apply_move_untracked(mv.inverse());
                        assert!(cube.is_solved(), "inverse failed for n={n}, move={mv:?}");
                    }
                }
            }
        }
    }

    fn exact_cube_storage_bytes<S: FaceletArray>(side_length: usize) -> usize {
        side_length
            .checked_mul(side_length)
            .map(S::storage_bytes_for_len)
            .and_then(|bytes_per_face| bytes_per_face.checked_mul(6))
            .expect("test cube storage estimate overflowed usize")
    }

    #[test]
    fn inverse_restores_byte() {
        every_move_inverse_restores::<Byte>();
    }

    #[test]
    fn inverse_restores_byte3() {
        every_move_inverse_restores::<Byte3>();
    }

    #[test]
    fn inverse_restores_nibble() {
        every_move_inverse_restores::<Nibble>();
    }

    #[test]
    fn inverse_restores_three_bit() {
        every_move_inverse_restores::<ThreeBit>();
    }

    #[test]
    fn cube_backends_agree_after_random_moves() {
        let side_length = 6;
        let moves = random_moves(side_length, 1_000, 0xC0DE_CAFE);

        let mut byte = Cube::<Byte>::new_solved(side_length);
        let mut byte3 = Cube::<Byte3>::new_solved(side_length);
        let mut nibble = Cube::<Nibble>::new_solved(side_length);
        let mut three_bit = Cube::<ThreeBit>::new_solved(side_length);

        byte.apply_moves_untracked(moves.iter().copied());
        byte3.apply_moves_untracked(moves.iter().copied());
        nibble.apply_moves_untracked(moves.iter().copied());
        three_bit.apply_moves_untracked(moves.iter().copied());

        assert_cubes_match(&byte3, &byte);
        assert_cubes_match(&nibble, &byte);
        assert_cubes_match(&three_bit, &byte);
    }

    #[test]
    fn cube_storage_estimates_are_exact() {
        for side_length in [1usize, 2, 3, 4, 5, 8, 9, 10, 17] {
            assert_eq!(
                Cube::<Byte>::new_solved(side_length).estimated_storage_bytes(),
                exact_cube_storage_bytes::<Byte>(side_length)
            );
            assert_eq!(
                Cube::<Byte3>::new_solved(side_length).estimated_storage_bytes(),
                exact_cube_storage_bytes::<Byte3>(side_length)
            );
            assert_eq!(
                Cube::<Nibble>::new_solved(side_length).estimated_storage_bytes(),
                exact_cube_storage_bytes::<Nibble>(side_length)
            );
            assert_eq!(
                Cube::<ThreeBit>::new_solved(side_length).estimated_storage_bytes(),
                exact_cube_storage_bytes::<ThreeBit>(side_length)
            );
        }
    }

    #[test]
    fn four_quarter_turns_restore() {
        for n in 1..6 {
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                for depth in 0..n {
                    let mv = Move::new(axis, depth, MoveAngle::Positive);
                    let mut cube = Cube::<Byte>::new_solved(n);
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
        let mut cube = Cube::<Byte>::new_solved(3);
        cube.apply_move(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.history().len(), 1);
    }

    #[test]
    fn basic_singmaster_turns_match_our_move_notation() {
        let side_length = 5;
        let last = side_length - 1;

        let cases = [
            ("U", Axis::Y, last, MoveAngle::Positive),
            ("U'", Axis::Y, last, MoveAngle::Negative),
            ("U2", Axis::Y, last, MoveAngle::Double),
            ("D", Axis::Y, 0, MoveAngle::Negative),
            ("D'", Axis::Y, 0, MoveAngle::Positive),
            ("D2", Axis::Y, 0, MoveAngle::Double),
            ("R", Axis::X, last, MoveAngle::Positive),
            ("R'", Axis::X, last, MoveAngle::Negative),
            ("R2", Axis::X, last, MoveAngle::Double),
            ("L", Axis::X, 0, MoveAngle::Negative),
            ("L'", Axis::X, 0, MoveAngle::Positive),
            ("L2", Axis::X, 0, MoveAngle::Double),
            ("F", Axis::Z, last, MoveAngle::Positive),
            ("F'", Axis::Z, last, MoveAngle::Negative),
            ("F2", Axis::Z, last, MoveAngle::Double),
            ("B", Axis::Z, 0, MoveAngle::Negative),
            ("B'", Axis::Z, 0, MoveAngle::Positive),
            ("B2", Axis::Z, 0, MoveAngle::Double),
        ];

        for (notation, axis, depth, angle) in cases {
            assert_eq!(
                basic_singmaster_turn(side_length, notation),
                Move::new(axis, depth, angle),
                "unexpected move notation for {notation}"
            );
        }
    }

    #[test]
    fn basic_singmaster_prime_and_double_turns_match_inverse_rules() {
        let side_length = 5;
        let cases = [
            ("U", "U'", "U2"),
            ("D", "D'", "D2"),
            ("R", "R'", "R2"),
            ("L", "L'", "L2"),
            ("F", "F'", "F2"),
            ("B", "B'", "B2"),
        ];

        for (turn, prime, double) in cases {
            let turn_move = basic_singmaster_turn(side_length, turn);
            let prime_move = basic_singmaster_turn(side_length, prime);
            let double_move = basic_singmaster_turn(side_length, double);

            assert_eq!(
                prime_move,
                turn_move.inverse(),
                "{prime} should invert {turn}"
            );
            assert_eq!(
                double_move,
                double_move.inverse(),
                "{double} should be self-inverse"
            );

            let mut cube = Cube::<Byte>::new_solved(side_length);
            cube.apply_move_untracked(turn_move);
            cube.apply_move_untracked(prime_move);
            assert!(
                cube.is_solved(),
                "{turn} followed by {prime} should restore"
            );

            let mut cube = Cube::<Byte>::new_solved(side_length);
            cube.apply_move_untracked(double_move);
            cube.apply_move_untracked(double_move);
            assert!(cube.is_solved(), "{double} twice should restore");
        }
    }

    #[test]
    fn outer_face_rotation_tracks_move_angle_directly() {
        let mut cube = Cube::<Byte>::new_solved(3);

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(1));

        cube.apply_move_untracked(Move::new(Axis::Z, 0, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::B).rotation(), FaceAngle::new(1));

        cube.apply_move_untracked(Move::new(Axis::X, 2, MoveAngle::Negative));
        assert_eq!(cube.face(FaceId::R).rotation(), FaceAngle::new(3));

        cube.apply_move_untracked(Move::new(Axis::X, 0, MoveAngle::Double));
        assert_eq!(cube.face(FaceId::L).rotation(), FaceAngle::new(2));
    }

    #[test]
    fn face_rotation_accumulates_angles_modulo_four() {
        let mut cube = Cube::<Byte>::new_solved(3);

        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(0));

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(1));

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(2));

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(3));

        cube.apply_move_untracked(Move::new(Axis::Z, 2, MoveAngle::Positive));
        assert_eq!(cube.face(FaceId::F).rotation(), FaceAngle::new(0));
    }

    #[test]
    fn net_uses_traditional_geometry() {
        let cube = Cube::<Byte>::new_solved(2);

        assert_eq!(
            cube.net_string(),
            concat!(
                "Cube(n=2, history=0, storage~24 bytes)\n",
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
    fn net_keeps_unfolded_face_orientations() {
        let mut cube = Cube::<Byte>::new_solved(3);

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
            cube.net_string(),
            concat!(
                "Cube(n=3, history=0, storage~54 bytes)\n",
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

    #[test]
    fn net_prints_full_small_faces() {
        let cube = Cube::<Byte>::new_solved(4);
        let net = cube.net_string();

        assert!(!net.contains("..."));
        assert!(net.contains("W W W W"));
        assert!(net.contains("O O O O   G G G G   R R R R   B B B B"));
    }

    #[test]
    fn net_prints_full_large_faces_without_ellipsis_markers() {
        let cube = Cube::<Byte>::new_solved(5);
        let net = cube.net_string();

        assert!(!net.contains("..."));
        assert!(net.contains("            W W W W W\n"));
        assert!(net.contains("O O O O O   G G G G G   R R R R R   B B B B B\n"));
        assert!(net.contains("            Y Y Y Y Y\n"));
    }
}
