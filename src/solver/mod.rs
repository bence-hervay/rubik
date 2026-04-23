mod context;
mod error;
mod execution_mode;
mod options;
mod phase;
mod pipeline;
mod report;

pub use crate::algorithms::{
    AlgorithmContract, AlgorithmExecutionSupport, AlgorithmSideLengthSupport, AlgorithmStepSpec,
    CenterReductionAlgorithm, CenterReductionStage, CenterTransferSpec, CornerReductionAlgorithm,
    CornerReductionStage, CornerSearchAlgorithm, CornerSearchStage, CornerSlot,
    CornerTwoCycleAlgorithm, CornerTwoCycleStage, EdgePairingAlgorithm, EdgePairingStage, EdgeSlot,
    MoveSequenceOperation, SolveAlgorithm,
};

pub use context::SolveContext;
pub use error::{SolveError, SolveResult};
pub use execution_mode::ExecutionMode;
pub use options::SolveOptions;
pub use phase::SolvePhase;
pub use pipeline::{ReductionSolver, Solver};
pub use report::{AlgorithmReport, MoveSequence, MoveStats, SolveOutcome};

pub use crate::algorithms::{
    AlgorithmContract as StageContract, AlgorithmExecutionSupport as StageExecutionSupport,
    AlgorithmSideLengthSupport as StageSideLengthSupport, AlgorithmStepSpec as SubStageSpec,
    SolveAlgorithm as SolverStage,
};
pub use report::AlgorithmReport as StageReport;

