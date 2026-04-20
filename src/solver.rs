use core::fmt;

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
    CenterCoordExpr, CenterLocation, CenterLocationExpr, CenterQuadrant, CenterScheduleStep,
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

#[derive(Clone, Debug)]
pub struct SolveContext {
    options: SolveOptions,
    center_commutators: CenterCommutatorTable,
    moves: MoveSequence,
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

    pub fn into_moves(self) -> MoveSequence {
        self.moves
    }

    pub fn apply_move<S: FaceletArray>(&mut self, cube: &mut Cube<S>, mv: Move) {
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
            self.moves.extend(literal_moves);
        }

        cube.apply_face_commutator_plan_untracked(commutator, rows, columns);
    }
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
            let moves_before = context.moves().len();
            stage.run(cube, &mut context)?;
            let moves_after = context.moves().len();

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
        solve_centers_with_schedule(cube, context, self.schedule, "center reduction")
    }
}

fn solve_centers_with_schedule<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    schedule: &[CenterScheduleStep],
    stage_name: &'static str,
) -> SolveResult<()> {
    if cube.side_len() < 4 || centers_are_solved(cube) {
        return Ok(());
    }

    let max_passes = total_center_count(cube.side_len())
        .checked_mul(2)
        .expect("center schedule pass limit overflowed usize");

    for _ in 0..max_passes {
        if centers_are_solved(cube) {
            return Ok(());
        }

        let mut changed = false;
        for step in schedule {
            changed |= apply_center_schedule_step(cube, context, *step);
        }

        if !changed {
            break;
        }
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

fn apply_center_schedule_step<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    step: CenterScheduleStep,
) -> bool {
    let side_length = cube.side_len();
    let target = target_center_color(step.destination);
    let Some(commutator) =
        context
            .center_commutators()
            .get(step.destination, step.helper, step.angle)
    else {
        return false;
    };
    let mut changed = false;

    for row in step.row_quadrant.rows(side_length) {
        for column in step.column_quadrant.columns(side_length) {
            if row == column {
                continue;
            }

            let source = step.source_location.eval(side_length, row, column);
            let destination = step.destination_location.eval(side_length, row, column);

            if cube.face(source.face).get(source.row, source.column) == target
                && cube
                    .face(destination.face)
                    .get(destination.row, destination.column)
                    != target
            {
                apply_normalized_center_commutator_parts(context, cube, commutator, row, column);
                changed = true;
            }
        }
    }

    changed
}

fn apply_normalized_center_commutator_parts<S: FaceletArray>(
    context: &mut SolveContext,
    cube: &mut Cube<S>,
    commutator: FaceCommutator,
    row: usize,
    column: usize,
) {
    context.apply_center_commutator(cube, commutator, &[row], &[column]);
    context.apply_move(
        cube,
        face_outer_move(
            cube.side_len(),
            commutator.destination(),
            MoveAngle::Positive,
        )
        .inverse(),
    );
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

fn total_center_count(side_length: usize) -> usize {
    let centers_per_face = side_length.saturating_sub(2);
    centers_per_face * centers_per_face * FaceId::ALL.len()
}

fn target_center_color(face: FaceId) -> Facelet {
    Facelet::from_u8(face.index() as u8)
}

fn face_outer_move(side_length: usize, face: FaceId, angle: MoveAngle) -> Move {
    let last = side_length - 1;

    match face {
        FaceId::U => Move::new(Axis::Y, last, angle),
        FaceId::D => Move::new(Axis::Y, 0, angle.inverse()),
        FaceId::R => Move::new(Axis::X, last, angle),
        FaceId::L => Move::new(Axis::X, 0, angle.inverse()),
        FaceId::F => Move::new(Axis::Z, last, angle),
        FaceId::B => Move::new(Axis::Z, 0, angle.inverse()),
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
            let rows = step.row_quadrant.rows(cube.side_len()).collect::<Vec<_>>();
            let columns = step
                .column_quadrant
                .columns(cube.side_len())
                .collect::<Vec<_>>();
            let row = rows[(rng.next_u64() as usize) % rows.len()];
            let column = columns[(rng.next_u64() as usize) % columns.len()];

            if row == column {
                continue;
            }
            let Some(commutator) = table.get(step.destination, step.helper, step.angle) else {
                continue;
            };

            for _ in 0..2 {
                cube.apply_face_commutator_plan_untracked(commutator, &[row], &[column]);
                cube.apply_move_untracked_with_threads(
                    face_outer_move(cube.side_len(), step.destination, MoveAngle::Positive)
                        .inverse(),
                    1,
                );
            }
            applied += 1;
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
    fn generated_center_schedule_is_ordered_by_face_quadrant_and_source() {
        assert_eq!(GENERATED_CENTER_SCHEDULE.len(), 1_152);

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
    ) -> (usize, usize, usize, usize, usize, usize) {
        (
            step.destination.index(),
            step.row_quadrant.index(),
            step.column_quadrant.index(),
            step.source.index(),
            step.helper.index(),
            step.angle.as_u8() as usize,
        )
    }
}
