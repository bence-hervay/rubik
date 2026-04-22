use core::fmt;

mod centers;
mod corners;
pub(crate) mod edges;

use crate::{
    algorithm::OptimizedAlgorithm,
    conventions::face_outer_move,
    cube::{Cube, FaceCommutator},
    face::FaceId,
    moves::{Axis, Move, MoveAngle},
    storage::FaceletArray,
};

#[deprecated(note = "use crate::support::centers::CenterCommutatorTable")]
pub use crate::support::centers::CenterCommutatorTable;
#[deprecated(note = "use crate::support::centers::CenterCoordExpr")]
pub use crate::support::centers::CenterCoordExpr;
#[deprecated(note = "use crate::support::centers::CenterLocation")]
pub use crate::support::centers::CenterLocation;
#[deprecated(note = "use crate::support::centers::CenterLocationExpr")]
pub use crate::support::centers::CenterLocationExpr;
#[deprecated(note = "use crate::support::centers::CenterScheduleStep")]
pub use crate::support::centers::CenterScheduleStep;
#[deprecated(note = "use crate::support::centers::GENERATED_CENTER_SCHEDULE")]
pub use crate::support::centers::GENERATED_CENTER_SCHEDULE;
pub use centers::{CenterReductionStage, CenterTransferSpec};
pub use corners::{CornerReductionStage, CornerSlot};
pub use edges::{EdgePairingStage, EdgeSlot};

pub type MoveSequence = Vec<Move>;
pub type SolveResult<T> = Result<T, SolveError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SolveError {
    UnsupportedCube {
        reason: &'static str,
    },
    StageFailed {
        stage: &'static str,
        reason: &'static str,
    },
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedCube { reason } => write!(f, "unsupported cube: {reason}"),
            Self::StageFailed { stage, reason } => write!(f, "stage {stage} failed: {reason}"),
        }
    }
}

impl std::error::Error for SolveError {}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum SolvePhase {
    Centers,
    Corners,
    Edges,
    ThreeByThree,
}

