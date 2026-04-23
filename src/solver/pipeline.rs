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
    stages: Vec<Box<dyn SolveAlgorithm<S>>>,
}

impl<S: FaceletArray + 'static> ReductionSolver<S> {
    pub fn new(options: SolveOptions) -> Self {
        Self {
            options,
            stages: Vec::new(),
        }
    }

    pub fn large_cube_default() -> Self {
        Self::new(SolveOptions::default())
            .with_stage(CenterReductionAlgorithm::western_default())
            .with_stage(CornerReductionAlgorithm::default())
            .with_stage(EdgePairingAlgorithm::default())
    }

    pub fn with_algorithm<T>(self, algorithm: T) -> Self
    where
        T: SolveAlgorithm<S> + 'static,
    {
        self.with_stage(algorithm)
    }

    pub fn with_stage<T>(mut self, stage: T) -> Self
    where
        T: SolveAlgorithm<S> + 'static,
    {
        self.stages.push(Box::new(stage));
        self
    }

    pub fn algorithm_count(&self) -> usize {
        self.stage_count()
    }

    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    pub fn algorithm_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.stage_names()
    }

    pub fn stage_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.stages.iter().map(|stage| stage.name())
    }
}

impl<S: FaceletArray + 'static> Solver<S> for ReductionSolver<S> {
    fn solve(&mut self, cube: &mut Cube<S>) -> SolveResult<SolveOutcome> {
        let mut context = SolveContext::new(self.options);
        let mut reports = Vec::with_capacity(self.stages.len());
        let execution_mode = context.execution_mode();

        for stage in &mut self.stages {
            if !stage.execution_mode_support().supports(execution_mode) {
                return Err(SolveError::StageFailed {
                    stage: stage.name(),
                    reason: "stage does not support the requested execution mode",
                });
            }

            let moves_before = context.move_stats().total;
            stage.run(cube, &mut context)?;
            let moves_after = context.move_stats().total;

            reports.push(AlgorithmReport {
                phase: stage.phase(),
                name: stage.name(),
                step_count: stage.steps().len(),
                moves_before,
                moves_after,
            });
        }

        Ok(context.into_outcome(reports))
    }
}
