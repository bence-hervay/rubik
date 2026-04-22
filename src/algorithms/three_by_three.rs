use crate::{
    cube::Cube,
    solver::{SolveContext, SolvePhase, SolveResult},
    storage::FaceletArray,
};

use super::{
    AlgorithmContract, AlgorithmExecutionSupport, AlgorithmSideLengthSupport, AlgorithmStepSpec,
    SolveAlgorithm,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreeByThreeAlgorithm {
    steps: [AlgorithmStepSpec; 2],
}

const THREE_BY_THREE_ALGORITHM_STANDARD_PRECONDITIONS: &[&str] =
    &["the cube should already be reduced to a 3x3-equivalent state"];
const THREE_BY_THREE_ALGORITHM_STANDARD_POSTCONDITIONS: &[&str] =
    &["currently a placeholder adapter algorithm; no additional guarantees are added yet"];
const THREE_BY_THREE_ALGORITHM_CONTRACT: AlgorithmContract = AlgorithmContract::new(
    AlgorithmSideLengthSupport::all(),
    true,
    THREE_BY_THREE_ALGORITHM_STANDARD_PRECONDITIONS,
    THREE_BY_THREE_ALGORITHM_STANDARD_POSTCONDITIONS,
    AlgorithmExecutionSupport::StandardAndOptimized,
);

impl Default for ThreeByThreeAlgorithm {
    fn default() -> Self {
        Self {
            steps: [
                AlgorithmStepSpec::new(
                    SolvePhase::ThreeByThree,
                    "reduced-state extraction",
                    "project centers, paired edges, and corners into a 3x3 representation",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::ThreeByThree,
                    "3x3 solve adapter",
                    "delegate the reduced state to a future 3x3 solver implementation",
                ),
            ],
        }
    }
}

impl<S: FaceletArray> SolveAlgorithm<S> for ThreeByThreeAlgorithm {
    fn phase(&self) -> SolvePhase {
        SolvePhase::ThreeByThree
    }

    fn name(&self) -> &'static str {
        "3x3 finish"
    }

    fn contract(&self) -> AlgorithmContract {
        THREE_BY_THREE_ALGORITHM_CONTRACT
    }

    fn steps(&self) -> &[AlgorithmStepSpec] {
        &self.steps
    }

    fn run(&mut self, _cube: &mut Cube<S>, _context: &mut SolveContext) -> SolveResult<()> {
        Ok(())
    }
}

pub type ThreeByThreeStage = ThreeByThreeAlgorithm;
