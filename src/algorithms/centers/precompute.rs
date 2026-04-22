use crate::{conventions::opposite_face, cube::FaceCommutator, face::FaceId, moves::MoveAngle};

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

#[path = "generated_center_schedule.rs"]
mod generated_center_schedule;

pub use generated_center_schedule::GENERATED_CENTER_SCHEDULE;
