use core::fmt;

use crate::{
    cube::{Cube, FaceCommutator, FaceletLocation, FaceletUpdate},
    face::FaceId,
    facelet::Facelet,
    moves::{Axis, Move, MoveAngle},
    storage::FaceletArray,
    threading::default_thread_count,
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
        solve_centers_with_commutators(cube, context, "center reduction")
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct CenterCommutatorStep {
    commutator: FaceCommutator,
    row: usize,
    column: usize,
    score_delta: isize,
    trapped_delta: isize,
}

fn solve_centers_with_commutators<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    stage_name: &'static str,
) -> SolveResult<()> {
    let side_length = cube.side_len();
    if side_length < 3 || centers_are_solved(cube) {
        Ok(())
    } else {
        let center_count = total_center_count(side_length);
        let max_steps = center_count
            .checked_mul(100)
            .expect("center commutator step limit overflowed usize");

        for _ in 0..max_steps {
            if centers_are_solved(cube) {
                return Ok(());
            }

            let Some(step) = best_center_commutator_step(cube, context.center_commutators()) else {
                if let Some(setup_move) =
                    find_center_move_setup_move(cube, context.center_commutators())
                {
                    context.apply_move(cube, setup_move);
                    continue;
                }

                return Err(SolveError::StageFailed {
                    stage: stage_name,
                    reason: "no improving center commutator or setup step was found",
                });
            };

            let score_before = center_score(cube);
            apply_normalized_center_commutator(context, cube, step);
            debug_assert_eq!(
                center_score(cube) as isize - score_before as isize,
                step.score_delta,
                "predicted center score delta did not match the applied commutator: {step:?}"
            );
        }

        Err(SolveError::StageFailed {
            stage: stage_name,
            reason: "center commutator step limit reached",
        })
    }
}

fn best_center_commutator_step<S: FaceletArray>(
    cube: &Cube<S>,
    table: &CenterCommutatorTable,
) -> Option<CenterCommutatorStep> {
    best_improving_center_commutator_step(cube, table)
        .or_else(|| opposite_face_route_center_step(cube, table))
        .or_else(|| lookahead_center_commutator_setup_step(cube, table))
}

fn apply_normalized_center_commutator<S: FaceletArray>(
    context: &mut SolveContext,
    cube: &mut Cube<S>,
    step: CenterCommutatorStep,
) {
    context.apply_center_commutator(cube, step.commutator, &[step.row], &[step.column]);
    context.apply_move(
        cube,
        face_outer_move(
            cube.side_len(),
            step.commutator.destination(),
            MoveAngle::Positive,
        )
        .inverse(),
    );
}

fn apply_normalized_center_commutator_untracked<S: FaceletArray>(
    cube: &mut Cube<S>,
    step: CenterCommutatorStep,
) {
    cube.apply_face_commutator_plan_untracked(step.commutator, &[step.row], &[step.column]);
    cube.apply_move_untracked_with_threads(
        face_outer_move(
            cube.side_len(),
            step.commutator.destination(),
            MoveAngle::Positive,
        )
        .inverse(),
        1,
    );
}

fn best_improving_center_commutator_step<S: FaceletArray>(
    cube: &Cube<S>,
    table: &CenterCommutatorTable,
) -> Option<CenterCommutatorStep> {
    let side_length = cube.side_len();
    let mut best_improving = None;

    for destination in FaceId::ALL {
        for helper in FaceId::ALL {
            for angle in MoveAngle::ALL {
                let Some(commutator) = table.get(destination, helper, angle) else {
                    continue;
                };

                for row in 1..side_length - 1 {
                    for column in 1..side_length - 1 {
                        if row == column {
                            continue;
                        }

                        let updates = cube.face_commutator_sparse_updates(commutator, row, column);
                        let score_delta = center_score_delta_after_normalized_commutator(
                            cube,
                            destination,
                            updates,
                        );
                        let trapped_delta = center_trapped_delta_after_normalized_commutator(
                            cube,
                            destination,
                            updates,
                        );

                        let step = CenterCommutatorStep {
                            commutator,
                            row,
                            column,
                            score_delta,
                            trapped_delta,
                        };

                        if score_delta > 0 {
                            if better_improving_center_step(step, best_improving) {
                                best_improving = Some(step);
                            }
                        }
                    }
                }
            }
        }
    }

    best_improving
}

