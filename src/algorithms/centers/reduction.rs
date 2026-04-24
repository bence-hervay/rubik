use std::collections::{HashSet, VecDeque};

use crate::{
    algorithms::{
        AlgorithmContract, AlgorithmExecutionSupport, AlgorithmSideLengthSupport,
        AlgorithmStepSpec, SolveAlgorithm,
    },
    conventions::home_facelet_for_face,
    cube::{Cube, FaceCommutator},
    face::FaceId,
    facelet::Facelet,
    moves::{Axis, Move, MoveAngle},
    solver::{SolveContext, SolveError, SolvePhase, SolveResult, StageProgressSpec},
    storage::FaceletArray,
};

use super::precompute::{
    CenterLocationExpr, CenterScheduleStep, GENERATED_CENTER_SCHEDULE,
};

#[cfg(test)]
use super::precompute::CenterLocation;

#[cfg(test)]
use crate::solver::{MoveSequenceOperation, SolveOptions, SolverStage};

#[cfg(test)]
use crate::{
    algorithms::centers::{CenterCommutatorTable, CenterCoordExpr},
    conventions::face_outer_move,
};

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
pub struct CenterReductionAlgorithm {
    transfers: Vec<CenterTransferSpec>,
    steps: Vec<AlgorithmStepSpec>,
    schedule: &'static [CenterScheduleStep],
}

const CENTER_STAGE_STANDARD_PRECONDITIONS: &[&str] =
    &["none; the center stage may start from any cube state"];
const CENTER_STAGE_STANDARD_POSTCONDITIONS: &[&str] =
    &["all center facelets are solved when the stage returns success"];
const CENTER_ALGORITHM_CONTRACT: AlgorithmContract = AlgorithmContract::new(
    AlgorithmSideLengthSupport::all(),
    false,
    CENTER_STAGE_STANDARD_PRECONDITIONS,
    CENTER_STAGE_STANDARD_POSTCONDITIONS,
    AlgorithmExecutionSupport::StandardAndOptimized,
);

