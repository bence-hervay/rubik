use rayon::prelude::*;

use crate::{
    conventions::face_layer_move,
    face::{Face, FaceId},
    facelet::Facelet,
    geometry,
    line::{cycle_four_lines, LineTraversal, StripSpec},
    moves::{Axis, Move, MoveAngle, MoveHistory},
    random::RandomSource,
    simulation::derived::FacePosition,
    storage::FaceletArray,
    util::threading::optimized_thread_pool,
};

use super::{ColorScheme, Cube, CubeReachability, DEFAULT_SCRAMBLE_ROUNDS};

pub fn balanced_outer_layer_probability(side_length: usize) -> f64 {
    assert!(side_length > 0, "cube side length must be > 0");
    (2.0 / side_length as f64).min(1.0)
}

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
        self.rotate_outer_face_meta_unchecked(mv);
        self.apply_side_cycle(specs, mv.angle);
    }

    pub(crate) fn apply_move_with_plan_untracked(&mut self, mv: Move, specs: [StripSpec; 4]) {
        self.validate_move(mv);
        self.rotate_outer_face_meta_unchecked(mv);
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
        self.rotate_outer_face_meta_unchecked(mv);
    }

    fn rotate_outer_face_meta_unchecked(&mut self, mv: Move) {
        debug_assert!(mv.depth < self.n, "move depth out of bounds");

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
        self.scramble_uniform_random_layers(rng, rounds);
    }

    pub fn scramble_uniform_random_layers<R: RandomSource>(&mut self, rng: &mut R, k: usize) {
        let moves_per_round = self
            .n
            .checked_mul(3)
            .expect("scramble move count overflowed usize");
        for _ in 0..k {
            self.scramble_random_moves(rng, moves_per_round);
        }
    }

    pub fn scramble_biased_random_layers<R: RandomSource>(&mut self, rng: &mut R, k: usize) {
        self.scramble_uniform_random_layers(rng, k);
    }

    pub fn scramble_biased_random_layers_with_outer_probability<R: RandomSource>(
        &mut self,
        rng: &mut R,
        k: usize,
        outer_layer_probability: f64,
    ) {
        assert!(
            outer_layer_probability.is_finite() && (0.0..=1.0).contains(&outer_layer_probability),
            "outer layer probability must be in 0.0..=1.0"
        );

        for _ in 0..k {
            for _ in 0..self.n {
                for _ in 0..3 {
                    let mv = Move::new(
                        random_axis(rng),
                        random_biased_layer(self.n, rng, outer_layer_probability),
                        random_move_angle(rng),
                    );
                    self.apply_move(mv);
                }
            }
        }
    }

    pub fn scramble_layer_sweeps<R: RandomSource>(&mut self, rng: &mut R, k: usize) {
        for _ in 0..k {
            for axis in [Axis::X, Axis::Y, Axis::Z] {
                for depth in 0..self.n {
                    self.apply_move(Move::new(axis, depth, random_move_angle(rng)));
                }
            }
        }
    }

    pub fn scramble_random_moves<R: RandomSource>(&mut self, rng: &mut R, count: usize) {
        for _ in 0..count {
            let mv = self.random_move(rng);
            self.apply_move(mv);
        }
    }

    pub fn scramble_parallel_random_layer_batches_untracked<R: RandomSource>(
        &mut self,
        rng: &mut R,
        rounds: usize,
        thread_count: usize,
    ) -> usize {
        let total_moves = scramble_move_count(self.n, rounds);
        let batch_size = parallel_scramble_batch_size(self.n);
        let mut remaining = total_moves;
        let mut depths = Vec::with_capacity(batch_size);
        let mut moves = Vec::with_capacity(batch_size);

        while remaining > 0 {
            let axis = random_axis(rng);
            let batch_len = batch_size.min(remaining).min(self.n);
            depths.clear();
            moves.clear();

            while moves.len() < batch_len {
                let depth = (rng.next_u64() as usize) % self.n;
                if depths.contains(&depth) {
                    continue;
                }

                depths.push(depth);
                moves.push(Move::new(axis, depth, random_move_angle(rng)));
            }

            self.apply_independent_same_axis_moves_untracked_parallel(&moves, thread_count);
            remaining -= moves.len();
        }

        total_moves
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

    pub(crate) fn position(&self, position: FacePosition) -> Facelet {
        self.face(position.face).get(position.row, position.col)
    }

    pub(crate) fn set_position(&mut self, position: FacePosition, value: Facelet) {
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

    pub(crate) fn apply_independent_same_axis_moves_untracked_parallel(
        &mut self,
        moves: &[Move],
        thread_count: usize,
    ) {
        if moves.is_empty() {
            return;
        }

        if moves.len() == 1
            || thread_count <= 1
            || !S::SUPPORTS_DISJOINT_PARALLEL_WRITES
            || !same_axis_unique_depths(moves, self.n)
        {
            self.apply_moves_untracked(moves.iter().copied());
            return;
        }

        let planned = moves
            .iter()
            .copied()
            .map(|mv| {
                self.validate_move(mv);
                let specs = self.plan_move(mv);
                PlannedIndependentMove {
                    angle: mv.angle,
                    face_indices: [
                        specs[0].face.index(),
                        specs[1].face.index(),
                        specs[2].face.index(),
                        specs[3].face.index(),
                    ],
                    traversals: [
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
                    ],
                }
            })
            .collect::<Vec<_>>();

        for mv in moves.iter().copied() {
            self.rotate_outer_face_meta_unchecked(mv);
        }

        let storages = ParallelFaceStorages::new(&mut self.faces);
        let thread_count = thread_count.min(planned.len());
        let chunk_size = planned.len().div_ceil(thread_count);
        let side_length = self.n;

        optimized_thread_pool().install(|| {
            planned.par_chunks(chunk_size).for_each(|chunk| {
                for planned in chunk {
                    unsafe {
                        cycle_four_lines_parallel(
                            storages,
                            planned.face_indices,
                            planned.traversals,
                            side_length,
                            planned.angle,
                        );
                    }
                }
            });
        });
    }
}

#[derive(Copy, Clone, Debug)]
struct PlannedIndependentMove {
    angle: MoveAngle,
    face_indices: [usize; 4],
    traversals: [LineTraversal; 4],
}

struct ParallelFaceStorages<S: FaceletArray> {
    ptrs: [*mut S; 6],
}

impl<S: FaceletArray> Copy for ParallelFaceStorages<S> {}

impl<S: FaceletArray> Clone for ParallelFaceStorages<S> {
    fn clone(&self) -> Self {
        *self
    }
}

unsafe impl<S: FaceletArray> Send for ParallelFaceStorages<S> {}
unsafe impl<S: FaceletArray> Sync for ParallelFaceStorages<S> {}

impl<S: FaceletArray> ParallelFaceStorages<S> {
    fn new(faces: &mut [Face<S>; 6]) -> Self {
        Self {
            ptrs: [
                faces[0].matrix_mut().storage_mut() as *mut S,
                faces[1].matrix_mut().storage_mut() as *mut S,
                faces[2].matrix_mut().storage_mut() as *mut S,
                faces[3].matrix_mut().storage_mut() as *mut S,
                faces[4].matrix_mut().storage_mut() as *mut S,
                faces[5].matrix_mut().storage_mut() as *mut S,
            ],
        }
    }

    unsafe fn get_raw(self, face_index: usize, index: usize) -> u8 {
        S::get_unchecked_raw_parallel(self.ptrs[face_index] as *const S, index)
    }

    unsafe fn set_raw(self, face_index: usize, index: usize, value: u8) {
        S::set_unchecked_raw_parallel(self.ptrs[face_index], index, value);
    }
}

unsafe fn cycle_four_lines_parallel<S: FaceletArray>(
    storages: ParallelFaceStorages<S>,
    face_indices: [usize; 4],
    traversals: [LineTraversal; 4],
    len: usize,
    angle: MoveAngle,
) {
    match angle {
        MoveAngle::Positive => cycle_four_lines_parallel_mapped(
            storages,
            face_indices,
            traversals,
            len,
            |v0, v1, v2, v3| (v3, v0, v1, v2),
        ),
        MoveAngle::Double => cycle_four_lines_parallel_mapped(
            storages,
            face_indices,
            traversals,
            len,
            |v0, v1, v2, v3| (v2, v3, v0, v1),
        ),
        MoveAngle::Negative => cycle_four_lines_parallel_mapped(
            storages,
            face_indices,
            traversals,
            len,
            |v0, v1, v2, v3| (v1, v2, v3, v0),
        ),
    }
}

unsafe fn cycle_four_lines_parallel_mapped<S, F>(
    storages: ParallelFaceStorages<S>,
    face_indices: [usize; 4],
    traversals: [LineTraversal; 4],
    len: usize,
    mut rotate: F,
) where
    S: FaceletArray,
    F: FnMut(u8, u8, u8, u8) -> (u8, u8, u8, u8),
{
    let mut p0 = traversals[0].start;
    let mut p1 = traversals[1].start;
    let mut p2 = traversals[2].start;
    let mut p3 = traversals[3].start;

    for _ in 0..len {
        let i0 = p0 as usize;
        let i1 = p1 as usize;
        let i2 = p2 as usize;
        let i3 = p3 as usize;

        let v0 = storages.get_raw(face_indices[0], i0);
        let v1 = storages.get_raw(face_indices[1], i1);
        let v2 = storages.get_raw(face_indices[2], i2);
        let v3 = storages.get_raw(face_indices[3], i3);
        let (n0, n1, n2, n3) = rotate(v0, v1, v2, v3);

        storages.set_raw(face_indices[0], i0, n0);
        storages.set_raw(face_indices[1], i1, n1);
        storages.set_raw(face_indices[2], i2, n2);
        storages.set_raw(face_indices[3], i3, n3);

        p0 += traversals[0].step;
        p1 += traversals[1].step;
        p2 += traversals[2].step;
        p3 += traversals[3].step;
    }
}

fn same_axis_unique_depths(moves: &[Move], side_length: usize) -> bool {
    let Some(first) = moves.first() else {
        return true;
    };
    let mut depths = Vec::with_capacity(moves.len());

    for mv in moves.iter().copied() {
        if mv.axis != first.axis || mv.depth >= side_length {
            return false;
        }
        depths.push(mv.depth);
    }

    depths.sort_unstable();
    depths.windows(2).all(|pair| pair[0] != pair[1])
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

fn scramble_move_count(side_length: usize, rounds: usize) -> usize {
    side_length
        .checked_mul(3)
        .and_then(|moves_per_round| moves_per_round.checked_mul(rounds))
        .expect("scramble move count overflowed usize")
}

fn parallel_scramble_batch_size(side_length: usize) -> usize {
    (side_length / 64).max(1)
}

fn random_axis<R: RandomSource>(rng: &mut R) -> Axis {
    match (rng.next_u64() % 3) as u8 {
        0 => Axis::X,
        1 => Axis::Y,
        _ => Axis::Z,
    }
}

fn random_biased_layer<R: RandomSource>(
    side_length: usize,
    rng: &mut R,
    outer_layer_probability: f64,
) -> usize {
    if side_length <= 2 || random_unit_interval(rng) < outer_layer_probability {
        if rng.next_u64() & 1 == 0 {
            0
        } else {
            side_length - 1
        }
    } else {
        1 + (rng.next_u64() as usize % (side_length - 2))
    }
}

fn random_unit_interval<R: RandomSource>(rng: &mut R) -> f64 {
    const DENOMINATOR: f64 = u64::MAX as f64 + 1.0;
    rng.next_u64() as f64 / DENOMINATOR
}
