use crate::{
    algorithms::{
        CenterReductionAlgorithm, CornerReductionAlgorithm, EdgePairingAlgorithm, SolveAlgorithm,
    },
    cube::Cube,
    storage::FaceletArray,
};

use super::{AlgorithmReport, SolveContext, SolveError, SolveOptions, SolveOutcome, SolveResult};

pub trait Solver<S: FaceletArray> {
    fn solve(&mut self, cube: &mut Cube<S>) -> SolveResult<SolveOutcome>;
}

pub struct ReductionSolver<S: FaceletArray> {
    options: SolveOptions,
    algorithms: Vec<Box<dyn SolveAlgorithm<S>>>,
}

impl<S: FaceletArray + 'static> ReductionSolver<S> {
    pub fn new(options: SolveOptions) -> Self {
        Self {
            options,
            algorithms: Vec::new(),
        }
    }

    pub fn large_cube_default() -> Self {
        Self::new(SolveOptions::default())
            .with_algorithm(CenterReductionAlgorithm::western_default())
            .with_algorithm(CornerReductionAlgorithm::default())
            .with_algorithm(EdgePairingAlgorithm::default())
    }

    pub fn with_algorithm<T>(mut self, algorithm: T) -> Self
    where
        T: SolveAlgorithm<S> + 'static,
    {
        self.algorithms.push(Box::new(algorithm));
        self
    }

    pub fn with_stage<T>(self, stage: T) -> Self
    where
        T: SolveAlgorithm<S> + 'static,
    {
        self.with_algorithm(stage)
    }

    pub fn algorithm_count(&self) -> usize {
        self.algorithms.len()
    }

    pub fn stage_count(&self) -> usize {
        self.algorithm_count()
    }

    pub fn algorithm_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.algorithms.iter().map(|algorithm| algorithm.name())
    }

    pub fn stage_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.algorithm_names()
    }
}

impl<S: FaceletArray + 'static> Solver<S> for ReductionSolver<S> {
    fn solve(&mut self, cube: &mut Cube<S>) -> SolveResult<SolveOutcome> {
        let mut context = SolveContext::new(self.options);
        let mut reports = Vec::with_capacity(self.algorithms.len());
        let execution_mode = context.execution_mode();

        for algorithm in &mut self.algorithms {
            if !algorithm.execution_mode_support().supports(execution_mode) {
                return Err(SolveError::StageFailed {
                    stage: algorithm.name(),
                    reason: "algorithm does not support the requested execution mode",
                });
            }

            let moves_before = context.move_stats().total;
            algorithm.run(cube, &mut context)?;
            let moves_after = context.move_stats().total;

            reports.push(AlgorithmReport {
                phase: algorithm.phase(),
                name: algorithm.name(),
                step_count: algorithm.steps().len(),
                moves_before,
                moves_after,
            });
        }

        Ok(context.into_outcome(reports))
    }
}