fn better_improving_center_step(
    step: CenterCommutatorStep,
    best: Option<CenterCommutatorStep>,
) -> bool {
    best.map(|best| {
        step.score_delta > best.score_delta
            || (step.score_delta == best.score_delta && step.trapped_delta < best.trapped_delta)
            || (step.score_delta == best.score_delta
                && step.trapped_delta == best.trapped_delta
                && step.commutator.destination().index() < best.commutator.destination().index())
    })
    .unwrap_or(true)
}

fn better_setup_center_step(
    step: CenterCommutatorStep,
    best: Option<CenterCommutatorStep>,
) -> bool {
    best.map(|best| {
        step.trapped_delta < best.trapped_delta
            || (step.trapped_delta == best.trapped_delta
                && step.commutator.destination().index() < best.commutator.destination().index())
    })
    .unwrap_or(true)
}

fn opposite_face_route_center_step<S: FaceletArray>(
    cube: &Cube<S>,
    table: &CenterCommutatorTable,
) -> Option<CenterCommutatorStep> {
    for target_face in FaceId::ALL {
        if face_centers_are_solved(cube, target_face) {
            continue;
        }

        let source_face = opposite_face(target_face);
        let color = target_center_color(target_face);
        if !face_contains_center_color(cube, source_face, color) {
            continue;
        }

        if let Some(step) = route_center_through_intermediate(cube, table, target_face, source_face)
        {
            return Some(step);
        }
    }

    None
}

fn route_center_through_intermediate<S: FaceletArray>(
    cube: &Cube<S>,
    table: &CenterCommutatorTable,
    target_face: FaceId,
    source_face: FaceId,
) -> Option<CenterCommutatorStep> {
    let color = target_center_color(target_face);

    for require_unsolved_destination in [true, false] {
        for intermediate in FaceId::ALL {
            if intermediate == target_face || intermediate == source_face {
                continue;
            }
            if require_unsolved_destination && face_centers_are_solved(cube, intermediate) {
                continue;
            }

            if let Some(step) =
                find_center_transfer_step(cube, table, intermediate, source_face, color, true, true)
                    .or_else(|| {
                        find_center_transfer_step(
                            cube,
                            table,
                            intermediate,
                            source_face,
                            color,
                            false,
                            true,
                        )
                    })
            {
                return Some(step);
            }
        }
    }

    None
}

fn find_center_transfer_step<S: FaceletArray>(
    cube: &Cube<S>,
    table: &CenterCommutatorTable,
    destination: FaceId,
    source: FaceId,
    color: Facelet,
    require_destination_mismatch: bool,
    require_followup_improvement: bool,
) -> Option<CenterCommutatorStep> {
    let side_length = cube.side_len();
    let mut best: Option<CenterCommutatorStep> = None;

    for helper in FaceId::ALL {
        for angle in MoveAngle::ALL {
            let Some(commutator) = table.get(destination, helper, angle) else {
                continue;
            };

            for row in 1..side_length - 1 {
                for column in 1..side_length - 1 {
                    if row == column {
                        continue;
                    }

                    let updates = cube.face_commutator_sparse_updates(commutator, row, column);
                    let transfers_requested_color = updates.iter().copied().any(|update| {
                        let final_destination =
                            normalized_update_destination(side_length, destination, update);

                        update.from.face == source
                            && final_destination.face == destination
                            && value_after_destination_turn(cube, destination, update.from) == color
                            && (!require_destination_mismatch
                                || cube
                                    .face(final_destination.face)
                                    .get(final_destination.row, final_destination.col)
                                    != target_center_color(destination))
                    });
                    if !transfers_requested_color {
                        continue;
                    }

                    let step = CenterCommutatorStep {
                        commutator,
                        row,
                        column,
                        score_delta: center_score_delta_after_normalized_commutator(
                            cube,
                            destination,
                            updates,
                        ),
                        trapped_delta: center_trapped_delta_after_normalized_commutator(
                            cube,
                            destination,
                            updates,
                        ),
                    };

                    if require_followup_improvement {
                        let mut trial = cube.clone();
                        apply_normalized_center_commutator_untracked(&mut trial, step);
                        if best_improving_center_commutator_step(&trial, table).is_none() {
                            continue;
                        }
                    }

                    if better_setup_center_step(step, best) {
                        best = Some(step);
                    }
                }
            }
        }
    }

    best
}

