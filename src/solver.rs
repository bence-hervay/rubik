use crate::{cube::Cube, moves::Move, storage::FaceletArray};

pub type MoveSequence = Vec<Move>;

pub trait Solver<S: FaceletArray> {
    fn solve(&mut self, cube: &Cube<S>) -> MoveSequence;
}
