pub(crate) mod geometry;
pub mod matrix;
pub mod strip;

pub use matrix::Matrix;
pub use strip::{LineBuffer, LineKind, MoveScratch, StripSpec};