fn lookahead_center_commutator_setup_step<S: FaceletArray>(
    cube: &Cube<S>,
    table: &CenterCommutatorTable,
) -> Option<CenterCommutatorStep> {
    let side_length = cube.side_len();
    let mut best: Option<CenterCommutatorStep> = None;
    let mut best_net_delta = 0;

    for destination in FaceId::ALL {
        for helper in FaceId::ALL {
            for angle in MoveAngle::ALL {
                let Some(commutator) = table.get(destination, helper, angle) else {
                    continue;
                };

                for row in 1..side_length - 1 {
                    for column in 1..side_length - 1 {
                        if row == column {
                            continue;
                        }

                        let updates = cube.face_commutator_sparse_updates(commutator, row, column);
                        let step = CenterCommutatorStep {
                            commutator,
                            row,
                            column,
                            score_delta: center_score_delta_after_normalized_commutator(
                                cube,
                                destination,
                                updates,
                            ),
                            trapped_delta: center_trapped_delta_after_normalized_commutator(
                                cube,
                                destination,
                                updates,
                            ),
                        };

                        if step.score_delta > 0 || step.score_delta < -2 {
                            continue;
                        }

                        let mut trial = cube.clone();
                        apply_normalized_center_commutator_untracked(&mut trial, step);

                        let Some(next) = best_improving_center_commutator_step(&trial, table)
                        else {
                            continue;
                        };
                        let net_delta = step.score_delta + next.score_delta;
                        if net_delta <= 0 {
                            continue;
                        }

                        if best.is_none()
                            || net_delta > best_net_delta
                            || (net_delta == best_net_delta
                                && step.trapped_delta
                                    < best.expect("best is initialized").trapped_delta)
                        {
                            best = Some(step);
                            best_net_delta = net_delta;
                        }
                    }
                }
            }
        }
    }

    best
}

fn find_center_move_setup_move<S: FaceletArray>(
    cube: &Cube<S>,
    table: &CenterCommutatorTable,
) -> Option<Move> {
    let score = center_score(cube);
    let mut best = None;
    let mut best_net_delta = 0;

    for max_setup_loss in [0isize, 1, 2, 4, 8] {
        for axis in [Axis::X, Axis::Y, Axis::Z] {
            for depth in 0..cube.side_len() {
                for angle in MoveAngle::ALL {
                    let mv = Move::new(axis, depth, angle);
                    let mut trial = cube.clone();
                    trial.apply_move_untracked_with_threads(mv, 1);
                    let setup_delta = center_score(&trial) as isize - score as isize;
                    if setup_delta < -max_setup_loss {
                        continue;
                    }

                    let Some(next) = best_improving_center_commutator_step(&trial, table) else {
                        continue;
                    };
                    let net_delta = setup_delta + next.score_delta;
                    if net_delta <= 0 {
                        continue;
                    }

                    if best.is_none() || net_delta > best_net_delta {
                        best = Some(mv);
                        best_net_delta = net_delta;
                    }
                }
            }
        }

        if best.is_some() {
            return best;
        }
    }

    best
}

fn center_score_delta_after_normalized_commutator<S: FaceletArray>(
    cube: &Cube<S>,
    destination: FaceId,
    updates: [FaceletUpdate; 3],
) -> isize {
    let mut delta = 0;

    for update in updates {
        let final_location = normalized_update_destination(cube.side_len(), destination, update);
        let old_value = cube
            .face(final_location.face)
            .get(final_location.row, final_location.col);
        let new_value = value_after_destination_turn(cube, destination, update.from);

        delta += center_position_score(final_location, new_value) as isize;
        delta -= center_position_score(final_location, old_value) as isize;
    }

    delta
}

fn center_trapped_delta_after_normalized_commutator<S: FaceletArray>(
    cube: &Cube<S>,
    destination: FaceId,
    updates: [FaceletUpdate; 3],
) -> isize {
    let mut delta = 0;

    for update in updates {
        let final_location = normalized_update_destination(cube.side_len(), destination, update);
        let old_value = cube
            .face(final_location.face)
            .get(final_location.row, final_location.col);
        let new_value = value_after_destination_turn(cube, destination, update.from);

        delta += center_position_trapped_score(final_location, new_value) as isize;
        delta -= center_position_trapped_score(final_location, old_value) as isize;
    }

    delta
}

fn normalized_update_destination(
    side_length: usize,
    destination: FaceId,
    update: FaceletUpdate,
) -> FaceletLocation {
    if update.to.face != destination {
        return update.to;
    }

    let (row, col) = rotate_face_location(
        side_length,
        update.to.row,
        update.to.col,
        baseline_destination_angle(destination).inverse(),
    );

    FaceletLocation {
        face: update.to.face,
        row,
        col,
    }
}

