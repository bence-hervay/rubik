use crate::face::FaceId;

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
