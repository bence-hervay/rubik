pub mod reduction;
pub mod search;
pub mod two_cycle;

pub use reduction::{CornerSearchAlgorithm, CornerSearchStage, CornerSlot};
pub use two_cycle::{CornerTwoCycleAlgorithm, CornerTwoCycleStage};

pub type CornerReductionAlgorithm = CornerTwoCycleAlgorithm;
pub type CornerReductionStage = CornerTwoCycleStage;
