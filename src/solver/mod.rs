use core::fmt;
use std::collections::{HashSet, VecDeque};

mod corners;
pub(crate) mod edges;

use crate::{
    algorithm::OptimizedAlgorithm,
    conventions::{face_outer_move, home_facelet_for_face},
    cube::{Cube, FaceCommutator},
    face::FaceId,
    facelet::Facelet,
    moves::{Axis, Move, MoveAngle},
    storage::FaceletArray,
    threading::default_thread_count,
};

pub use crate::support::centers::{
    CenterCommutatorTable, CenterCoordExpr, CenterLocation, CenterLocationExpr, CenterScheduleStep,
    GENERATED_CENTER_SCHEDULE,
};
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
    pub thread_count: usize,
    pub record_moves: bool,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self::standard(default_thread_count())
    }
}

impl SolveOptions {
    pub const fn new(thread_count: usize, execution_mode: ExecutionMode) -> Self {
        Self {
            thread_count,
            record_moves: execution_mode.records_moves(),
        }
    }

    pub const fn standard(thread_count: usize) -> Self {
        Self::new(thread_count, ExecutionMode::Standard)
    }

    pub const fn optimized(thread_count: usize) -> Self {
        Self::new(thread_count, ExecutionMode::Optimized)
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
        assert!(
            options.thread_count > 0,
            "thread count must be greater than zero"
        );

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
        cube.apply_move_untracked_with_threads(mv, self.options.thread_count);
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CenterTransferSpec {
    pub source: FaceId,
    pub destination: FaceId,
    pub color: Facelet,
}

impl CenterTransferSpec {
    pub const fn new(source: FaceId, destination: FaceId, color: Facelet) -> Self {
        Self {
            source,
            destination,
            color,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CenterReductionStage {
    transfers: Vec<CenterTransferSpec>,
    sub_stages: Vec<SubStageSpec>,
    schedule: &'static [CenterScheduleStep],
}

const CENTER_STAGE_STANDARD_PRECONDITIONS: &[&str] =
    &["none; the center stage may start from any cube state"];
const CENTER_STAGE_STANDARD_POSTCONDITIONS: &[&str] =
    &["all center facelets are solved when the stage returns success"];
const CENTER_STAGE_CONTRACT: StageContract = StageContract::new(
    StageSideLengthSupport::all(),
    false,
    CENTER_STAGE_STANDARD_PRECONDITIONS,
    CENTER_STAGE_STANDARD_POSTCONDITIONS,
    StageExecutionSupport::StandardAndOptimized,
);

impl CenterReductionStage {
    pub fn new(transfers: Vec<CenterTransferSpec>) -> Self {
        let sub_stages = vec![
            SubStageSpec::new(
                SolvePhase::Centers,
                "center scan tables",
                "scan source and destination centers into reusable row and column tables",
            ),
            SubStageSpec::new(
                SolvePhase::Centers,
                "center batch selection",
                "select disjoint row and column batches for each planned transfer",
            ),
            SubStageSpec::new(
                SolvePhase::Centers,
                "center commutator updates",
                "apply precomputed face commutator plans to selected batches",
            ),
        ];

        Self {
            transfers,
            sub_stages,
            schedule: GENERATED_CENTER_SCHEDULE,
        }
    }

    pub fn western_default() -> Self {
        Self::new(vec![
            CenterTransferSpec::new(FaceId::F, FaceId::R, Facelet::Red),
            CenterTransferSpec::new(FaceId::U, FaceId::R, Facelet::Red),
            CenterTransferSpec::new(FaceId::B, FaceId::R, Facelet::Red),
            CenterTransferSpec::new(FaceId::L, FaceId::R, Facelet::Red),
            CenterTransferSpec::new(FaceId::D, FaceId::R, Facelet::Red),
            CenterTransferSpec::new(FaceId::U, FaceId::L, Facelet::Orange),
            CenterTransferSpec::new(FaceId::D, FaceId::L, Facelet::Orange),
            CenterTransferSpec::new(FaceId::B, FaceId::L, Facelet::Orange),
            CenterTransferSpec::new(FaceId::F, FaceId::L, Facelet::Orange),
            CenterTransferSpec::new(FaceId::B, FaceId::F, Facelet::Green),
            CenterTransferSpec::new(FaceId::U, FaceId::F, Facelet::Green),
            CenterTransferSpec::new(FaceId::D, FaceId::F, Facelet::Green),
            CenterTransferSpec::new(FaceId::U, FaceId::D, Facelet::Yellow),
            CenterTransferSpec::new(FaceId::B, FaceId::D, Facelet::Yellow),
            CenterTransferSpec::new(FaceId::U, FaceId::B, Facelet::Blue),
        ])
    }

    pub fn transfers(&self) -> &[CenterTransferSpec] {
        &self.transfers
    }

    pub fn schedule(&self) -> &'static [CenterScheduleStep] {
        self.schedule
    }

    pub fn with_schedule(mut self, schedule: &'static [CenterScheduleStep]) -> Self {
        self.schedule = schedule;
        self
    }
}

impl<S: FaceletArray> SolverStage<S> for CenterReductionStage {
    fn phase(&self) -> SolvePhase {
        SolvePhase::Centers
    }

    fn name(&self) -> &'static str {
        "center reduction"
    }

    fn contract(&self) -> StageContract {
        CENTER_STAGE_CONTRACT
    }

    fn sub_stages(&self) -> &[SubStageSpec] {
        &self.sub_stages
    }

    fn run(&mut self, cube: &mut Cube<S>, context: &mut SolveContext) -> SolveResult<()> {
        solve_centers_with_transfers(
            cube,
            context,
            &self.transfers,
            self.schedule,
            "center reduction",
        )
    }
}

fn solve_centers_with_transfers<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    transfers: &[CenterTransferSpec],
    schedule: &[CenterScheduleStep],
    stage_name: &'static str,
) -> SolveResult<()> {
    if centers_are_solved(cube) {
        return Ok(());
    }

    align_true_centers(cube, context, stage_name)?;

    if cube.side_len() < 4 || centers_are_solved(cube) {
        return Ok(());
    }

    let mut column_buffer = Vec::with_capacity(cube.side_len().saturating_sub(2));

    for transfer in transfers.iter().copied() {
        if centers_are_solved(cube) {
            return Ok(());
        }
        push_center_transfer(
            cube,
            context,
            transfer,
            schedule,
            &mut column_buffer,
            stage_name,
        )?;
    }

    if centers_are_solved(cube) {
        Ok(())
    } else {
        Err(SolveError::StageFailed {
            stage: stage_name,
            reason: "generated center schedule made no further progress",
        })
    }
}

fn align_true_centers<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    stage_name: &'static str,
) -> SolveResult<()> {
    let side_length = cube.side_len();
    if side_length < 3 || side_length % 2 == 0 {
        return Ok(());
    }

    let mid = side_length / 2;
    let start = center_orientation(cube, mid);
    let target = solved_center_orientation();
    if start == target {
        return Ok(());
    }

    let Some(moves) = center_alignment_moves(start, target, mid) else {
        return Err(SolveError::StageFailed {
            stage: stage_name,
            reason: "could not align true centers",
        });
    };

    context.apply_moves(cube, moves);
    Ok(())
}

fn center_orientation<S: FaceletArray>(cube: &Cube<S>, mid: usize) -> [Facelet; 6] {
    FaceId::ALL.map(|face| cube.face(face).get(mid, mid))
}

fn solved_center_orientation() -> [Facelet; 6] {
    FaceId::ALL.map(home_facelet_for_face)
}

fn center_alignment_moves(
    start: [Facelet; 6],
    target: [Facelet; 6],
    middle_depth: usize,
) -> Option<Vec<Move>> {
    let generators = [
        Move::new(Axis::X, middle_depth, MoveAngle::Positive),
        Move::new(Axis::X, middle_depth, MoveAngle::Negative),
        Move::new(Axis::X, middle_depth, MoveAngle::Double),
        Move::new(Axis::Y, middle_depth, MoveAngle::Positive),
        Move::new(Axis::Y, middle_depth, MoveAngle::Negative),
        Move::new(Axis::Y, middle_depth, MoveAngle::Double),
        Move::new(Axis::Z, middle_depth, MoveAngle::Positive),
        Move::new(Axis::Z, middle_depth, MoveAngle::Negative),
        Move::new(Axis::Z, middle_depth, MoveAngle::Double),
    ];
    let mut seen = HashSet::new();
    let mut queue = VecDeque::new();

    seen.insert(start);
    queue.push_back((start, Vec::new()));

    while let Some((state, moves)) = queue.pop_front() {
        if state == target {
            return Some(moves);
        }

        for mv in generators {
            let next = center_orientation_after_move(state, mv);
            if !seen.insert(next) {
                continue;
            }

            let mut next_moves = moves.clone();
            next_moves.push(mv);
            queue.push_back((next, next_moves));
        }
    }

    None
}

fn center_orientation_after_move(mut state: [Facelet; 6], mv: Move) -> [Facelet; 6] {
    let original = state;

    match (mv.axis, mv.angle) {
        (Axis::X, MoveAngle::Positive) => {
            state[FaceId::U.index()] = original[FaceId::F.index()];
            state[FaceId::D.index()] = original[FaceId::B.index()];
            state[FaceId::F.index()] = original[FaceId::D.index()];
            state[FaceId::B.index()] = original[FaceId::U.index()];
        }
        (Axis::X, MoveAngle::Negative) => {
            state[FaceId::U.index()] = original[FaceId::B.index()];
            state[FaceId::D.index()] = original[FaceId::F.index()];
            state[FaceId::F.index()] = original[FaceId::U.index()];
            state[FaceId::B.index()] = original[FaceId::D.index()];
        }
        (Axis::X, MoveAngle::Double) => {
            state[FaceId::U.index()] = original[FaceId::D.index()];
            state[FaceId::D.index()] = original[FaceId::U.index()];
            state[FaceId::F.index()] = original[FaceId::B.index()];
            state[FaceId::B.index()] = original[FaceId::F.index()];
        }
        (Axis::Y, MoveAngle::Positive) => {
            state[FaceId::R.index()] = original[FaceId::B.index()];
            state[FaceId::L.index()] = original[FaceId::F.index()];
            state[FaceId::F.index()] = original[FaceId::R.index()];
            state[FaceId::B.index()] = original[FaceId::L.index()];
        }
        (Axis::Y, MoveAngle::Negative) => {
            state[FaceId::R.index()] = original[FaceId::F.index()];
            state[FaceId::L.index()] = original[FaceId::B.index()];
            state[FaceId::F.index()] = original[FaceId::L.index()];
            state[FaceId::B.index()] = original[FaceId::R.index()];
        }
        (Axis::Y, MoveAngle::Double) => {
            state[FaceId::R.index()] = original[FaceId::L.index()];
            state[FaceId::L.index()] = original[FaceId::R.index()];
            state[FaceId::F.index()] = original[FaceId::B.index()];
            state[FaceId::B.index()] = original[FaceId::F.index()];
        }
        (Axis::Z, MoveAngle::Positive) => {
            state[FaceId::U.index()] = original[FaceId::L.index()];
            state[FaceId::D.index()] = original[FaceId::R.index()];
            state[FaceId::R.index()] = original[FaceId::U.index()];
            state[FaceId::L.index()] = original[FaceId::D.index()];
        }
        (Axis::Z, MoveAngle::Negative) => {
            state[FaceId::U.index()] = original[FaceId::R.index()];
            state[FaceId::D.index()] = original[FaceId::L.index()];
            state[FaceId::R.index()] = original[FaceId::D.index()];
            state[FaceId::L.index()] = original[FaceId::U.index()];
        }
        (Axis::Z, MoveAngle::Double) => {
            state[FaceId::U.index()] = original[FaceId::D.index()];
            state[FaceId::D.index()] = original[FaceId::U.index()];
            state[FaceId::R.index()] = original[FaceId::L.index()];
            state[FaceId::L.index()] = original[FaceId::R.index()];
        }
    }

    state
}

fn push_center_transfer<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    transfer: CenterTransferSpec,
    schedule: &[CenterScheduleStep],
    columns: &mut Vec<usize>,
    stage_name: &'static str,
) -> SolveResult<()> {
    let steps = schedule
        .iter()
        .copied()
        .filter(|step| step.source == transfer.source && step.destination == transfer.destination)
        .collect::<Vec<_>>();

    if steps.is_empty() {
        return Err(SolveError::StageFailed {
            stage: stage_name,
            reason: "missing center transfer route",
        });
    }

    let mut remaining = face_center_color_count(cube, transfer.source, transfer.color);
    while remaining > 0 {
        let before = remaining;

        for _ in 0..4 {
            for step in steps.iter().copied() {
                let moved = apply_center_transfer_step(cube, context, transfer, step, columns);
                remaining = remaining
                    .checked_sub(moved)
                    .expect("center transfer moved more facelets than remain on source face");
                debug_assert_eq!(
                    remaining,
                    face_center_color_count(cube, transfer.source, transfer.color)
                );
                if remaining == 0 {
                    return Ok(());
                }
            }

            context.apply_center_face_rotation(cube, transfer.source, MoveAngle::Positive);
        }

        if remaining >= before {
            return Err(SolveError::StageFailed {
                stage: stage_name,
                reason: "center transfer made no further progress",
            });
        }
    }

    Ok(())
}

