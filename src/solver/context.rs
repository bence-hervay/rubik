use crate::{
    algorithms::centers::{CenterCommutatorTable, FaceCommutator},
    algorithms::operation::{record_face_commutator_move_stats, OptimizedAlgorithm},
    conventions::face_outer_move,
    cube::{Cube, EdgeThreeCyclePlan, FaceCommutatorMode},
    face::FaceId,
    moves::{Move, MoveAngle},
    storage::FaceletArray,
};

use super::{
    progress::SolveProgress, AlgorithmReport, ExecutionMode, MoveSequence, MoveStats, SolveOptions,
    SolveOutcome, StageProgressSpec,
};

#[derive(Clone, Debug)]
pub struct SolveContext {
    options: SolveOptions,
    center_commutators: CenterCommutatorTable,
    moves: MoveSequence,
    move_stats: MoveStats,
    progress: SolveProgress,
}

impl SolveContext {
    pub fn new(options: SolveOptions) -> Self {
        Self {
            options,
            center_commutators: CenterCommutatorTable::new(),
            moves: Vec::new(),
            move_stats: MoveStats::default(),
            progress: SolveProgress::disabled(),
        }
    }

    pub fn enable_progress_bars(&mut self) {
        self.progress.enable();
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

    pub(crate) fn progress_enabled(&self) -> bool {
        self.progress.is_enabled()
    }

    pub(crate) fn with_stage_progress<T, F>(&mut self, spec: StageProgressSpec, work: F) -> T
    where
        F: FnOnce(&mut Self) -> T,
    {
        self.progress.start_stage(spec);
        let result = work(self);
        self.progress.finish_stage();
        result
    }

    pub(crate) fn advance_stage_progress(&mut self, delta: usize) {
        self.progress.advance(delta);
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
                operation.record_move_stats(&mut self.move_stats, cube.side_len());
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

    pub fn apply_normalized_center_commutator_row<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        commutator: FaceCommutator,
        row: usize,
        columns: &[usize],
    ) {
        let rows = [row];

        match self.execution_mode() {
            ExecutionMode::Standard => {
                self.apply_normalized_center_commutator(cube, commutator, &rows, columns);
            }
            ExecutionMode::Optimized => {
                record_face_commutator_move_stats(
                    &mut self.move_stats,
                    cube.side_len(),
                    commutator,
                    FaceCommutatorMode::Normalized,
                    &rows,
                    columns,
                );
                cube.apply_normalized_face_commutator_prevalidated_untracked(
                    commutator, &rows, columns,
                );
            }
        }
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
