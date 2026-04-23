use crate::{
    algorithms::{
        AlgorithmContract, AlgorithmExecutionSupport, AlgorithmSideLengthSupport,
        AlgorithmStepSpec, MoveSequenceOperation, SolveAlgorithm,
    },
    cube::{Cube, FaceletLocation},
    face::FaceId,
    solver::{SolveContext, SolveError, SolvePhase, SolveResult},
    storage::FaceletArray,
};

use super::search::{all_corner_facelets_solved, read_corner_state, CornerMoveTables};

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CornerSlot {
    UFR = 0,
    ULF = 1,
    UBL = 2,
    URB = 3,
    DFR = 4,
    DLF = 5,
    DBL = 6,
    DRB = 7,
}

impl CornerSlot {
    pub const ALL: [Self; 8] = [
        Self::UFR,
        Self::ULF,
        Self::UBL,
        Self::URB,
        Self::DFR,
        Self::DLF,
        Self::DBL,
        Self::DRB,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn faces(self) -> [FaceId; 3] {
        match self {
            Self::UFR => [FaceId::U, FaceId::R, FaceId::F],
            Self::ULF => [FaceId::U, FaceId::F, FaceId::L],
            Self::UBL => [FaceId::U, FaceId::L, FaceId::B],
            Self::URB => [FaceId::U, FaceId::B, FaceId::R],
            Self::DFR => [FaceId::D, FaceId::F, FaceId::R],
            Self::DLF => [FaceId::D, FaceId::L, FaceId::F],
            Self::DBL => [FaceId::D, FaceId::B, FaceId::L],
            Self::DRB => [FaceId::D, FaceId::R, FaceId::B],
        }
    }

    pub fn stickers(self, side_length: usize) -> [FaceletLocation; 3] {
        let last = side_length - 1;

        match self {
            Self::UFR => [
                FaceletLocation {
                    face: FaceId::U,
                    row: last,
                    col: last,
                },
                FaceletLocation {
                    face: FaceId::R,
                    row: 0,
                    col: 0,
                },
                FaceletLocation {
                    face: FaceId::F,
                    row: 0,
                    col: last,
                },
            ],
            Self::ULF => [
                FaceletLocation {
                    face: FaceId::U,
                    row: last,
                    col: 0,
                },
                FaceletLocation {
                    face: FaceId::F,
                    row: 0,
                    col: 0,
                },
                FaceletLocation {
                    face: FaceId::L,
                    row: 0,
                    col: last,
                },
            ],
            Self::UBL => [
                FaceletLocation {
                    face: FaceId::U,
                    row: 0,
                    col: 0,
                },
                FaceletLocation {
                    face: FaceId::L,
                    row: 0,
                    col: 0,
                },
                FaceletLocation {
                    face: FaceId::B,
                    row: 0,
                    col: last,
                },
            ],
            Self::URB => [
                FaceletLocation {
                    face: FaceId::U,
                    row: 0,
                    col: last,
                },
                FaceletLocation {
                    face: FaceId::B,
                    row: 0,
                    col: 0,
                },
                FaceletLocation {
                    face: FaceId::R,
                    row: 0,
                    col: last,
                },
            ],
            Self::DFR => [
                FaceletLocation {
                    face: FaceId::D,
                    row: 0,
                    col: last,
                },
                FaceletLocation {
                    face: FaceId::F,
                    row: last,
                    col: last,
                },
                FaceletLocation {
                    face: FaceId::R,
                    row: last,
                    col: 0,
                },
            ],
            Self::DLF => [
                FaceletLocation {
                    face: FaceId::D,
                    row: 0,
                    col: 0,
                },
                FaceletLocation {
                    face: FaceId::L,
                    row: last,
                    col: last,
                },
                FaceletLocation {
                    face: FaceId::F,
                    row: last,
                    col: 0,
                },
            ],
            Self::DBL => [
                FaceletLocation {
                    face: FaceId::D,
                    row: last,
                    col: 0,
                },
                FaceletLocation {
                    face: FaceId::B,
                    row: last,
                    col: last,
                },
                FaceletLocation {
                    face: FaceId::L,
                    row: last,
                    col: 0,
                },
            ],
            Self::DRB => [
                FaceletLocation {
                    face: FaceId::D,
                    row: last,
                    col: last,
                },
                FaceletLocation {
                    face: FaceId::R,
                    row: last,
                    col: last,
                },
                FaceletLocation {
                    face: FaceId::B,
                    row: last,
                    col: 0,
                },
            ],
        }
    }

    pub fn from_faces(first: FaceId, second: FaceId, third: FaceId) -> Option<Self> {
        let faces = [first, second, third];

        CornerSlot::ALL
            .into_iter()
            .find(|slot| slot.faces().iter().all(|face| faces.contains(face)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CornerSearchAlgorithm {
    tables: CornerMoveTables,
    steps: [AlgorithmStepSpec; 4],
}

const CORNER_STAGE_STANDARD_PRECONDITIONS: &[&str] =
    &["none; the corner stage may start from any cube state"];
const CORNER_STAGE_STANDARD_POSTCONDITIONS: &[&str] =
    &["all corner facelets are solved when the stage returns success"];
const CORNER_ALGORITHM_CONTRACT: AlgorithmContract = AlgorithmContract::new(
    AlgorithmSideLengthSupport::all(),
    false,
    CORNER_STAGE_STANDARD_PRECONDITIONS,
    CORNER_STAGE_STANDARD_POSTCONDITIONS,
    AlgorithmExecutionSupport::StandardAndOptimized,
);

impl Default for CornerSearchAlgorithm {
    fn default() -> Self {
        Self {
            tables: CornerMoveTables::new(),
            steps: [
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner state extraction",
                    "read corner permutation and orientation from the current cube state",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner move tables",
                    "reuse reduced corner-state move and pruning tables",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner search",
                    "solve the reduced corner state with outer-face moves only",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Corners,
                    "corner validation",
                    "verify that every corner facelet matches its home face color",
                ),
            ],
        }
    }
}

impl<S: FaceletArray> SolveAlgorithm<S> for CornerSearchAlgorithm {
    fn phase(&self) -> SolvePhase {
        SolvePhase::Corners
    }

    fn name(&self) -> &'static str {
        "corner search"
    }

    fn contract(&self) -> AlgorithmContract {
        CORNER_ALGORITHM_CONTRACT
    }

    fn steps(&self) -> &[AlgorithmStepSpec] {
        &self.steps
    }

    fn run(&mut self, cube: &mut Cube<S>, context: &mut SolveContext) -> SolveResult<()> {
        if cube.side_len() < 2 || all_corner_facelets_solved(cube) {
            return Ok(());
        }

        let state = read_corner_state(cube).ok_or(SolveError::StageFailed {
            stage: "corner search",
            reason: "could not read a valid reduced corner state",
        })?;
        let solution = self.tables.solve(state).ok_or(SolveError::StageFailed {
            stage: "corner search",
            reason: "corner search did not find a solution",
        })?;

        let side_length = cube.side_len();
        let moves = solution
            .into_iter()
            .map(|spec| spec.move_for_side_length(side_length))
            .collect::<Vec<_>>();
        let operation = MoveSequenceOperation::new(side_length, &moves);
        context.apply_operation(cube, &operation);

        if all_corner_facelets_solved(cube) {
            Ok(())
        } else {
            Err(SolveError::StageFailed {
                stage: "corner search",
                reason: "corner solving left some corner facelets unsolved",
            })
        }
    }
}

pub type CornerSearchStage = CornerSearchAlgorithm;
