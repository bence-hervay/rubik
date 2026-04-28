use crate::{
    conventions::face_layer_move,
    cube::{Cube, EdgeThreeCyclePlan, FaceCommutator, FaceCommutatorMode, FaceCommutatorPlan},
    geometry,
    line::StripSpec,
    moves::{Move, MoveAngle},
    solver::MoveStats,
    storage::FaceletArray,
};

pub trait Operation {
    fn side_length(&self) -> usize;
    fn is_valid(&self) -> bool;
    fn for_each_literal_move(&self, f: &mut dyn FnMut(Move));

    fn record_move_stats(&self, stats: &mut MoveStats, side_length: usize) {
        self.for_each_literal_move(&mut |mv| stats.record(mv, side_length));
    }

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

pub trait OptimizedOperation: Operation {
    fn apply_optimized<S: FaceletArray>(&self, cube: &mut Cube<S>);
}

#[derive(Copy, Clone, Debug)]
struct PlannedMove {
    mv: Move,
    specs: [StripSpec; 4],
}

#[derive(Clone, Debug)]
pub struct MoveSequenceOperation<'a> {
    side_length: usize,
    moves: &'a [Move],
    planned_moves: Vec<PlannedMove>,
}

impl<'a> MoveSequenceOperation<'a> {
    pub fn new(side_length: usize, moves: &'a [Move]) -> Self {
        let planned_moves = if moves.iter().all(|mv| mv.depth < side_length) {
            moves
                .iter()
                .copied()
                .map(|mv| PlannedMove {
                    mv,
                    specs: geometry::plan_positive_quarter_turn(mv.axis, mv.depth, side_length),
                })
                .collect()
        } else {
            Vec::new()
        };

        Self {
            side_length,
            moves,
            planned_moves,
        }
    }

    pub const fn moves(&self) -> &'a [Move] {
        self.moves
    }
}

pub(crate) fn record_face_commutator_move_stats(
    stats: &mut MoveStats,
    side_length: usize,
    commutator: FaceCommutator,
    mode: FaceCommutatorMode,
    rows: &[usize],
    columns: &[usize],
) {
    let inner_count = rows.len() + columns.len();
    let sample_depth = rows
        .first()
        .copied()
        .or_else(|| columns.first().copied())
        .expect("validated face commutator plans must include at least one row or column");
    let helper = commutator.helper();
    let reverse = commutator.slice_angle().inverse();
    let forward = commutator.slice_angle();

    stats.record_repeated(
        face_layer_move(side_length, helper, sample_depth, reverse),
        side_length,
        inner_count,
    );
    stats.record_repeated(
        face_layer_move(
            side_length,
            commutator.destination(),
            0,
            MoveAngle::Positive,
        ),
        side_length,
        2,
    );
    stats.record_repeated(
        face_layer_move(
            side_length,
            commutator.destination(),
            0,
            MoveAngle::Negative,
        ),
        side_length,
        1 + usize::from(mode == FaceCommutatorMode::Normalized),
    );
    stats.record_repeated(
        face_layer_move(side_length, helper, sample_depth, forward),
        side_length,
        inner_count,
    );
}

impl Operation for FaceCommutatorPlan<'_> {
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

    fn record_move_stats(&self, stats: &mut MoveStats, side_length: usize) {
        let layers = self.layers();
        record_face_commutator_move_stats(
            stats,
            side_length,
            self.commutator(),
            self.mode(),
            layers.rows(),
            layers.columns(),
        );
    }
}

impl OptimizedOperation for FaceCommutatorPlan<'_> {
    fn apply_optimized<S: FaceletArray>(&self, cube: &mut Cube<S>) {
        cube.apply_face_commutator_plan_untracked(*self);
    }
}

impl Operation for EdgeThreeCyclePlan {
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

impl OptimizedOperation for EdgeThreeCyclePlan {
    fn apply_optimized<S: FaceletArray>(&self, cube: &mut Cube<S>) {
        cube.apply_edge_three_cycle_plan_untracked(self);
    }
}

impl Operation for MoveSequenceOperation<'_> {
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

impl OptimizedOperation for MoveSequenceOperation<'_> {
    fn apply_optimized<S: FaceletArray>(&self, cube: &mut Cube<S>) {
        if self.planned_moves.len() != self.moves.len() {
            cube.apply_moves_untracked(self.moves.iter().copied());
            return;
        }

        for planned in &self.planned_moves {
            cube.apply_move_with_plan_untracked(planned.mv, planned.specs);
        }
    }
}

pub use MoveSequenceOperation as MoveSequenceAlgorithm;
pub use Operation as Algorithm;
pub use OptimizedOperation as OptimizedAlgorithm;
