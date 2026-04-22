use crate::{
    cube::Cube,
    solver::{ExecutionMode, SolveContext, SolvePhase, SolveResult},
    storage::FaceletArray,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum AlgorithmExecutionSupport {
    StandardOnly,
    StandardAndOptimized,
}

impl AlgorithmExecutionSupport {
    pub const fn supports(self, mode: ExecutionMode) -> bool {
        match (self, mode) {
            (Self::StandardOnly, ExecutionMode::Standard) => true,
            (Self::StandardOnly, ExecutionMode::Optimized) => false,
            (Self::StandardAndOptimized, _) => true,
        }
    }

    pub const fn supports_optimized(self) -> bool {
        self.supports(ExecutionMode::Optimized)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct AlgorithmSideLengthSupport {
    pub minimum: usize,
    pub maximum: Option<usize>,
    pub supports_odd: bool,
    pub supports_even: bool,
}

impl AlgorithmSideLengthSupport {
    pub const fn new(
        minimum: usize,
        maximum: Option<usize>,
        supports_odd: bool,
        supports_even: bool,
    ) -> Self {
        Self {
            minimum,
            maximum,
            supports_odd,
            supports_even,
        }
    }

    pub const fn all() -> Self {
        Self::new(1, None, true, true)
    }

    pub const fn supports(self, side_length: usize) -> bool {
        if side_length < self.minimum {
            return false;
        }

        if let Some(maximum) = self.maximum {
            if side_length > maximum {
                return false;
            }
        }

        if side_length % 2 == 0 {
            self.supports_even
        } else {
            self.supports_odd
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AlgorithmContract {
    pub side_lengths: AlgorithmSideLengthSupport,
    pub requires_previous_stages_solved: bool,
    pub standard_preconditions: &'static [&'static str],
    pub standard_postconditions: &'static [&'static str],
    pub execution_mode_support: AlgorithmExecutionSupport,
}

impl AlgorithmContract {
    pub const fn new(
        side_lengths: AlgorithmSideLengthSupport,
        requires_previous_stages_solved: bool,
        standard_preconditions: &'static [&'static str],
        standard_postconditions: &'static [&'static str],
        execution_mode_support: AlgorithmExecutionSupport,
    ) -> Self {
        Self {
            side_lengths,
            requires_previous_stages_solved,
            standard_preconditions,
            standard_postconditions,
            execution_mode_support,
        }
    }

    pub const fn supports(self, side_length: usize, mode: ExecutionMode) -> bool {
        self.side_lengths.supports(side_length) && self.execution_mode_support.supports(mode)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AlgorithmStepSpec {
    pub phase: SolvePhase,
    pub name: &'static str,
    pub description: &'static str,
}

impl AlgorithmStepSpec {
    pub const fn new(phase: SolvePhase, name: &'static str, description: &'static str) -> Self {
        Self {
            phase,
            name,
            description,
        }
    }
}

pub trait SolveAlgorithm<S: FaceletArray> {
    fn phase(&self) -> SolvePhase;
    fn name(&self) -> &'static str;
    fn contract(&self) -> AlgorithmContract;

    fn execution_mode_support(&self) -> AlgorithmExecutionSupport {
        self.contract().execution_mode_support
    }

    fn side_length_support(&self) -> AlgorithmSideLengthSupport {
        self.contract().side_lengths
    }

    fn requires_previous_stages_solved(&self) -> bool {
        self.contract().requires_previous_stages_solved
    }

    fn is_applicable_to_side_length(&self, side_length: usize) -> bool {
        self.side_length_support().supports(side_length)
    }

    fn steps(&self) -> &[AlgorithmStepSpec];
    fn run(&mut self, cube: &mut Cube<S>, context: &mut SolveContext) -> SolveResult<()>;
}
