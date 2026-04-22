pub mod algorithms;
pub mod layout;
pub mod model;
pub mod moves;
pub mod simulation;
pub mod solver;
pub mod storage;
pub mod util;

pub use algorithms::operation::{
    Algorithm, MoveSequenceAlgorithm, MoveSequenceOperation, OptimizedAlgorithm,
};
pub use algorithms::{
    AlgorithmContract, AlgorithmExecutionSupport, AlgorithmSideLengthSupport, AlgorithmStepSpec,
    CenterReductionAlgorithm, CenterTransferSpec, CornerReductionAlgorithm, CornerSlot,
    EdgePairingAlgorithm, EdgeSlot, SolveAlgorithm, ThreeByThreeAlgorithm,
    Operation, OptimizedOperation,
};
pub(crate) use layout::geometry;
pub use layout::matrix;
pub use layout::strip as line;
pub use layout::Matrix;
pub use model::cube;
pub use model::face;
pub use model::facelet;
pub use model::{
    ColorScheme, Cube, CubeReachability, Face, FaceAngle, FaceId, Facelet,
    DEFAULT_SCRAMBLE_ROUNDS,
};
pub use moves::history;
pub use moves::{Axis, Move, MoveAngle, MoveHistory};
pub use simulation::conventions;
pub use simulation::derived::{CornerCubieLocation, EdgeCubieLocation, FaceletLocation, FaceletUpdate};
pub use algorithms::centers::{
    CenterCommutatorTable, CenterCoordExpr, CenterLocation, CenterLocationExpr,
    CenterScheduleStep, FaceCommutator, FaceCommutatorLayers, FaceCommutatorMode,
    FaceCommutatorPlan, FaceCommutatorValidationError, LayerSetKind,
    LayerSetValidationError, GENERATED_CENTER_SCHEDULE,
};
pub use algorithms::corners::CornerReductionStage;
pub use algorithms::edges::{
    EdgePairingStage, EdgeThreeCycle, EdgeThreeCycleDirection, EdgeThreeCycleKind,
    EdgeThreeCyclePlan, EdgeThreeCycleValidationError,
};
pub use util::random;
pub use util::threading::default_thread_count;
pub use util::{RandomSource, XorShift64};
pub use solver::{
    AlgorithmReport, CenterReductionStage, ExecutionMode, MoveSequence, MoveStats,
    ReductionSolver, SolveContext, SolveError, SolveOptions, SolveOutcome, SolvePhase,
    SolveResult, Solver, SolverStage, StageContract, StageExecutionSupport, StageReport,
    StageSideLengthSupport, SubStageSpec, ThreeByThreeStage,
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
#[deprecated(note = "use rubik::algorithms::centers::CenterCommutatorTable")]
pub use algorithms::centers::CenterCommutatorTable as DeprecatedCenterCommutatorTable;
#[deprecated(note = "use rubik::algorithms::centers::CenterCoordExpr")]
pub use algorithms::centers::CenterCoordExpr as DeprecatedCenterCoordExpr;
#[deprecated(note = "use rubik::algorithms::centers::CenterLocation")]
pub use algorithms::centers::CenterLocation as DeprecatedCenterLocation;
#[deprecated(note = "use rubik::algorithms::centers::CenterLocationExpr")]
pub use algorithms::centers::CenterLocationExpr as DeprecatedCenterLocationExpr;
#[deprecated(note = "use rubik::algorithms::centers::CenterScheduleStep")]
pub use algorithms::centers::CenterScheduleStep as DeprecatedCenterScheduleStep;
#[deprecated(note = "use rubik::algorithms::centers::GENERATED_CENTER_SCHEDULE")]
pub use algorithms::centers::GENERATED_CENTER_SCHEDULE as DeprecatedGeneratedCenterSchedule;
