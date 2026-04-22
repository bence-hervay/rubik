use crate::{
    geometry,
    line::{cycle_four_lines, StripSpec},
    moves::Axis,
    random::RandomSource,
};

use super::pieces::FacePosition;
use super::*;
impl<S: FaceletArray> Cube<S> {
    pub fn new_solved(n: usize) -> Self {
        Self::new_with_scheme(n, ColorScheme::default())
    }

    pub fn from_facelet_fn<F>(n: usize, reachability: CubeReachability, facelet_at: F) -> Self
    where
        F: FnMut(FaceId, usize, usize) -> Facelet,
    {
        assert!(n > 0, "cube side length must be > 0");

        let mut facelet_at = facelet_at;
        let faces = FaceId::ALL.map(|face_id| {
            let mut face = Face::new(face_id, n, Facelet::White);
            for row in 0..n {
                for col in 0..n {
                    face.set(row, col, facelet_at(face_id, row, col));
                }
            }
            face
        });

        Self {
            n,
            faces,
            reachability,
            history: MoveHistory::new(),
        }
    }

    pub fn new_with_scheme(n: usize, scheme: ColorScheme) -> Self {
        Self::from_facelet_fn(n, CubeReachability::Reachable, |face, _, _| {
            scheme.color_of(face)
        })
    }

    pub fn side_len(&self) -> usize {
        self.n
    }

    pub fn face(&self, id: FaceId) -> &Face<S> {
        &self.faces[id.index()]
    }

    pub fn face_mut(&mut self, id: FaceId) -> &mut Face<S> {
        self.reachability = CubeReachability::Unverified;
        &mut self.faces[id.index()]
    }

    pub fn faces(&self) -> &[Face<S>; 6] {
        &self.faces
    }

    pub fn reachability(&self) -> CubeReachability {
        self.reachability
    }

    pub fn is_reachable(&self) -> bool {
        self.reachability.is_reachable()
    }

    pub fn set_reachability(&mut self, reachability: CubeReachability) {
        self.reachability = reachability;
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
            self.faces[face.index()].rotate_meta_by(mv.angle.inverse());
        }
    }

    pub fn random_move<R: RandomSource>(&self, rng: &mut R) -> Move {
        let axis = match (rng.next_u64() % 3) as u8 {
            0 => Axis::X,
            1 => Axis::Y,
            _ => Axis::Z,
        };

        let depth = (rng.next_u64() as usize) % self.n;
        Move::new(axis, depth, random_move_angle(rng))
    }

    pub fn random_outer_face_move<R: RandomSource>(&self, face: FaceId, rng: &mut R) -> Move {
        face_layer_move(self.n, face, 0, random_move_angle(rng))
    }

    pub fn scramble<R: RandomSource>(&mut self, rng: &mut R) {
        self.scramble_with_rounds(rng, DEFAULT_SCRAMBLE_ROUNDS);
    }

    pub fn scramble_with_rounds<R: RandomSource>(&mut self, rng: &mut R, rounds: usize) {
        for _ in 0..rounds {
            self.scramble_random_moves(rng, self.n);

            for face in FaceId::ALL {
                let mv = self.random_outer_face_move(face, rng);
                self.apply_move(mv);
            }
        }
    }

    pub fn scramble_random_moves<R: RandomSource>(&mut self, rng: &mut R, count: usize) {
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

    fn validate_move(&self, mv: Move) {
        assert!(mv.depth < self.n, "move depth out of bounds");
    }

    pub(super) fn position(&self, position: FacePosition) -> Facelet {
        self.face(position.face).get(position.row, position.col)
    }

    pub(super) fn set_position(&mut self, position: FacePosition, value: Facelet) {
        self.faces[position.face.index()].set(position.row, position.col, value);
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

fn random_move_angle<R: RandomSource>(rng: &mut R) -> MoveAngle {
    match (rng.next_u64() % 3) as u8 {
        0 => MoveAngle::Positive,
        1 => MoveAngle::Double,
        _ => MoveAngle::Negative,
    }
}