fn apply_center_transfer_step<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    transfer: CenterTransferSpec,
    step: CenterScheduleStep,
    columns: &mut Vec<usize>,
) -> usize {
    let side_length = cube.side_len();
    let Some(commutator) =
        context
            .center_commutators()
            .get(step.destination, step.helper, step.angle)
    else {
        return 0;
    };
    let mut moved = 0;

    for row in 1..side_length - 1 {
        let mut destination_rotations = 0;

        loop {
            let source_piece_count = scan_center_transfer_row(cube, transfer, step, row, columns);

            if source_piece_count == 0 {
                break;
            }

            if columns.is_empty() {
                if destination_rotations == 4 {
                    break;
                }
                context.apply_center_face_rotation(cube, transfer.destination, MoveAngle::Positive);
                destination_rotations += 1;
                continue;
            }

            moved += columns.len();
            apply_normalized_center_commutator_row(context, cube, commutator, row, columns);
            destination_rotations = 0;
        }
    }

    moved
}

fn scan_center_transfer_row<S: FaceletArray>(
    cube: &Cube<S>,
    transfer: CenterTransferSpec,
    step: CenterScheduleStep,
    row: usize,
    columns: &mut Vec<usize>,
) -> usize {
    debug_assert_eq!(step.source_location.face, transfer.source);
    debug_assert_eq!(step.destination_location.face, transfer.destination);

    let side_length = cube.side_len();
    let target = transfer.color.as_u8();
    let source_stream = CenterScanStream::bind(cube, step.source_location, row);
    let destination_stream = CenterScanStream::bind(cube, step.destination_location, row);
    let source_storage = cube.face(transfer.source).matrix().storage();
    let destination_storage = cube.face(transfer.destination).matrix().storage();
    let mut source_piece_count = 0;

    columns.clear();
    for column in 1..side_length - 1 {
        if row == column {
            continue;
        }

        let source_index = unsafe { source_stream.index_unchecked(column) };
        if unsafe { source_storage.get_unchecked_raw(source_index) } == target {
            source_piece_count += 1;

            let destination_index = unsafe { destination_stream.index_unchecked(column) };
            if unsafe { destination_storage.get_unchecked_raw(destination_index) } != target {
                columns.push(column);
            }
        }
    }

    source_piece_count
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct CenterScanStream {
    start: usize,
    step: isize,
}

impl CenterScanStream {
    fn bind<S: FaceletArray>(cube: &Cube<S>, expr: CenterLocationExpr, row: usize) -> Self {
        let start = raw_center_index_for_expr(cube, expr, row, 0);
        let next = raw_center_index_for_expr(cube, expr, row, 1);
        let start_signed = isize::try_from(start).expect("raw center index overflowed isize");
        let next_signed = isize::try_from(next).expect("raw center index overflowed isize");

        Self {
            start,
            step: next_signed - start_signed,
        }
    }

    #[inline(always)]
    unsafe fn index_unchecked(self, column: usize) -> usize {
        (self.start as isize + self.step * column as isize) as usize
    }
}

fn raw_center_index_for_expr<S: FaceletArray>(
    cube: &Cube<S>,
    expr: CenterLocationExpr,
    row: usize,
    column: usize,
) -> usize {
    let side_length = cube.side_len();
    let location = expr.eval(side_length, row, column);
    let (physical_row, physical_column) = cube
        .face(location.face)
        .physical_coords(location.row, location.column);

    physical_row
        .checked_mul(side_length)
        .and_then(|row_start| row_start.checked_add(physical_column))
        .expect("raw center index overflowed usize")
}

fn apply_normalized_center_commutator_row<S: FaceletArray>(
    context: &mut SolveContext,
    cube: &mut Cube<S>,
    commutator: FaceCommutator,
    row: usize,
    columns: &[usize],
) {
    context.apply_normalized_center_commutator(cube, commutator, &[row], columns);
}

fn centers_are_solved<S: FaceletArray>(cube: &Cube<S>) -> bool {
    FaceId::ALL
        .iter()
        .copied()
        .all(|face| face_centers_are_solved(cube, face))
}

fn face_centers_are_solved<S: FaceletArray>(cube: &Cube<S>, face: FaceId) -> bool {
    let side_length = cube.side_len();
    let target = home_facelet_for_face(face);
    let target_raw = target.as_u8();
    let storage = cube.face(face).matrix().storage();

    for row in 1..side_length.saturating_sub(1) {
        let mut index = row * side_length + 1;
        for _ in 1..side_length.saturating_sub(1) {
            if unsafe { storage.get_unchecked_raw(index) } != target_raw {
                return false;
            }
            index += 1;
        }
    }

    true
}

fn face_center_color_count<S: FaceletArray>(cube: &Cube<S>, face: FaceId, color: Facelet) -> usize {
    let mut count = 0;
    let side_length = cube.side_len();
    let target = color.as_u8();
    let storage = cube.face(face).matrix().storage();

    for row in 1..side_length.saturating_sub(1) {
        let mut index = row * side_length + 1;
        for _ in 1..side_length.saturating_sub(1) {
            count += usize::from(unsafe { storage.get_unchecked_raw(index) } == target);
            index += 1;
        }
    }

    count
}

#[cfg(test)]
fn total_center_count(side_length: usize) -> usize {
    let centers_per_face = side_length.saturating_sub(2);
    centers_per_face * centers_per_face * FaceId::ALL.len()
}

#[allow(dead_code)]
fn center_score<S: FaceletArray>(cube: &Cube<S>) -> usize {
    let mut score = 0;

    for face in FaceId::ALL {
        let target = home_facelet_for_face(face);
        for row in 1..cube.side_len().saturating_sub(1) {
            for column in 1..cube.side_len().saturating_sub(1) {
                score += usize::from(cube.face(face).get(row, column) == target);
            }
        }
    }

    score
}

#[allow(dead_code)]
fn face_center_score<S: FaceletArray>(cube: &Cube<S>, face: FaceId) -> usize {
    let target = home_facelet_for_face(face);
    let mut score = 0;

    for row in 1..cube.side_len().saturating_sub(1) {
        for column in 1..cube.side_len().saturating_sub(1) {
            score += usize::from(cube.face(face).get(row, column) == target);
        }
    }

    score
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
    use crate::{conventions::opposite_face, Byte, RandomSource, XorShift64};

    #[test]
    fn center_commutator_table_contains_only_perpendicular_helpers() {
        let table = CenterCommutatorTable::new();

        for destination in FaceId::ALL {
            assert_eq!(table.helper_count_for_destination(destination), 4);

            for helper in FaceId::ALL {
                let valid = destination != helper && destination != opposite_face(helper);
                for angle in MoveAngle::ALL {
                    assert_eq!(
                        table.get(destination, helper, angle).is_some(),
                        valid,
                        "unexpected table entry for destination={destination}, helper={helper}, angle={angle}"
                    );
                }
            }
        }
    }

    #[test]
    fn normalized_center_commutator_records_the_literal_move_count() {
        let side_length = 7;
        let rows = [1usize, 4];
        let columns = [2usize, 3, 5];
        let commutator = FaceCommutator::new(FaceId::R, FaceId::F, MoveAngle::Negative);
        let expected_total = 2 * rows.len() + 2 * columns.len() + 4;

        let mut unrecorded_cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        let mut unrecorded_context = SolveContext::new(SolveOptions {
            thread_count: 1,
            record_moves: false,
        });
        unrecorded_context.apply_normalized_center_commutator(
            &mut unrecorded_cube,
            commutator,
            &rows,
            &columns,
        );

        let stats = unrecorded_context.move_stats();
        assert_eq!(stats.total, expected_total);
        assert_eq!(stats.outer_layer, 4);
        assert_eq!(stats.inner_layer, expected_total - 4);

        let mut recorded_cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        let mut recorded_context = SolveContext::new(SolveOptions {
            thread_count: 1,
            record_moves: true,
        });
        recorded_context.apply_normalized_center_commutator(
            &mut recorded_cube,
            commutator,
            &rows,
            &columns,
        );

        assert_eq!(recorded_context.moves().len(), expected_total);
        assert_eq!(recorded_context.move_stats(), stats);
        assert!(recorded_cube.history().is_empty());
        assert!(unrecorded_cube.history().is_empty());
    }

    #[test]
    fn center_face_rotation_matches_physical_move_on_full_cube() {
        let side_length = 6;

        for face in FaceId::ALL {
            for angle in MoveAngle::ALL {
                let mut physical = patterned_cube(side_length);
                let mut optimized = physical.clone();
                let mv = face_outer_move(side_length, face, angle);
                let mut context = SolveContext::new(SolveOptions {
                    thread_count: 1,
                    record_moves: true,
                });

                physical.apply_move_untracked(mv);
                context.apply_center_face_rotation(&mut optimized, face, angle);

                assert_cubes_match(&optimized, &physical);
                assert_eq!(context.moves(), &[mv]);
                assert_eq!(context.move_stats().total, 1);
                assert!(optimized.history().is_empty());
            }
        }
    }

    #[test]
    fn move_sequence_operation_matches_literal_move_application() {
        let side_length = 5;
        let moves = [
            Move::new(Axis::X, 0, MoveAngle::Positive),
            Move::new(Axis::Y, 2, MoveAngle::Negative),
            Move::new(Axis::Z, 1, MoveAngle::Double),
        ];
        let mut expected = patterned_cube(side_length);
        expected.apply_moves_untracked_with_threads(moves, 1);

        let mut actual = patterned_cube(side_length);
        let mut context = SolveContext::new(SolveOptions {
            thread_count: 1,
            record_moves: true,
        });
        let operation = MoveSequenceOperation::new(side_length, &moves);
        context.apply_operation(&mut actual, &operation);

        assert_cubes_match(&actual, &expected);
        assert_eq!(context.moves(), &moves);
        assert_eq!(context.move_stats().total, moves.len());
        assert!(actual.history().is_empty());
    }

    #[test]
    fn center_stage_recorded_moves_replay_to_same_full_cube_state() {
        for side_length in 4..=8 {
            for seed in [0xC011_EC7u64, 0xA11_CE57u64] {
                let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                cube.scramble_random_moves(&mut rng, 120);
                let initial = cube.clone();
                let history_before = cube.history().len();
                let history_before_moves = initial.history().as_slice().to_vec();

                let mut stage = CenterReductionStage::western_default();
                let mut context = SolveContext::new(SolveOptions {
                    thread_count: 1,
                    record_moves: true,
                });

                <CenterReductionStage as SolverStage<Byte>>::run(
                    &mut stage,
                    &mut cube,
                    &mut context,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "center stage failed for replay test n={side_length}, seed={seed:#x}, score={}/{}: {error}\n{}",
                        center_score(&cube),
                        total_center_count(side_length),
                        cube.net_string(),
                    )
                });

                let mut replay = initial;
                replay.apply_moves_untracked_with_threads(context.moves().iter().copied(), 1);

                assert_cubes_match(&cube, &replay);
                assert!(centers_are_solved(&cube));
                assert_eq!(cube.history().len(), history_before);
                assert_eq!(cube.history().as_slice(), history_before_moves.as_slice());
            }
        }
    }

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
        let standard = SolveOptions::standard(3);
        assert_eq!(standard.thread_count, 3);
        assert!(standard.record_moves);
        assert_eq!(standard.execution_mode(), ExecutionMode::Standard);

        let optimized = SolveOptions::optimized(5);
        assert_eq!(optimized.thread_count, 5);
        assert!(!optimized.record_moves);
        assert_eq!(optimized.execution_mode(), ExecutionMode::Optimized);

        let switched = SolveOptions::standard(7).with_execution_mode(ExecutionMode::Optimized);
        assert_eq!(switched.thread_count, 7);
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
            ReductionSolver::<Byte>::new(SolveOptions::optimized(1)).with_stage(StandardOnlyStage);
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

    #[test]
    fn center_stage_solves_scrambled_centers_for_various_sizes() {
        for side_length in 4..=8 {
            for seed in [0xC011_EC7u64, 0xCE17_E25u64] {
                let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                scramble_centers_with_normalized_commutators(&mut cube, &mut rng, 1);

                let mut stage = CenterReductionStage::western_default();
                let mut context = SolveContext::new(SolveOptions {
                    thread_count: 1,
                    record_moves: true,
                });

                <CenterReductionStage as SolverStage<Byte>>::run(
                    &mut stage,
                    &mut cube,
                    &mut context,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "center stage failed for n={side_length}, seed={seed:#x}, score={}/{}: {error}\n{}",
                        center_score(&cube),
                        total_center_count(side_length),
                        cube.net_string(),
                    )
                });

                assert!(
                    centers_are_solved(&cube),
                    "centers not solved for n={side_length}, seed={seed:#x}, score={}/{}",
                    center_score(&cube),
                    total_center_count(side_length),
                );
            }
        }
    }

    #[test]
    fn center_stage_solves_random_move_scrambled_centers() {
        for side_length in 4..=8 {
            for seed in [0xA11_CE57u64, 0xBADC_0DEu64] {
                let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                let moves = (0..120)
                    .map(|_| cube.random_move(&mut rng))
                    .collect::<Vec<_>>();
                cube.apply_moves_untracked_with_threads(moves, 1);

                let mut stage = CenterReductionStage::western_default();
                let mut context = SolveContext::new(SolveOptions {
                    thread_count: 1,
                    record_moves: false,
                });

                <CenterReductionStage as SolverStage<Byte>>::run(
                    &mut stage,
                    &mut cube,
                    &mut context,
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "center stage failed for random-move scramble n={side_length}, seed={seed:#x}, score={}/{}: {error}\n{}",
                        center_score(&cube),
                        total_center_count(side_length),
                        cube.net_string(),
                    )
                });

                assert!(
                    centers_are_solved(&cube),
                    "centers not solved for random-move scramble n={side_length}, seed={seed:#x}, score={}/{}",
                    center_score(&cube),
                    total_center_count(side_length),
                );
            }
        }
    }

    #[test]
    fn center_stage_solves_dense_batched_commutator_scramble() {
        let side_length = 8;
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        scramble_dense_center_route(&mut cube);

        let mut stage = CenterReductionStage::western_default();
        let mut context = SolveContext::new(SolveOptions {
            thread_count: 1,
            record_moves: false,
        });

        <CenterReductionStage as SolverStage<Byte>>::run(&mut stage, &mut cube, &mut context)
            .unwrap_or_else(|error| {
                panic!(
                    "center stage failed for dense batched scramble, score={}/{}: {error}\n{}",
                    center_score(&cube),
                    total_center_count(side_length),
                    cube.net_string(),
                )
            });

        assert!(centers_are_solved(&cube));
    }

    #[test]
    fn true_center_alignment_model_matches_middle_slice_moves() {
        let side_length = 5;
        let middle = side_length / 2;
        let start = solved_center_orientation();

        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for angle in MoveAngle::ALL {
                let mv = Move::new(axis, middle, angle);
                let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                cube.apply_move_untracked_with_threads(mv, 1);

                assert_eq!(
                    center_orientation_after_move(start, mv),
                    center_orientation(&cube, middle),
                    "center alignment model differs for {mv}",
                );
            }
        }
    }

    fn scramble_centers_with_normalized_commutators(
        cube: &mut Cube<Byte>,
        rng: &mut XorShift64,
        count: usize,
    ) {
        let table = CenterCommutatorTable::new();
        let mut applied = 0;

        while applied < count {
            let step = GENERATED_CENTER_SCHEDULE
                [(rng.next_u64() as usize) % GENERATED_CENTER_SCHEDULE.len()];
            let row = 1 + (rng.next_u64() as usize) % (cube.side_len() - 2);
            let column = 1 + (rng.next_u64() as usize) % (cube.side_len() - 2);

            if row == column {
                continue;
            }
            let Some(commutator) = table.get(step.destination, step.helper, step.angle) else {
                continue;
            };

            for _ in 0..2 {
                let rows = [row];
                let columns = [column];
                let plan = cube.normalized_face_commutator_plan(commutator, &rows, &columns);
                cube.apply_face_commutator_plan_untracked(plan);
            }
            applied += 1;
        }
    }

    fn scramble_dense_center_route(cube: &mut Cube<Byte>) {
        let table = CenterCommutatorTable::new();
        let step = GENERATED_CENTER_SCHEDULE[0];
        let commutator = table
            .get(step.destination, step.helper, step.angle)
            .expect("generated step must have a commutator");
        let side_length = cube.side_len();
        let mut columns = Vec::with_capacity(side_length.saturating_sub(3));

        for row in 1..side_length - 1 {
            columns.clear();
            columns.extend((1..side_length - 1).filter(|column| *column != row));

            for _ in 0..2 {
                let rows = [row];
                let plan = cube.normalized_face_commutator_plan(commutator, &rows, &columns);
                cube.apply_face_commutator_plan_untracked(plan);
            }
        }
    }

    fn patterned_cube(side_length: usize) -> Cube<Byte> {
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);

        for face in FaceId::ALL {
            for row in 0..side_length {
                for column in 0..side_length {
                    let value = (face.index() + row * 2 + column * 3) % FaceId::ALL.len();
                    cube.face_mut(face)
                        .set(row, column, Facelet::from_u8(value as u8));
                }
            }
        }

        cube
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

    #[test]
    fn center_stage_default_transfer_order_is_explicit() {
        let stage = CenterReductionStage::western_default();

        assert_eq!(stage.transfers().len(), 15);
        assert_eq!(stage.schedule().len(), GENERATED_CENTER_SCHEDULE.len());
        assert_eq!(
            stage.transfers()[0],
            CenterTransferSpec::new(FaceId::F, FaceId::R, Facelet::Red)
        );
        assert_eq!(
            stage.transfers()[14],
            CenterTransferSpec::new(FaceId::U, FaceId::B, Facelet::Blue)
        );
    }

    #[test]
    fn generated_center_schedule_is_compact_and_ordered_by_face_pair() {
        assert_eq!(GENERATED_CENTER_SCHEDULE.len(), 72);

        for window in GENERATED_CENTER_SCHEDULE.windows(2) {
            let left = center_schedule_sort_key(window[0]);
            let right = center_schedule_sort_key(window[1]);
            assert!(
                left <= right,
                "generated center schedule is out of order: {:?} before {:?}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn generated_center_schedule_matches_normalized_sparse_updates() {
        let cube = Cube::<Byte>::new_solved_with_threads(9, 1);
        let table = CenterCommutatorTable::new();
        let row = 2usize;
        let column = 5usize;

        for step in GENERATED_CENTER_SCHEDULE.iter().copied() {
            let commutator = table
                .get(step.destination, step.helper, step.angle)
                .expect("generated step must have a commutator");
            let updates = cube.normalized_face_commutator_sparse_updates(commutator, row, column);
            let transfer = updates
                .into_iter()
                .find(|update| {
                    update.from.face == step.source && update.to.face == step.destination
                })
                .expect("generated step must correspond to a source->destination sparse update");

            let expected_source = step.source_location.eval(cube.side_len(), row, column);
            let expected_destination = step.destination_location.eval(cube.side_len(), row, column);

            assert_eq!(transfer.from.face, expected_source.face);
            assert_eq!(transfer.from.row, expected_source.row);
            assert_eq!(transfer.from.col, expected_source.column);
            assert_eq!(transfer.to.face, expected_destination.face);
            assert_eq!(transfer.to.row, expected_destination.row);
            assert_eq!(transfer.to.col, expected_destination.column);
        }
    }

    fn center_schedule_sort_key(
        step: CenterScheduleStep,
    ) -> (usize, usize, usize, usize, usize, usize, usize, usize) {
        (
            step.destination.index(),
            step.source.index(),
            step.helper.index(),
            step.angle.as_u8() as usize,
            center_coord_expr_sort_key(step.source_location.row),
            center_coord_expr_sort_key(step.source_location.column),
            center_coord_expr_sort_key(step.destination_location.row),
            center_coord_expr_sort_key(step.destination_location.column),
        )
    }

    fn center_coord_expr_sort_key(expr: CenterCoordExpr) -> usize {
        match expr {
            CenterCoordExpr::Row => 0,
            CenterCoordExpr::Column => 1,
            CenterCoordExpr::ReverseRow => 2,
            CenterCoordExpr::ReverseColumn => 3,
        }
    }
}
