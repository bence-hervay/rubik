use crate::{
    algorithms::centers::{CenterCommutatorTable, FaceCommutator},
    algorithms::operation::OptimizedAlgorithm,
    conventions::face_outer_move,
    cube::{Cube, EdgeThreeCyclePlan},
    face::FaceId,
    moves::{Move, MoveAngle},
    storage::FaceletArray,
};

use super::{AlgorithmReport, ExecutionMode, MoveSequence, MoveStats, SolveOptions, SolveOutcome};

#[derive(Clone, Debug)]
pub struct SolveContext {
    options: SolveOptions,
    center_commutators: CenterCommutatorTable,
    moves: MoveSequence,
    move_stats: MoveStats,
}

impl SolveContext {
    pub fn new(options: SolveOptions) -> Self {
        Self {
            options,
            center_commutators: CenterCommutatorTable::new(),
            moves: Vec::new(),
            move_stats: MoveStats::default(),
        }
    }

    pub fn options(&self) -> SolveOptions {
        self.options
    }

    pub fn execution_mode(&self) -> ExecutionMode {
        self.options.execution_mode()
    }

    pub fn center_commutators(&self) -> &CenterCommutatorTable {
        &self.center_commutators
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    pub fn move_stats(&self) -> MoveStats {
        self.move_stats
    }

    pub fn into_outcome(self, reports: Vec<AlgorithmReport>) -> SolveOutcome {
        SolveOutcome {
            moves: self.moves,
            move_stats: self.move_stats,
            reports,
        }
    }

    pub fn apply_operation<S, O>(&mut self, cube: &mut Cube<S>, operation: &O)
    where
        S: FaceletArray,
        O: OptimizedAlgorithm,
    {
        debug_assert_eq!(
            cube.side_len(),
            operation.side_length(),
            "operation side length must match the cube",
        );
        debug_assert!(operation.is_valid(), "operation must be valid");

        match self.execution_mode() {
            ExecutionMode::Standard => {
                let moves = operation.literal_moves();
                self.apply_moves(cube, moves);
            }
            ExecutionMode::Optimized => {
                operation.for_each_literal_move(&mut |mv| {
                    self.move_stats.record(mv, cube.side_len());
                });
                operation.apply_optimized(cube);
            }
        }
    }

    pub fn apply_move<S: FaceletArray>(&mut self, cube: &mut Cube<S>, mv: Move) {
        self.move_stats.record(mv, cube.side_len());
        cube.apply_move_untracked(mv);
        if self.execution_mode().records_moves() {
            self.moves.push(mv);
        }
    }

    pub fn apply_moves<S, I>(&mut self, cube: &mut Cube<S>, moves: I)
    where
        S: FaceletArray,
        I: IntoIterator<Item = Move>,
    {
        for mv in moves {
            self.apply_move(cube, mv);
        }
    }

    pub fn apply_center_commutator<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
    ) {
        let operation = cube.face_commutator_plan(commutator, rows, columns);
        self.apply_operation(cube, &operation);
    }

    pub fn apply_normalized_center_commutator<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
    ) {
        let operation = cube.normalized_face_commutator_plan(commutator, rows, columns);
        self.apply_operation(cube, &operation);
    }

    pub fn apply_edge_three_cycle_plan<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        operation: &EdgeThreeCyclePlan,
    ) {
        self.apply_operation(cube, operation);
    }

    pub(crate) fn apply_center_face_rotation<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        face: FaceId,
        angle: MoveAngle,
    ) {
        let mv = face_outer_move(cube.side_len(), face, angle);
        self.apply_move(cube, mv);
    }
}
