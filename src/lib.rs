pub mod algorithm;
pub mod puzzle;
pub mod runtime;
pub mod solver;
pub mod storage;
pub mod support;

pub use algorithm::{Algorithm, MoveSequenceAlgorithm, OptimizedAlgorithm};
pub(crate) use layout::geometry;
pub use layout::matrix;
pub use layout::strip as line;
pub use layout::Matrix;
pub use model::cube;
pub use model::face;
pub use model::facelet;
pub use model::{
    ColorScheme, CornerCubieLocation, Cube, CubeReachability, EdgeCubieLocation, EdgeThreeCycle,
    EdgeThreeCycleDirection, EdgeThreeCycleKind, EdgeThreeCyclePlan, EdgeThreeCycleValidationError,
    Face, FaceAngle, FaceCommutator, FaceCommutatorLayers, FaceCommutatorMode, FaceCommutatorPlan,
    FaceCommutatorValidationError, FaceId, Facelet, FaceletLocation, FaceletUpdate, LayerSetKind,
    LayerSetValidationError, DEFAULT_SCRAMBLE_ROUNDS,
};
pub use moves::history;
pub use moves::{Axis, Move, MoveAngle, MoveHistory};
pub use puzzle::{conventions, layout, model, moves};
pub use runtime::random;
pub(crate) use runtime::threading;
pub use runtime::threading::default_thread_count;
pub use runtime::{RandomSource, XorShift64};
pub use solver::{
    CenterReductionStage, CenterTransferSpec, CornerReductionStage, CornerSlot, EdgePairingStage,
    EdgeSlot, ExecutionMode, MoveSequence, MoveStats, ReductionSolver, SolveContext, SolveError,
    SolveOptions, SolveOutcome, SolvePhase, SolveResult, Solver, SolverStage, StageContract,
    StageExecutionSupport, StageReport, StageSideLengthSupport, SubStageSpec, ThreeByThreeStage,
};
pub use storage::{Byte, Byte3, FaceletArray, Nibble, ThreeBit};

#[deprecated(note = "use rubik::layout::LineBuffer")]
pub use layout::LineBuffer;
#[deprecated(note = "use rubik::layout::LineKind")]
pub use layout::LineKind;
#[deprecated(note = "use rubik::layout::MoveScratch")]
pub use layout::MoveScratch;
#[deprecated(note = "use rubik::layout::StripSpec")]
pub use layout::StripSpec;
#[deprecated(note = "use rubik::support::centers::CenterCommutatorTable")]
pub use support::centers::CenterCommutatorTable;
#[deprecated(note = "use rubik::support::centers::CenterCoordExpr")]
pub use support::centers::CenterCoordExpr;
#[deprecated(note = "use rubik::support::centers::CenterLocation")]
pub use support::centers::CenterLocation;
#[deprecated(note = "use rubik::support::centers::CenterLocationExpr")]
pub use support::centers::CenterLocationExpr;
#[deprecated(note = "use rubik::support::centers::CenterScheduleStep")]
pub use support::centers::CenterScheduleStep;
#[deprecated(note = "use rubik::support::centers::GENERATED_CENTER_SCHEDULE")]
pub use support::centers::GENERATED_CENTER_SCHEDULE;
