use core::ops::Range;

use crate::{face::FaceId, moves::MoveAngle};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CenterQuadrant {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl CenterQuadrant {
    pub const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::TopRight => "top_right",
            Self::BottomLeft => "bottom_left",
            Self::BottomRight => "bottom_right",
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::TopLeft => 0,
            Self::TopRight => 1,
            Self::BottomLeft => 2,
            Self::BottomRight => 3,
        }
    }

    pub fn rows(self, side_length: usize) -> Range<usize> {
        assert!(
            side_length >= 4,
            "center quadrant requires side length >= 4"
        );
        let middle_low = side_length / 2;
        let middle_high = side_length.div_ceil(2);

        match self {
            Self::TopLeft | Self::TopRight => 1..middle_low,
            Self::BottomLeft | Self::BottomRight => middle_high..side_length - 1,
        }
    }

    pub fn columns(self, side_length: usize) -> Range<usize> {
        assert!(
            side_length >= 4,
            "center quadrant requires side length >= 4"
        );
        let middle_low = side_length / 2;
        let middle_high = side_length.div_ceil(2);

        match self {
            Self::TopLeft | Self::BottomLeft => 1..middle_low,
            Self::TopRight | Self::BottomRight => middle_high..side_length - 1,
        }
    }
}

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
    pub row_quadrant: CenterQuadrant,
    pub column_quadrant: CenterQuadrant,
    pub source_location: CenterLocationExpr,
    pub destination_location: CenterLocationExpr,
}

#[path = "generated_center_schedule.rs"]
mod generated_center_schedule;

pub use generated_center_schedule::GENERATED_CENTER_SCHEDULE;
