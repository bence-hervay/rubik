use crate::{
    face::{Face, FaceId},
    facelet::Facelet,
    moves::history::MoveHistory,
    storage::FaceletArray,
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

#[derive(Clone, Debug)]
pub struct Cube<S: FaceletArray> {
    pub(crate) n: usize,
    pub(crate) faces: [Face<S>; 6],
    pub(crate) reachability: CubeReachability,
    pub(crate) history: MoveHistory,
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
