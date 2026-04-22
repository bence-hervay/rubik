use crate::moves::{Axis, Move, MoveAngle};

use super::SolvePhase;

pub type MoveSequence = Vec<Move>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlgorithmReport {
    pub phase: SolvePhase,
    pub name: &'static str,
    pub step_count: usize,
    pub moves_before: usize,
    pub moves_after: usize,
}

impl AlgorithmReport {
    pub fn moves_added(&self) -> usize {
        self.moves_after - self.moves_before
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SolveOutcome {
    pub moves: MoveSequence,
    pub move_stats: MoveStats,
    pub reports: Vec<AlgorithmReport>,
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