fn value_after_destination_turn<S: FaceletArray>(
    cube: &Cube<S>,
    destination: FaceId,
    location: FaceletLocation,
) -> Facelet {
    if location.face == destination {
        let side_length = cube.side_len();
        let (row, col) = source_coords_after_face_turn(
            side_length,
            location.row,
            location.col,
            baseline_destination_angle(destination),
        );
        cube.face(location.face).get(row, col)
    } else {
        cube.face(location.face).get(location.row, location.col)
    }
}

fn source_coords_after_face_turn(
    side_length: usize,
    row: usize,
    column: usize,
    angle: MoveAngle,
) -> (usize, usize) {
    match angle {
        MoveAngle::Positive => (side_length - 1 - column, row),
        MoveAngle::Double => (side_length - 1 - row, side_length - 1 - column),
        MoveAngle::Negative => (column, side_length - 1 - row),
    }
}

fn rotate_face_location(
    side_length: usize,
    row: usize,
    column: usize,
    angle: MoveAngle,
) -> (usize, usize) {
    match angle {
        MoveAngle::Positive => (column, side_length - 1 - row),
        MoveAngle::Double => (side_length - 1 - row, side_length - 1 - column),
        MoveAngle::Negative => (side_length - 1 - column, row),
    }
}

fn baseline_destination_angle(destination: FaceId) -> MoveAngle {
    match destination {
        FaceId::U | FaceId::R | FaceId::F => MoveAngle::Positive,
        FaceId::D | FaceId::L | FaceId::B => MoveAngle::Negative,
    }
}

fn center_position_score(location: FaceletLocation, value: Facelet) -> usize {
    usize::from(value == target_center_color(location.face))
}

fn center_position_trapped_score(location: FaceletLocation, value: Facelet) -> usize {
    let target_face = target_face_for_color(value);

    usize::from(
        location.face != target_face
            && location.face == opposite_face(target_face)
            && is_inner_location(location),
    )
}

fn is_inner_location(location: FaceletLocation) -> bool {
    location.row > 0 && location.col > 0
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

fn face_contains_center_color<S: FaceletArray>(
    cube: &Cube<S>,
    face: FaceId,
    color: Facelet,
) -> bool {
    for row in 1..cube.side_len().saturating_sub(1) {
        for column in 1..cube.side_len().saturating_sub(1) {
            if cube.face(face).get(row, column) == color {
                return true;
            }
        }
    }

    false
}

fn total_center_count(side_length: usize) -> usize {
    let centers_per_face = side_length.saturating_sub(2);
    centers_per_face * centers_per_face * FaceId::ALL.len()
}

fn target_center_color(face: FaceId) -> Facelet {
    Facelet::from_u8(face.index() as u8)
}

fn target_face_for_color(value: Facelet) -> FaceId {
    FaceId::ALL[value.as_u8() as usize]
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
            let destination = FaceId::ALL[(rng.next_u64() as usize) % FaceId::ALL.len()];
            let helper = FaceId::ALL[(rng.next_u64() as usize) % FaceId::ALL.len()];
            let angle = MoveAngle::ALL[(rng.next_u64() as usize) % MoveAngle::ALL.len()];
            let row = 1 + (rng.next_u64() as usize) % (cube.side_len() - 2);
            let column = 1 + (rng.next_u64() as usize) % (cube.side_len() - 2);

            if row == column {
                continue;
            }
            let Some(commutator) = table.get(destination, helper, angle) else {
                continue;
            };

            cube.apply_face_commutator_plan_untracked(commutator, &[row], &[column]);
            cube.apply_move_untracked_with_threads(
                face_outer_move(cube.side_len(), destination, MoveAngle::Positive).inverse(),
                1,
            );
            applied += 1;
        }
    }

    #[test]
    fn center_stage_default_transfer_order_is_explicit() {
        let stage = CenterReductionStage::western_default();

        assert_eq!(stage.transfers().len(), 15);
        assert_eq!(
            stage.transfers()[0],
            CenterTransferSpec::new(FaceId::F, FaceId::R, Facelet::Red)
        );
        assert_eq!(
            stage.transfers()[14],
            CenterTransferSpec::new(FaceId::U, FaceId::B, Facelet::Blue)
        );
    }
}