impl fmt::Display for SolvePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Centers => f.write_str("centers"),
            Self::Corners => f.write_str("corners"),
            Self::Edges => f.write_str("edges"),
            Self::ThreeByThree => f.write_str("3x3"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ExecutionMode {
    Standard,
    Optimized,
}

impl ExecutionMode {
    pub const fn records_moves(self) -> bool {
        match self {
            Self::Standard => true,
            Self::Optimized => false,
        }
    }
}

impl fmt::Display for ExecutionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Standard => f.write_str("standard"),
            Self::Optimized => f.write_str("optimized"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum StageExecutionSupport {
    StandardOnly,
    StandardAndOptimized,
}

impl StageExecutionSupport {
    pub const fn supports(self, mode: ExecutionMode) -> bool {
        match (self, mode) {
            (Self::StandardOnly, ExecutionMode::Standard) => true,
            (Self::StandardOnly, ExecutionMode::Optimized) => false,
            (Self::StandardAndOptimized, _) => true,
        }
    }

    pub const fn supports_optimized(self) -> bool {
        self.supports(ExecutionMode::Optimized)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StageSideLengthSupport {
    pub minimum: usize,
    pub maximum: Option<usize>,
    pub supports_odd: bool,
    pub supports_even: bool,
}

impl StageSideLengthSupport {
    pub const fn new(
        minimum: usize,
        maximum: Option<usize>,
        supports_odd: bool,
        supports_even: bool,
    ) -> Self {
        Self {
            minimum,
            maximum,
            supports_odd,
            supports_even,
        }
    }

    pub const fn all() -> Self {
        Self::new(1, None, true, true)
    }

    pub const fn supports(self, side_length: usize) -> bool {
        if side_length < self.minimum {
            return false;
        }

        if let Some(maximum) = self.maximum {
            if side_length > maximum {
                return false;
            }
        }

        if side_length % 2 == 0 {
            self.supports_even
        } else {
            self.supports_odd
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct StageContract {
    pub side_lengths: StageSideLengthSupport,
    pub requires_previous_stages_solved: bool,
    pub standard_preconditions: &'static [&'static str],
    pub standard_postconditions: &'static [&'static str],
    pub execution_mode_support: StageExecutionSupport,
}

impl StageContract {
    pub const fn new(
        side_lengths: StageSideLengthSupport,
        requires_previous_stages_solved: bool,
        standard_preconditions: &'static [&'static str],
        standard_postconditions: &'static [&'static str],
        execution_mode_support: StageExecutionSupport,
    ) -> Self {
        Self {
            side_lengths,
            requires_previous_stages_solved,
            standard_preconditions,
            standard_postconditions,
            execution_mode_support,
        }
    }

    pub const fn supports(self, side_length: usize, mode: ExecutionMode) -> bool {
        self.side_lengths.supports(side_length) && self.execution_mode_support.supports(mode)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SolveOptions {
    pub record_moves: bool,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self::standard()
    }
}

impl SolveOptions {
    pub const fn new(execution_mode: ExecutionMode) -> Self {
        Self {
            record_moves: execution_mode.records_moves(),
        }
    }

    pub const fn standard() -> Self {
        Self::new(ExecutionMode::Standard)
    }

    pub const fn optimized() -> Self {
        Self::new(ExecutionMode::Optimized)
    }

    pub const fn execution_mode(self) -> ExecutionMode {
        if self.record_moves {
            ExecutionMode::Standard
        } else {
            ExecutionMode::Optimized
        }
    }

    pub const fn with_execution_mode(mut self, execution_mode: ExecutionMode) -> Self {
        self.record_moves = execution_mode.records_moves();
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StageReport {
    pub phase: SolvePhase,
    pub name: &'static str,
    pub sub_stage_count: usize,
    pub moves_before: usize,
    pub moves_after: usize,
}

impl StageReport {
    pub fn moves_added(&self) -> usize {
        self.moves_after - self.moves_before
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolveOutcome {
    pub moves: MoveSequence,
    pub move_stats: MoveStats,
    pub reports: Vec<StageReport>,
}

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct MoveStats {
    pub total: usize,
    pub axis_x: usize,
    pub axis_y: usize,
    pub axis_z: usize,
    pub positive: usize,
    pub double: usize,
    pub negative: usize,
    pub outer_layer: usize,
    pub inner_layer: usize,
}

impl MoveStats {
    pub fn record(&mut self, mv: Move, side_length: usize) {
        self.total += 1;

        match mv.axis {
            Axis::X => self.axis_x += 1,
            Axis::Y => self.axis_y += 1,
            Axis::Z => self.axis_z += 1,
        }

        match mv.angle {
            MoveAngle::Positive => self.positive += 1,
            MoveAngle::Double => self.double += 1,
            MoveAngle::Negative => self.negative += 1,
        }

        if mv.depth == 0 || mv.depth + 1 == side_length {
            self.outer_layer += 1;
        } else {
            self.inner_layer += 1;
        }
    }

    pub fn record_all(&mut self, moves: impl IntoIterator<Item = Move>, side_length: usize) {
        for mv in moves {
            self.record(mv, side_length);
        }
    }
}

pub use crate::algorithm::MoveSequenceAlgorithm;
pub use crate::algorithm::MoveSequenceAlgorithm as MoveSequenceOperation;

pub trait StageOperation: OptimizedAlgorithm {
    fn apply_direct<S: FaceletArray>(&self, cube: &mut Cube<S>);
}

impl<T> StageOperation for T
where
    T: OptimizedAlgorithm + ?Sized,
{
    fn apply_direct<S: FaceletArray>(&self, cube: &mut Cube<S>) {
        self.apply_optimized(cube);
    }
}

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

    pub fn into_outcome(self, reports: Vec<StageReport>) -> SolveOutcome {
        SolveOutcome {
            moves: self.moves,
            move_stats: self.move_stats,
            reports,
        }
    }

    pub fn apply_algorithm<S, A>(&mut self, cube: &mut Cube<S>, algorithm: &A)
    where
        S: FaceletArray,
        A: OptimizedAlgorithm,
    {
        debug_assert_eq!(
            cube.side_len(),
            algorithm.side_length(),
            "algorithm side length must match the cube",
        );
        debug_assert!(algorithm.is_valid(), "algorithm must be valid");

        match self.execution_mode() {
            ExecutionMode::Standard => {
                let moves = algorithm.literal_moves();
                self.apply_moves(cube, moves);
            }
            ExecutionMode::Optimized => {
                algorithm.for_each_literal_move(&mut |mv| {
                    self.move_stats.record(mv, cube.side_len());
                });
                algorithm.apply_optimized(cube);
            }
        }
    }

    pub fn apply_operation<S, O>(&mut self, cube: &mut Cube<S>, operation: &O)
    where
        S: FaceletArray,
        O: StageOperation,
    {
        self.apply_algorithm(cube, operation);
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
        let plan = cube.face_commutator_plan(commutator, rows, columns);
        self.apply_algorithm(cube, &plan);
    }

    pub fn apply_normalized_center_commutator<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
    ) {
        let plan = cube.normalized_face_commutator_plan(commutator, rows, columns);
        self.apply_algorithm(cube, &plan);
    }

    pub fn apply_edge_three_cycle_plan<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        plan: &crate::cube::EdgeThreeCyclePlan,
    ) {
        self.apply_algorithm(cube, plan);
    }

    fn apply_center_face_rotation<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        face: FaceId,
        angle: MoveAngle,
    ) {
        let mv = face_outer_move(cube.side_len(), face, angle);
        self.apply_move(cube, mv);
    }
}

pub trait Solver<S: FaceletArray> {
    fn solve(&mut self, cube: &mut Cube<S>) -> SolveResult<SolveOutcome>;
}

pub trait SolverStage<S: FaceletArray> {
    fn phase(&self) -> SolvePhase;
    fn name(&self) -> &'static str;
    fn contract(&self) -> StageContract;

    fn execution_mode_support(&self) -> StageExecutionSupport {
        self.contract().execution_mode_support
    }

    fn side_length_support(&self) -> StageSideLengthSupport {
        self.contract().side_lengths
    }

    fn requires_previous_stages_solved(&self) -> bool {
        self.contract().requires_previous_stages_solved
    }

    fn is_applicable_to_side_length(&self, side_length: usize) -> bool {
        self.side_length_support().supports(side_length)
    }

    fn sub_stages(&self) -> &[SubStageSpec];
    fn run(&mut self, cube: &mut Cube<S>, context: &mut SolveContext) -> SolveResult<()>;
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SubStageSpec {
    pub phase: SolvePhase,
    pub name: &'static str,
    pub description: &'static str,
}

impl SubStageSpec {
    pub const fn new(phase: SolvePhase, name: &'static str, description: &'static str) -> Self {
        Self {
            phase,
            name,
            description,
        }
    }
}

pub struct ReductionSolver<S: FaceletArray> {
    options: SolveOptions,
    stages: Vec<Box<dyn SolverStage<S>>>,
}

impl<S: FaceletArray + 'static> ReductionSolver<S> {
    pub fn new(options: SolveOptions) -> Self {
        Self {
            options,
            stages: Vec::new(),
        }
    }

    pub fn large_cube_default() -> Self {
        Self::new(SolveOptions::default())
            .with_stage(CenterReductionStage::western_default())
            .with_stage(CornerReductionStage::default())
            .with_stage(EdgePairingStage::default())
    }

    pub fn with_stage<T>(mut self, stage: T) -> Self
    where
        T: SolverStage<S> + 'static,
    {
        self.stages.push(Box::new(stage));
        self
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    pub fn stage_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.stages.iter().map(|stage| stage.name())
    }
}

impl<S: FaceletArray + 'static> Solver<S> for ReductionSolver<S> {
    fn solve(&mut self, cube: &mut Cube<S>) -> SolveResult<SolveOutcome> {
        let mut context = SolveContext::new(self.options);
        let mut reports = Vec::with_capacity(self.stages.len());
        let execution_mode = context.execution_mode();

        for stage in &mut self.stages {
            if !stage.execution_mode_support().supports(execution_mode) {
                return Err(SolveError::StageFailed {
                    stage: stage.name(),
                    reason: "stage does not support the requested execution mode",
                });
            }

            let moves_before = context.move_stats().total;
            stage.run(cube, &mut context)?;
            let moves_after = context.move_stats().total;

            reports.push(StageReport {
                phase: stage.phase(),
                name: stage.name(),
                sub_stage_count: stage.sub_stages().len(),
                moves_before,
                moves_after,
            });
        }

        Ok(context.into_outcome(reports))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreeByThreeStage {
    sub_stages: [SubStageSpec; 2],
}

const THREE_BY_THREE_STAGE_STANDARD_PRECONDITIONS: &[&str] =
    &["the cube should already be reduced to a 3x3-equivalent state"];
const THREE_BY_THREE_STAGE_STANDARD_POSTCONDITIONS: &[&str] =
    &["currently a placeholder adapter stage; no additional guarantees are added yet"];
const THREE_BY_THREE_STAGE_CONTRACT: StageContract = StageContract::new(
    StageSideLengthSupport::all(),
    true,
    THREE_BY_THREE_STAGE_STANDARD_PRECONDITIONS,
    THREE_BY_THREE_STAGE_STANDARD_POSTCONDITIONS,
    StageExecutionSupport::StandardAndOptimized,
);

impl Default for ThreeByThreeStage {
    fn default() -> Self {
        Self {
            sub_stages: [
                SubStageSpec::new(
                    SolvePhase::ThreeByThree,
                    "reduced-state extraction",
                    "project centers, paired edges, and corners into a 3x3 representation",
                ),
                SubStageSpec::new(
                    SolvePhase::ThreeByThree,
                    "3x3 solve adapter",
                    "delegate the reduced state to a future 3x3 solver implementation",
                ),
            ],
        }
    }
}

impl<S: FaceletArray> SolverStage<S> for ThreeByThreeStage {
    fn phase(&self) -> SolvePhase {
        SolvePhase::ThreeByThree
    }

    fn name(&self) -> &'static str {
        "3x3 finish"
    }

    fn contract(&self) -> StageContract {
        THREE_BY_THREE_STAGE_CONTRACT
    }

    fn sub_stages(&self) -> &[SubStageSpec] {
        &self.sub_stages
    }

    fn run(&mut self, _cube: &mut Cube<S>, _context: &mut SolveContext) -> SolveResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Byte;

    #[test]
    fn default_reduction_solver_has_center_corner_and_edge_stages() {
        let solver = ReductionSolver::<Byte>::large_cube_default();
        let names = solver.stage_names().collect::<Vec<_>>();

        assert_eq!(solver.stage_count(), 3);
        assert_eq!(
            names,
            ["center reduction", "corner reduction", "edge pairing"]
        );
    }

    #[test]
    fn solve_options_named_modes_round_trip_through_recording_flag() {
        let standard = SolveOptions::standard();
        assert!(standard.record_moves);
        assert_eq!(standard.execution_mode(), ExecutionMode::Standard);

        let optimized = SolveOptions::optimized();
        assert!(!optimized.record_moves);
        assert_eq!(optimized.execution_mode(), ExecutionMode::Optimized);

        let switched = SolveOptions::standard().with_execution_mode(ExecutionMode::Optimized);
        assert!(!switched.record_moves);
        assert_eq!(switched.execution_mode(), ExecutionMode::Optimized);
    }

    #[test]
    fn default_stages_explicitly_declare_execution_mode_support() {
        assert_eq!(
            <CenterReductionStage as SolverStage<Byte>>::execution_mode_support(
                &CenterReductionStage::western_default()
            ),
            StageExecutionSupport::StandardAndOptimized
        );
        assert_eq!(
            <CornerReductionStage as SolverStage<Byte>>::execution_mode_support(
                &CornerReductionStage::default()
            ),
            StageExecutionSupport::StandardAndOptimized
        );
        assert_eq!(
            <EdgePairingStage as SolverStage<Byte>>::execution_mode_support(
                &EdgePairingStage::default()
            ),
            StageExecutionSupport::StandardAndOptimized
        );
        assert_eq!(
            <ThreeByThreeStage as SolverStage<Byte>>::execution_mode_support(
                &ThreeByThreeStage::default()
            ),
            StageExecutionSupport::StandardAndOptimized
        );
    }

    #[test]
    fn stage_side_length_support_respects_range_and_parity() {
        let support = StageSideLengthSupport::new(2, Some(6), true, false);

        assert!(!support.supports(1));
        assert!(!support.supports(2));
        assert!(support.supports(3));
        assert!(!support.supports(4));
        assert!(support.supports(5));
        assert!(!support.supports(6));
        assert!(!support.supports(7));
    }

    #[test]
    fn default_stage_contracts_are_explicit_and_nonempty() {
        let center = <CenterReductionStage as SolverStage<Byte>>::contract(
            &CenterReductionStage::western_default(),
        );
        assert!(center.side_lengths.supports(1));
        assert!(!center.requires_previous_stages_solved);
        assert!(!center.standard_preconditions.is_empty());
        assert!(!center.standard_postconditions.is_empty());

        let corners =
            <CornerReductionStage as SolverStage<Byte>>::contract(&CornerReductionStage::default());
        assert!(corners.side_lengths.supports(2));
        assert!(!corners.requires_previous_stages_solved);
        assert!(!corners.standard_preconditions.is_empty());
        assert!(!corners.standard_postconditions.is_empty());

        let edges = <EdgePairingStage as SolverStage<Byte>>::contract(&EdgePairingStage::default());
        assert!(edges.side_lengths.supports(3));
        assert!(!edges.requires_previous_stages_solved);
        assert!(!edges.standard_preconditions.is_empty());
        assert!(!edges.standard_postconditions.is_empty());

        let three_by_three =
            <ThreeByThreeStage as SolverStage<Byte>>::contract(&ThreeByThreeStage::default());
        assert!(three_by_three.side_lengths.supports(3));
        assert!(three_by_three.requires_previous_stages_solved);
        assert!(!three_by_three.standard_preconditions.is_empty());
        assert!(!three_by_three.standard_postconditions.is_empty());
    }

    #[test]
    fn solver_rejects_stage_when_requested_mode_is_not_supported() {
        #[derive(Default)]
        struct StandardOnlyStage;

        impl<S: FaceletArray> SolverStage<S> for StandardOnlyStage {
            fn phase(&self) -> SolvePhase {
                SolvePhase::ThreeByThree
            }

            fn name(&self) -> &'static str {
                "standard only test stage"
            }

            fn contract(&self) -> StageContract {
                StageContract::new(
                    StageSideLengthSupport::all(),
                    false,
                    &["standard execution only"],
                    &["no-op"],
                    StageExecutionSupport::StandardOnly,
                )
            }

            fn sub_stages(&self) -> &[SubStageSpec] {
                &[]
            }

            fn run(&mut self, _cube: &mut Cube<S>, _context: &mut SolveContext) -> SolveResult<()> {
                Ok(())
            }
        }

        let mut solver =
            ReductionSolver::<Byte>::new(SolveOptions::optimized()).with_stage(StandardOnlyStage);
        let mut cube = Cube::<Byte>::new_solved(3);

        assert_eq!(
            solver.solve(&mut cube),
            Err(SolveError::StageFailed {
                stage: "standard only test stage",
                reason: "stage does not support the requested execution mode",
            })
        );
    }

    #[test]
    fn default_pipeline_runs_on_a_solved_cube_without_adding_moves() {
        let mut solver = ReductionSolver::<Byte>::large_cube_default();
        let mut cube = Cube::<Byte>::new_solved(5);

        let outcome = solver
            .solve(&mut cube)
            .expect("default stages should run on a solved cube");

        assert!(outcome.moves.is_empty());
        assert_eq!(outcome.move_stats, MoveStats::default());
        assert_eq!(outcome.reports.len(), 3);
        assert_eq!(outcome.reports[0].phase, SolvePhase::Centers);
        assert_eq!(outcome.reports[1].phase, SolvePhase::Corners);
        assert_eq!(outcome.reports[2].phase, SolvePhase::Edges);
    }
}
