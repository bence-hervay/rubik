use core::fmt;
use std::collections::{HashSet, VecDeque};

mod center_schedule;

use crate::{
    cube::{Cube, FaceCommutator},
    face::FaceId,
    facelet::Facelet,
    moves::{Axis, Move, MoveAngle},
    storage::FaceletArray,
    threading::default_thread_count,
};

pub use center_schedule::{
    CenterCoordExpr, CenterLocation, CenterLocationExpr, CenterScheduleStep,
    GENERATED_CENTER_SCHEDULE,
};

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
    Edges,
    ThreeByThree,
}

impl fmt::Display for SolvePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Centers => f.write_str("centers"),
            Self::Edges => f.write_str("edges"),
            Self::ThreeByThree => f.write_str("3x3"),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SolveOptions {
    pub thread_count: usize,
    pub record_moves: bool,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            thread_count: default_thread_count(),
            record_moves: true,
        }
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

    pub fn center_commutators(&self) -> &CenterCommutatorTable {
        &self.center_commutators
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    pub fn move_stats(&self) -> MoveStats {
        self.move_stats
    }

    pub fn into_moves(self) -> MoveSequence {
        self.moves
    }

    pub fn apply_move<S: FaceletArray>(&mut self, cube: &mut Cube<S>, mv: Move) {
        self.move_stats.record(mv, cube.side_len());
        if self.options.record_moves {
            cube.apply_move_with_threads(mv, self.options.thread_count);
            self.moves.push(mv);
        } else {
            cube.apply_move_untracked_with_threads(mv, self.options.thread_count);
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
        if self.options.record_moves {
            let literal_moves = cube.face_commutator_moves(
                commutator.destination(),
                commutator.helper(),
                rows,
                columns,
                commutator.slice_angle(),
            );
            self.move_stats
                .record_all(literal_moves.iter().copied(), cube.side_len());
            self.moves.extend(literal_moves);
        } else {
            record_center_commutator_move_stats(
                &mut self.move_stats,
                cube.side_len(),
                commutator,
                rows,
                columns,
            );
        }

        cube.apply_face_commutator_plan_untracked(commutator, rows, columns);
    }

    pub fn apply_normalized_center_commutator<S: FaceletArray>(
        &mut self,
        cube: &mut Cube<S>,
        commutator: FaceCommutator,
        rows: &[usize],
        columns: &[usize],
    ) {
        if self.options.record_moves {
            let literal_moves = cube.normalized_face_commutator_moves(
                commutator.destination(),
                commutator.helper(),
                rows,
                columns,
                commutator.slice_angle(),
            );
            self.move_stats
                .record_all(literal_moves.iter().copied(), cube.side_len());
            self.moves.extend(literal_moves);
        } else {
            record_normalized_center_commutator_move_stats(
                &mut self.move_stats,
                cube.side_len(),
                commutator,
                rows,
                columns,
            );
        }

        cube.apply_normalized_face_commutator_plan_untracked(commutator, rows, columns);
    }
}

fn record_center_commutator_move_stats(
    stats: &mut MoveStats,
    side_length: usize,
    commutator: FaceCommutator,
    rows: &[usize],
    columns: &[usize],
) {
    let reverse = commutator.slice_angle().inverse();

    for column in columns.iter().copied() {
        stats.record(
            face_layer_move(side_length, commutator.helper(), column, reverse),
            side_length,
        );
    }
    stats.record(
        face_layer_move(
            side_length,
            commutator.destination(),
            0,
            MoveAngle::Positive,
        ),
        side_length,
    );
    for row in rows.iter().copied() {
        stats.record(
            face_layer_move(side_length, commutator.helper(), row, reverse),
            side_length,
        );
    }
    stats.record(
        face_layer_move(
            side_length,
            commutator.destination(),
            0,
            MoveAngle::Negative,
        ),
        side_length,
    );
    for column in columns.iter().copied() {
        stats.record(
            face_layer_move(
                side_length,
                commutator.helper(),
                column,
                commutator.slice_angle(),
            ),
            side_length,
        );
    }
    stats.record(
        face_layer_move(
            side_length,
            commutator.destination(),
            0,
            MoveAngle::Positive,
        ),
        side_length,
    );
    for row in rows.iter().copied() {
        stats.record(
            face_layer_move(
                side_length,
                commutator.helper(),
                row,
                commutator.slice_angle(),
            ),
            side_length,
        );
    }
}

fn record_normalized_center_commutator_move_stats(
    stats: &mut MoveStats,
    side_length: usize,
    commutator: FaceCommutator,
    rows: &[usize],
    columns: &[usize],
) {
    record_center_commutator_move_stats(stats, side_length, commutator, rows, columns);
    stats.record(
        face_outer_move(side_length, commutator.destination(), MoveAngle::Positive).inverse(),
        side_length,
    );
}

pub trait Solver<S: FaceletArray> {
    fn solve(&mut self, cube: &mut Cube<S>) -> SolveResult<SolveOutcome>;
}

pub trait SolverStage<S: FaceletArray> {
    fn phase(&self) -> SolvePhase;
    fn name(&self) -> &'static str;
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
            .with_stage(EdgePairingStage::default())
            .with_stage(ThreeByThreeStage::default())
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

        for stage in &mut self.stages {
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

        Ok(SolveOutcome {
            moves: context.into_moves(),
            reports,
        })
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct AngleCommutators {
    positive: FaceCommutator,
    double: FaceCommutator,
    negative: FaceCommutator,
}

impl AngleCommutators {
    fn new(destination: FaceId, helper: FaceId) -> Self {
        Self {
            positive: FaceCommutator::new(destination, helper, MoveAngle::Positive),
            double: FaceCommutator::new(destination, helper, MoveAngle::Double),
            negative: FaceCommutator::new(destination, helper, MoveAngle::Negative),
        }
    }

    fn get(self, angle: MoveAngle) -> FaceCommutator {
        match angle {
            MoveAngle::Positive => self.positive,
            MoveAngle::Double => self.double,
            MoveAngle::Negative => self.negative,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct CenterCommutatorTable {
    entries: [[Option<AngleCommutators>; 6]; 6],
}

impl CenterCommutatorTable {
    pub fn new() -> Self {
        let mut entries = [[None; 6]; 6];

        for destination in FaceId::ALL {
            for helper in FaceId::ALL {
                if destination == helper || destination == opposite_face(helper) {
                    continue;
                }

                entries[destination.index()][helper.index()] =
                    Some(AngleCommutators::new(destination, helper));
            }
        }

        Self { entries }
    }

    pub fn get(
        &self,
        destination: FaceId,
        helper: FaceId,
        angle: MoveAngle,
    ) -> Option<FaceCommutator> {
        self.entries[destination.index()][helper.index()].map(|entry| entry.get(angle))
    }

    pub fn helper_count_for_destination(&self, destination: FaceId) -> usize {
        self.entries[destination.index()]
            .iter()
            .filter(|entry| entry.is_some())
            .count()
    }
}

impl Default for CenterCommutatorTable {
    fn default() -> Self {
        Self::new()
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
    FaceId::ALL.map(target_center_color)
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
                apply_center_transfer_step(cube, context, transfer, step, columns);
                remaining = face_center_color_count(cube, transfer.source, transfer.color);
                if remaining == 0 {
                    return Ok(());
                }
            }

            context.apply_move(
                cube,
                face_outer_move(cube.side_len(), transfer.source, MoveAngle::Positive),
            );
        }

        remaining = face_center_color_count(cube, transfer.source, transfer.color);
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
) {
    let side_length = cube.side_len();
    let Some(commutator) =
        context
            .center_commutators()
            .get(step.destination, step.helper, step.angle)
    else {
        return;
    };

    for row in 1..side_length - 1 {
        let mut destination_rotations = 0;

        loop {
            let mut source_piece_count = 0;
            columns.clear();

            for column in 1..side_length - 1 {
                if row == column {
                    continue;
                }

                let source = step.source_location.eval(side_length, row, column);
                let destination = step.destination_location.eval(side_length, row, column);

                debug_assert_eq!(source.face, transfer.source);
                debug_assert_eq!(destination.face, transfer.destination);

                if cube.face(source.face).get(source.row, source.column) == transfer.color {
                    source_piece_count += 1;
                    if cube
                        .face(destination.face)
                        .get(destination.row, destination.column)
                        != transfer.color
                    {
                        columns.push(column);
                    }
                }
            }

            if source_piece_count == 0 {
                break;
            }

            if columns.is_empty() {
                if destination_rotations == 4 {
                    break;
                }
                context.apply_move(
                    cube,
                    face_outer_move(side_length, transfer.destination, MoveAngle::Positive),
                );
                destination_rotations += 1;
                continue;
            }

            apply_normalized_center_commutator_row(context, cube, commutator, row, columns);
            destination_rotations = 0;
        }
    }
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
    let target = target_center_color(face);

    for row in 1..side_length.saturating_sub(1) {
        for column in 1..side_length.saturating_sub(1) {
            if cube.face(face).get(row, column) != target {
                return false;
            }
        }
    }

    true
}

fn face_center_color_count<S: FaceletArray>(cube: &Cube<S>, face: FaceId, color: Facelet) -> usize {
    let mut count = 0;

    for row in 1..cube.side_len().saturating_sub(1) {
        for column in 1..cube.side_len().saturating_sub(1) {
            count += usize::from(cube.face(face).get(row, column) == color);
        }
    }

    count
}

#[cfg(test)]
fn total_center_count(side_length: usize) -> usize {
    let centers_per_face = side_length.saturating_sub(2);
    centers_per_face * centers_per_face * FaceId::ALL.len()
}

fn target_center_color(face: FaceId) -> Facelet {
    Facelet::from_u8(face.index() as u8)
}

fn face_outer_move(side_length: usize, face: FaceId, angle: MoveAngle) -> Move {
    face_layer_move(side_length, face, 0, angle)
}

fn face_layer_move(
    side_length: usize,
    face: FaceId,
    depth_from_face: usize,
    angle: MoveAngle,
) -> Move {
    let last = side_length - 1;

    match face {
        FaceId::U => Move::new(Axis::Y, last - depth_from_face, angle),
        FaceId::D => Move::new(Axis::Y, depth_from_face, angle.inverse()),
        FaceId::R => Move::new(Axis::X, last - depth_from_face, angle),
        FaceId::L => Move::new(Axis::X, depth_from_face, angle.inverse()),
        FaceId::F => Move::new(Axis::Z, last - depth_from_face, angle),
        FaceId::B => Move::new(Axis::Z, depth_from_face, angle.inverse()),
    }
}

#[allow(dead_code)]
fn center_score<S: FaceletArray>(cube: &Cube<S>) -> usize {
    let mut score = 0;

    for face in FaceId::ALL {
        let target = target_center_color(face);
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
    let target = target_center_color(face);
    let mut score = 0;

    for row in 1..cube.side_len().saturating_sub(1) {
        for column in 1..cube.side_len().saturating_sub(1) {
            score += usize::from(cube.face(face).get(row, column) == target);
        }
    }

    score
}

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum EdgeSlot {
    UF = 0,
    UR = 1,
    UB = 2,
    UL = 3,
    FR = 4,
    FL = 5,
    BR = 6,
    BL = 7,
    DF = 8,
    DR = 9,
    DB = 10,
    DL = 11,
}

impl EdgeSlot {
    pub const ALL: [Self; 12] = [
        Self::UF,
        Self::UR,
        Self::UB,
        Self::UL,
        Self::FR,
        Self::FL,
        Self::BR,
        Self::BL,
        Self::DF,
        Self::DR,
        Self::DB,
        Self::DL,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdgePairingStage {
    slots: [EdgeSlot; 12],
    sub_stages: [SubStageSpec; 3],
}

impl Default for EdgePairingStage {
    fn default() -> Self {
        Self {
            slots: EdgeSlot::ALL,
            sub_stages: [
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "edge orientation tracking",
                    "maintain edge slot state while setup moves relocate edge bands",
                ),
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "edge pairing scans",
                    "scan source and destination edge rows and batch compatible swaps",
                ),
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "edge parity handling",
                    "reserve hooks for even-cube and odd-cube parity correction",
                ),
            ],
        }
    }
}

impl EdgePairingStage {
    pub fn slots(&self) -> &[EdgeSlot; 12] {
        &self.slots
    }
}

impl<S: FaceletArray> SolverStage<S> for EdgePairingStage {
    fn phase(&self) -> SolvePhase {
        SolvePhase::Edges
    }

    fn name(&self) -> &'static str {
        "edge pairing"
    }

    fn sub_stages(&self) -> &[SubStageSpec] {
        &self.sub_stages
    }

    fn run(&mut self, _cube: &mut Cube<S>, _context: &mut SolveContext) -> SolveResult<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreeByThreeStage {
    sub_stages: [SubStageSpec; 2],
}

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

    fn sub_stages(&self) -> &[SubStageSpec] {
        &self.sub_stages
    }

    fn run(&mut self, _cube: &mut Cube<S>, _context: &mut SolveContext) -> SolveResult<()> {
        Ok(())
    }
}

fn opposite_face(face: FaceId) -> FaceId {
    match face {
        FaceId::U => FaceId::D,
        FaceId::D => FaceId::U,
        FaceId::R => FaceId::L,
        FaceId::L => FaceId::R,
        FaceId::F => FaceId::B,
        FaceId::B => FaceId::F,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Byte, RandomSource, XorShift64};

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
    }

    #[test]
    fn default_reduction_solver_has_center_edge_and_3x3_stages() {
        let solver = ReductionSolver::<Byte>::large_cube_default();
        let names = solver.stage_names().collect::<Vec<_>>();

        assert_eq!(solver.stage_count(), 3);
        assert_eq!(names, ["center reduction", "edge pairing", "3x3 finish"]);
    }

    #[test]
    fn placeholder_pipeline_runs_without_adding_moves() {
        let mut solver = ReductionSolver::<Byte>::large_cube_default();
        let mut cube = Cube::<Byte>::new_solved(5);

        let outcome = solver
            .solve(&mut cube)
            .expect("placeholder stages should run");

        assert!(outcome.moves.is_empty());
        assert_eq!(outcome.reports.len(), 3);
        assert_eq!(outcome.reports[0].phase, SolvePhase::Centers);
        assert_eq!(outcome.reports[1].phase, SolvePhase::Edges);
        assert_eq!(outcome.reports[2].phase, SolvePhase::ThreeByThree);
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
                cube.apply_normalized_face_commutator_plan_untracked(commutator, &[row], &[column]);
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
                cube.apply_normalized_face_commutator_plan_untracked(commutator, &[row], &columns);
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
