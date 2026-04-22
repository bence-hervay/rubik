pub(crate) use crate::conventions::{face_layer_move, opposite_face};

use crate::{
    face::{Face, FaceId},
    facelet::Facelet,
    history::MoveHistory,
    moves::{Move, MoveAngle},
    storage::FaceletArray,
};

use self::commutator::CenterCommutatorTemplate;

mod commutator;
mod edge_cycles;
mod pieces;
mod render;
mod state;

#[cfg(test)]
mod tests;

pub(crate) use edge_cycles::edge_three_cycle_plan_from_updates;
#[allow(unused_imports)]
pub(crate) use pieces::{
    corner_cubie_for_facelet_location, edge_cubie_for_facelet_location, edge_cubie_orbit_index,
    trace_corner_cubie_through_move, trace_edge_cubie_through_move,
    trace_facelet_location_through_move, trace_facelet_location_through_moves,
};

pub const DEFAULT_SCRAMBLE_ROUNDS: usize = 6;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ColorScheme {
    pub u: Facelet,
    pub d: Facelet,
    pub r: Facelet,
    pub l: Facelet,
    pub f: Facelet,
    pub b: Facelet,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            u: Facelet::White,
            d: Facelet::Yellow,
            r: Facelet::Red,
            l: Facelet::Orange,
            f: Facelet::Green,
            b: Facelet::Blue,
        }
    }
}

impl ColorScheme {
    pub const fn color_of(self, face: FaceId) -> Facelet {
        match face {
            FaceId::U => self.u,
            FaceId::D => self.d,
            FaceId::R => self.r,
            FaceId::L => self.l,
            FaceId::F => self.f,
            FaceId::B => self.b,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FaceCommutator {
    destination: FaceId,
    helper: FaceId,
    slice_angle: MoveAngle,
    expanded_template: CenterCommutatorTemplate,
    normalized_template: CenterCommutatorTemplate,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FaceCommutatorMode {
    Expanded,
    Normalized,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FaceCommutatorLayers<'a> {
    rows: &'a [usize],
    columns: &'a [usize],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FaceCommutatorPlan<'a> {
    side_length: usize,
    commutator: FaceCommutator,
    mode: FaceCommutatorMode,
    layers: FaceCommutatorLayers<'a>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LayerSetKind {
    Rows,
    Columns,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum LayerSetValidationError {
    MustContainOnlyInnerLayers { set: LayerSetKind },
    MustBeStrictlyIncreasing { set: LayerSetKind },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FaceCommutatorValidationError {
    CubeTooSmall,
    DestinationAndHelperMustDiffer,
    DestinationAndHelperMustBePerpendicular,
    InvalidLayerSet(LayerSetValidationError),
    RowAndColumnSetsMustBeDisjoint,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum EdgeThreeCycleValidationError {
    WingCycleRequiresSideLengthAtLeastFour,
    RowMustBeInnerLayer,
    WingRowCannotBeOddMiddleLayer,
    MiddleCycleRequiresOddSideLengthAtLeastThree,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FaceletLocation {
    pub face: FaceId,
    pub row: usize,
    pub col: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FaceletUpdate {
    pub from: FaceletLocation,
    pub to: FaceletLocation,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct EdgeCubieLocation {
    /// The two stickers belonging to the same physical edge cubie, stored in a
    /// stable canonical order.
    pub stickers: [FaceletLocation; 2],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct CornerCubieLocation {
    /// The three stickers belonging to the same physical corner cubie, stored
    /// in a stable canonical order.
    pub stickers: [FaceletLocation; 3],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EdgeThreeCycle {
    kind: EdgeThreeCycleKind,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum EdgeThreeCycleKind {
    FrontRightWing { row: usize },
    FrontRightMiddle { direction: EdgeThreeCycleDirection },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum EdgeThreeCycleDirection {
    Positive,
    Negative,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EdgeThreeCyclePlan {
    side_length: usize,
    cycle: Option<EdgeThreeCycle>,
    moves: Vec<Move>,
    cubies: [EdgeCubieLocation; 3],
    updates: [FaceletUpdate; 6],
}

#[derive(Clone, Debug)]
pub struct Cube<S: FaceletArray> {
    n: usize,
    faces: [Face<S>; 6],
    reachability: CubeReachability,
    history: MoveHistory,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum CubeReachability {
    Reachable,
    Unverified,
}

impl CubeReachability {
    pub const fn is_reachable(self) -> bool {
        matches!(self, Self::Reachable)
    }
}
