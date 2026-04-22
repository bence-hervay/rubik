use super::ExecutionMode;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SolveOptions {
    pub record_moves: bool,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self::standard()
    }
}

impl SolveOptions {
    pub const fn new(execution_mode: ExecutionMode) -> Self {
        Self {
            record_moves: execution_mode.records_moves(),
        }
    }

    pub const fn standard() -> Self {
        Self::new(ExecutionMode::Standard)
    }

    pub const fn optimized() -> Self {
        Self::new(ExecutionMode::Optimized)
    }

    pub const fn execution_mode(self) -> ExecutionMode {
        if self.record_moves {
            ExecutionMode::Standard
        } else {
            ExecutionMode::Optimized
        }
    }

    pub const fn with_execution_mode(mut self, execution_mode: ExecutionMode) -> Self {
        self.record_moves = execution_mode.records_moves();
        self
    }
}