#[deprecated(note = "use crate::algorithms::centers::CenterCommutatorTable")]
pub use crate::algorithms::centers::CenterCommutatorTable;
#[deprecated(note = "use crate::algorithms::centers::CenterCoordExpr")]
pub use crate::algorithms::centers::CenterCoordExpr;
#[deprecated(note = "use crate::algorithms::centers::CenterLocation")]
pub use crate::algorithms::centers::CenterLocation;
#[deprecated(note = "use crate::algorithms::centers::CenterLocationExpr")]
pub use crate::algorithms::centers::CenterLocationExpr;
#[deprecated(note = "use crate::algorithms::centers::CenterScheduleStep")]
pub use crate::algorithms::centers::CenterScheduleStep;
#[deprecated(note = "use crate::algorithms::centers::GENERATED_CENTER_SCHEDULE")]
pub use crate::algorithms::centers::GENERATED_CENTER_SCHEDULE;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Byte, FaceletArray};

    #[test]
    fn default_reduction_solver_has_center_corner_and_edge_stages() {
        let solver = ReductionSolver::<Byte>::large_cube_default();
        let names = solver.stage_names().collect::<Vec<_>>();

        assert_eq!(solver.stage_count(), 3);
        assert_eq!(
            names,
            ["center reduction", "corner reduction", "edge pairing"]
        );
    }

    #[test]
    fn solve_options_named_modes_round_trip_through_recording_flag() {
        let standard = SolveOptions::standard();
        assert!(standard.record_moves);
        assert_eq!(standard.execution_mode(), ExecutionMode::Standard);

        let optimized = SolveOptions::optimized();
        assert!(!optimized.record_moves);
        assert_eq!(optimized.execution_mode(), ExecutionMode::Optimized);

        let switched = SolveOptions::standard().with_execution_mode(ExecutionMode::Optimized);
        assert!(!switched.record_moves);
        assert_eq!(switched.execution_mode(), ExecutionMode::Optimized);
    }

    #[test]
    fn default_algorithms_explicitly_declare_execution_mode_support() {
        assert_eq!(
            <CenterReductionAlgorithm as SolveAlgorithm<Byte>>::execution_mode_support(
                &CenterReductionAlgorithm::western_default()
            ),
            AlgorithmExecutionSupport::StandardAndOptimized
        );
        assert_eq!(
            <CornerReductionAlgorithm as SolveAlgorithm<Byte>>::execution_mode_support(
                &CornerReductionAlgorithm::default()
            ),
            AlgorithmExecutionSupport::StandardAndOptimized
        );
        assert_eq!(
            <EdgePairingAlgorithm as SolveAlgorithm<Byte>>::execution_mode_support(
                &EdgePairingAlgorithm::default()
            ),
            AlgorithmExecutionSupport::StandardAndOptimized
        );
    }

    #[test]
    fn algorithm_side_length_support_respects_range_and_parity() {
        let support = AlgorithmSideLengthSupport::new(2, Some(6), true, false);

        assert!(!support.supports(1));
        assert!(!support.supports(2));
        assert!(support.supports(3));
        assert!(!support.supports(4));
        assert!(support.supports(5));
        assert!(!support.supports(6));
        assert!(!support.supports(7));
    }

    #[test]
    fn default_algorithm_contracts_are_explicit_and_nonempty() {
        let center = <CenterReductionAlgorithm as SolveAlgorithm<Byte>>::contract(
            &CenterReductionAlgorithm::western_default(),
        );
        assert!(center.side_lengths.supports(1));
        assert!(!center.requires_previous_stages_solved);
        assert!(!center.standard_preconditions.is_empty());
        assert!(!center.standard_postconditions.is_empty());

        let corners = <CornerReductionAlgorithm as SolveAlgorithm<Byte>>::contract(
            &CornerReductionAlgorithm::default(),
        );
        assert!(corners.side_lengths.supports(2));
        assert!(!corners.requires_previous_stages_solved);
        assert!(!corners.standard_preconditions.is_empty());
        assert!(!corners.standard_postconditions.is_empty());

        let edges = <EdgePairingAlgorithm as SolveAlgorithm<Byte>>::contract(
            &EdgePairingAlgorithm::default(),
        );
        assert!(edges.side_lengths.supports(3));
        assert!(!edges.requires_previous_stages_solved);
        assert!(!edges.standard_preconditions.is_empty());
        assert!(!edges.standard_postconditions.is_empty());
    }

    #[test]
    fn solver_rejects_algorithm_when_requested_mode_is_not_supported() {
        #[derive(Default)]
        struct StandardOnlyAlgorithm;

        impl<S: FaceletArray> SolveAlgorithm<S> for StandardOnlyAlgorithm {
            fn phase(&self) -> SolvePhase {
                SolvePhase::Edges
            }

            fn name(&self) -> &'static str {
                "standard only test stage"
            }

            fn contract(&self) -> AlgorithmContract {
                AlgorithmContract::new(
                    AlgorithmSideLengthSupport::all(),
                    false,
                    &["standard execution only"],
                    &["no-op"],
                    AlgorithmExecutionSupport::StandardOnly,
                )
            }

            fn steps(&self) -> &[AlgorithmStepSpec] {
                &[]
            }

            fn run(
                &mut self,
                _cube: &mut crate::Cube<S>,
                _context: &mut SolveContext,
            ) -> SolveResult<()> {
                Ok(())
            }
        }

        let mut solver = ReductionSolver::<Byte>::new(SolveOptions::optimized())
            .with_algorithm(StandardOnlyAlgorithm);
        let mut cube = crate::Cube::<Byte>::new_solved(3);

        assert_eq!(
            solver.solve(&mut cube),
            Err(SolveError::StageFailed {
                stage: "standard only test stage",
                reason: "stage does not support the requested execution mode",
            })
        );
    }

    #[test]
    fn default_pipeline_runs_on_a_solved_cube_without_adding_moves() {
        let mut solver = ReductionSolver::<Byte>::large_cube_default();
        let mut cube = crate::Cube::<Byte>::new_solved(5);

        let outcome = solver
            .solve(&mut cube)
            .expect("default algorithms should run on a solved cube");

        assert!(outcome.moves.is_empty());
        assert_eq!(outcome.move_stats, MoveStats::default());
        assert_eq!(outcome.reports.len(), 3);
        assert_eq!(outcome.reports[0].phase, SolvePhase::Centers);
        assert_eq!(outcome.reports[1].phase, SolvePhase::Corners);
        assert_eq!(outcome.reports[2].phase, SolvePhase::Edges);
    }
}
