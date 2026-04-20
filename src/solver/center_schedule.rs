use crate::{face::FaceId, moves::MoveAngle};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CenterCoordExpr {
    Row,
    Column,
    ReverseRow,
    ReverseColumn,
}

impl CenterCoordExpr {
    pub fn eval(self, side_length: usize, row: usize, column: usize) -> usize {
        match self {
            Self::Row => row,
            Self::Column => column,
            Self::ReverseRow => side_length - 1 - row,
            Self::ReverseColumn => side_length - 1 - column,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CenterLocationExpr {
    pub face: FaceId,
    pub row: CenterCoordExpr,
    pub column: CenterCoordExpr,
}

impl CenterLocationExpr {
    pub fn eval(self, side_length: usize, row: usize, column: usize) -> CenterLocation {
        CenterLocation {
            face: self.face,
            row: self.row.eval(side_length, row, column),
            column: self.column.eval(side_length, row, column),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CenterLocation {
    pub face: FaceId,
    pub row: usize,
    pub column: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CenterScheduleStep {
    pub destination: FaceId,
    pub source: FaceId,
    pub helper: FaceId,
    pub angle: MoveAngle,
    pub source_location: CenterLocationExpr,
    pub destination_location: CenterLocationExpr,
}

#[path = "generated_center_schedule.rs"]
mod generated_center_schedule;

pub use generated_center_schedule::GENERATED_CENTER_SCHEDULE;
