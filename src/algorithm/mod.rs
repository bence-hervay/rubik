use crate::{
    cube::{Cube, EdgeThreeCyclePlan, FaceCommutatorPlan},
    moves::Move,
    storage::FaceletArray,
};

pub trait Algorithm {
    fn side_length(&self) -> usize;
    fn is_valid(&self) -> bool;
    fn for_each_literal_move(&self, f: &mut dyn FnMut(Move));

    fn literal_move_count(&self) -> usize {
        let mut count = 0;
        self.for_each_literal_move(&mut |_| count += 1);
        count
    }

    fn literal_moves(&self) -> Vec<Move> {
        let mut moves = Vec::with_capacity(self.literal_move_count());
        self.for_each_literal_move(&mut |mv| moves.push(mv));
        moves
    }
}

pub trait OptimizedAlgorithm: Algorithm {
    fn apply_optimized<S: FaceletArray>(&self, cube: &mut Cube<S>);
}

#[derive(Copy, Clone, Debug)]
pub struct MoveSequenceAlgorithm<'a> {
    side_length: usize,
    moves: &'a [Move],
}

impl<'a> MoveSequenceAlgorithm<'a> {
    pub const fn new(side_length: usize, moves: &'a [Move]) -> Self {
        Self { side_length, moves }
    }

    pub const fn moves(self) -> &'a [Move] {
        self.moves
    }
}

impl Algorithm for FaceCommutatorPlan<'_> {
    fn side_length(&self) -> usize {
        FaceCommutatorPlan::side_length(*self)
    }

    fn is_valid(&self) -> bool {
        FaceCommutatorPlan::is_valid(*self)
    }

    fn for_each_literal_move(&self, f: &mut dyn FnMut(Move)) {
        FaceCommutatorPlan::for_each_literal_move(*self, &mut |mv| f(mv));
    }

    fn literal_move_count(&self) -> usize {
        FaceCommutatorPlan::literal_move_count(*self)
    }
}

impl OptimizedAlgorithm for FaceCommutatorPlan<'_> {
    fn apply_optimized<S: FaceletArray>(&self, cube: &mut Cube<S>) {
        cube.apply_face_commutator_plan_untracked(*self);
    }
}

impl Algorithm for EdgeThreeCyclePlan {
    fn side_length(&self) -> usize {
        EdgeThreeCyclePlan::side_length(self)
    }

    fn is_valid(&self) -> bool {
        EdgeThreeCyclePlan::is_valid(self)
    }

    fn for_each_literal_move(&self, f: &mut dyn FnMut(Move)) {
        for mv in self.moves().iter().copied() {
            f(mv);
        }
    }

    fn literal_move_count(&self) -> usize {
        self.moves().len()
    }
}

impl OptimizedAlgorithm for EdgeThreeCyclePlan {
    fn apply_optimized<S: FaceletArray>(&self, cube: &mut Cube<S>) {
        cube.apply_edge_three_cycle_plan_untracked(self);
    }
}

impl Algorithm for MoveSequenceAlgorithm<'_> {
    fn side_length(&self) -> usize {
        self.side_length
    }

    fn is_valid(&self) -> bool {
        self.moves.iter().all(|mv| mv.depth < self.side_length)
    }

    fn for_each_literal_move(&self, f: &mut dyn FnMut(Move)) {
        for mv in self.moves.iter().copied() {
            f(mv);
        }
    }

    fn literal_move_count(&self) -> usize {
        self.moves.len()
    }
}

impl OptimizedAlgorithm for MoveSequenceAlgorithm<'_> {
    fn apply_optimized<S: FaceletArray>(&self, cube: &mut Cube<S>) {
        cube.apply_moves_untracked_with_threads(self.moves.iter().copied(), 1);
    }
}