impl CenterReductionAlgorithm {
    pub fn new(transfers: Vec<CenterTransferSpec>) -> Self {
        let steps = vec![
            AlgorithmStepSpec::new(
                SolvePhase::Centers,
                "center scan tables",
                "scan source and destination centers into reusable row and column tables",
            ),
            AlgorithmStepSpec::new(
                SolvePhase::Centers,
                "center batch selection",
                "select disjoint row and column batches for each planned transfer",
            ),
            AlgorithmStepSpec::new(
                SolvePhase::Centers,
                "center commutator updates",
                "apply precomputed face commutator plans to selected batches",
            ),
        ];

        Self {
            transfers,
            steps,
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

impl<S: FaceletArray> SolveAlgorithm<S> for CenterReductionAlgorithm {
    fn phase(&self) -> SolvePhase {
        SolvePhase::Centers
    }

    fn name(&self) -> &'static str {
        "center reduction"
    }

    fn contract(&self) -> AlgorithmContract {
        CENTER_ALGORITHM_CONTRACT
    }

    fn steps(&self) -> &[AlgorithmStepSpec] {
        &self.steps
    }

    fn run(&mut self, cube: &mut Cube<S>, context: &mut SolveContext) -> SolveResult<()> {
        let stage_name = "center reduction";

        if centers_are_solved(cube) {
            return Ok(());
        }

        align_true_centers(cube, context, stage_name)?;

        if cube.side_len() < 4 || centers_are_solved(cube) {
            return Ok(());
        }

        let total_work = context
            .progress_enabled()
            .then(|| unsolved_scalable_center_facelet_count(cube))
            .unwrap_or(0);
        let progress_enabled = context.progress_enabled();

        context.with_stage_progress(
            StageProgressSpec::new(
                SolvePhase::Centers,
                stage_name,
                total_work,
                "facelets",
            ),
            |context| {
                solve_centers_with_transfers(
                    cube,
                    context,
                    &self.transfers,
                    self.schedule,
                    progress_enabled,
                    stage_name,
                )
            },
        )
    }
}

pub type CenterReductionStage = CenterReductionAlgorithm;

fn solve_centers_with_transfers<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    transfers: &[CenterTransferSpec],
    schedule: &[CenterScheduleStep],
    progress_enabled: bool,
    stage_name: &'static str,
) -> SolveResult<()> {
    let mut column_buffer = Vec::with_capacity(cube.side_len().saturating_sub(2));
    for transfer in transfers.iter().copied() {
        push_center_transfer(
            cube,
            context,
            transfer,
            schedule,
            &mut column_buffer,
            progress_enabled,
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
    progress_enabled: bool,
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
                let moved = apply_center_transfer_step(
                    cube,
                    context,
                    transfer,
                    step,
                    columns,
                    progress_enabled,
                );
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
    progress_enabled: bool,
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
            let source_piece_count = scan_center_transfer_row(
                cube,
                transfer,
                step,
                row,
                columns,
            );

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
            if progress_enabled {
                context.advance_stage_progress(columns.len());
            }
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
    context.apply_normalized_center_commutator_row(cube, commutator, row, columns);
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

fn scalable_center_facelet_count(side_length: usize) -> usize {
    let centers_per_face = side_length.saturating_sub(2);
    let per_face = centers_per_face
        .checked_mul(centers_per_face)
        .unwrap_or(usize::MAX);
    let mut total = per_face
        .checked_mul(FaceId::ALL.len())
        .unwrap_or(usize::MAX);

    if side_length >= 3 && side_length % 2 == 1 {
        total = total.saturating_sub(FaceId::ALL.len());
    }

    total
}

fn unsolved_scalable_center_facelet_count<S: FaceletArray>(cube: &Cube<S>) -> usize {
    scalable_center_facelet_count(cube.side_len())
        .saturating_sub(solved_scalable_center_facelet_count(cube))
}

fn solved_scalable_center_facelet_count<S: FaceletArray>(cube: &Cube<S>) -> usize {
    let side_length = cube.side_len();
    let middle = (side_length % 2 == 1).then_some(side_length / 2);
    let mut solved = 0;

    for face in FaceId::ALL {
        let target = home_facelet_for_face(face).as_u8();
        let storage = cube.face(face).matrix().storage();

        for row in 1..side_length.saturating_sub(1) {
            let mut index = row * side_length + 1;
            for column in 1..side_length.saturating_sub(1) {
                if middle == Some(row) && middle == Some(column) {
                    index += 1;
                    continue;
                }

                solved += usize::from(unsafe { storage.get_unchecked_raw(index) } == target);
                index += 1;
            }
        }
    }

    solved
}

#[cfg(test)]
fn center_progress_index(side_length: usize, location: CenterLocation) -> Option<usize> {
    if location.row == 0
        || location.column == 0
        || location.row + 1 == side_length
        || location.column + 1 == side_length
    {
        return None;
    }

    let centers_per_face = side_length.saturating_sub(2);
    if centers_per_face == 0 {
        return None;
    }
    let per_face = centers_per_face.checked_mul(centers_per_face)?;

    let row_index = location.row - 1;
    let column_index = location.column - 1;
    let linear = row_index
        .checked_mul(centers_per_face)?
        .checked_add(column_index)?;

    if side_length >= 3 && side_length % 2 == 1 {
        let middle = side_length / 2;
        if location.row == middle && location.column == middle {
            return None;
        }

        let local_middle = middle - 1;
        let true_center_linear = local_middle * centers_per_face + local_middle;
        let local = if linear < true_center_linear {
            linear
        } else {
            linear - 1
        };
        return location
            .face
            .index()
            .checked_mul(per_face.checked_sub(1)?)?
            .checked_add(local);
    }

    location
        .face
        .index()
        .checked_mul(per_face)?
        .checked_add(linear)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{conventions::opposite_face, Byte, RandomSource, XorShift64};

    #[test]
    fn scalable_center_progress_total_excludes_true_centers() {
        assert_eq!(scalable_center_facelet_count(2), 0);
        assert_eq!(scalable_center_facelet_count(4), 24);
        assert_eq!(scalable_center_facelet_count(5), 48);
        assert_eq!(scalable_center_facelet_count(7), 144);
    }

    #[test]
    fn center_progress_index_skips_outer_ring_and_true_centers() {
        let front_corner = center_progress_index(
            5,
            CenterLocation {
                face: FaceId::F,
                row: 1,
                column: 1,
            },
        );
        let right_corner = center_progress_index(
            5,
            CenterLocation {
                face: FaceId::R,
                row: 3,
                column: 3,
            },
        );

        assert_eq!(
            center_progress_index(
                5,
                CenterLocation {
                    face: FaceId::F,
                    row: 0,
                    column: 2,
                },
            ),
            None,
        );
        assert_eq!(
            center_progress_index(
                5,
                CenterLocation {
                    face: FaceId::F,
                    row: 2,
                    column: 2,
                },
            ),
            None,
        );
        assert!(front_corner.is_some());
        assert!(right_corner.is_some());
        assert_ne!(front_corner, right_corner);
    }

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

        let mut unrecorded_cube = Cube::<Byte>::new_solved(side_length);
        let mut unrecorded_context = SolveContext::new(SolveOptions {
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

        let mut recorded_cube = Cube::<Byte>::new_solved(side_length);
        let mut recorded_context = SolveContext::new(SolveOptions { record_moves: true });
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
                let mut context = SolveContext::new(SolveOptions { record_moves: true });

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
        expected.apply_moves_untracked(moves);

        let mut actual = patterned_cube(side_length);
        let mut context = SolveContext::new(SolveOptions { record_moves: true });
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
                let mut cube = Cube::<Byte>::new_solved(side_length);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                cube.scramble_random_moves(&mut rng, 120);
                let initial = cube.clone();
                let history_before = cube.history().len();
                let history_before_moves = initial.history().as_slice().to_vec();

                let mut stage = CenterReductionStage::western_default();
                let mut context = SolveContext::new(SolveOptions { record_moves: true });

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
                replay.apply_moves_untracked(context.moves().iter().copied());

                assert_cubes_match(&cube, &replay);
                assert!(centers_are_solved(&cube));
                assert_eq!(cube.history().len(), history_before);
                assert_eq!(cube.history().as_slice(), history_before_moves.as_slice());
            }
        }
    }

    #[test]
    fn center_stage_solves_scrambled_centers_for_various_sizes() {
        for side_length in 4..=8 {
            for seed in [0xC011_EC7u64, 0xCE17_E25u64] {
                let mut cube = Cube::<Byte>::new_solved(side_length);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                scramble_centers_with_normalized_commutators(&mut cube, &mut rng, 1);

                let mut stage = CenterReductionStage::western_default();
                let mut context = SolveContext::new(SolveOptions { record_moves: true });

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
                let mut cube = Cube::<Byte>::new_solved(side_length);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                let moves = (0..120)
                    .map(|_| cube.random_move(&mut rng))
                    .collect::<Vec<_>>();
                cube.apply_moves_untracked(moves);

                let mut stage = CenterReductionStage::western_default();
                let mut context = SolveContext::new(SolveOptions {
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
        let mut cube = Cube::<Byte>::new_solved(side_length);
        scramble_dense_center_route(&mut cube);

        let mut stage = CenterReductionStage::western_default();
        let mut context = SolveContext::new(SolveOptions {
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
                let mut cube = Cube::<Byte>::new_solved(side_length);
                cube.apply_move_untracked(mv);

                assert_eq!(
                    center_orientation_after_move(start, mv),
                    center_orientation(&cube, middle),
                    "center alignment model differs for {mv}",
                );
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
        let cube = Cube::<Byte>::new_solved(9);
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
        let mut cube = Cube::<Byte>::new_solved(side_length);

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
