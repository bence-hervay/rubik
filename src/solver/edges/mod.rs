use std::time::{Duration, Instant};

use crate::{cube::Cube, face::FaceId, storage::FaceletArray, support::edges::PreparedEdgeStage};

use super::{
    SolveContext, SolveError, SolvePhase, SolveResult, SolverStage, StageContract,
    StageExecutionSupport, StageSideLengthSupport, SubStageSpec,
};

mod core;

use core::{
    all_edge_facelets_solved, solve_middle_edges, solve_wing_orbit, solve_wing_orientations,
    solved_edge_slot_keys, wings_match_solved_slots_from_cube, EdgeColorKey,
};
pub(crate) use core::{
    EdgeSlotSetupTable, MiddleOrbitTable, WingOrbitSetupTemplate, WingOrbitTable,
};

#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum EdgeSlot {
    UF = 0,
    UR = 1,
    UB = 2,
    UL = 3,
    FR = 4,
    FL = 5,
    BR = 6,
    BL = 7,
    DF = 8,
    DR = 9,
    DB = 10,
    DL = 11,
}

impl EdgeSlot {
    pub const ALL: [Self; 12] = [
        Self::UF,
        Self::UR,
        Self::UB,
        Self::UL,
        Self::FR,
        Self::FL,
        Self::BR,
        Self::BL,
        Self::DF,
        Self::DR,
        Self::DB,
        Self::DL,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn faces(self) -> (FaceId, FaceId) {
        match self {
            Self::UF => (FaceId::U, FaceId::F),
            Self::UR => (FaceId::U, FaceId::R),
            Self::UB => (FaceId::U, FaceId::B),
            Self::UL => (FaceId::U, FaceId::L),
            Self::FR => (FaceId::F, FaceId::R),
            Self::FL => (FaceId::F, FaceId::L),
            Self::BR => (FaceId::B, FaceId::R),
            Self::BL => (FaceId::B, FaceId::L),
            Self::DF => (FaceId::D, FaceId::F),
            Self::DR => (FaceId::D, FaceId::R),
            Self::DB => (FaceId::D, FaceId::B),
            Self::DL => (FaceId::D, FaceId::L),
        }
    }

    fn solved_key(self) -> EdgeColorKey {
        let (first, second) = self.faces();
        EdgeColorKey::from_face_ids(first, second)
    }

    pub fn from_faces(first: FaceId, second: FaceId) -> Self {
        match (first, second) {
            (FaceId::U, FaceId::F) | (FaceId::F, FaceId::U) => Self::UF,
            (FaceId::U, FaceId::R) | (FaceId::R, FaceId::U) => Self::UR,
            (FaceId::U, FaceId::B) | (FaceId::B, FaceId::U) => Self::UB,
            (FaceId::U, FaceId::L) | (FaceId::L, FaceId::U) => Self::UL,
            (FaceId::F, FaceId::R) | (FaceId::R, FaceId::F) => Self::FR,
            (FaceId::F, FaceId::L) | (FaceId::L, FaceId::F) => Self::FL,
            (FaceId::B, FaceId::R) | (FaceId::R, FaceId::B) => Self::BR,
            (FaceId::B, FaceId::L) | (FaceId::L, FaceId::B) => Self::BL,
            (FaceId::D, FaceId::F) | (FaceId::F, FaceId::D) => Self::DF,
            (FaceId::D, FaceId::R) | (FaceId::R, FaceId::D) => Self::DR,
            (FaceId::D, FaceId::B) | (FaceId::B, FaceId::D) => Self::DB,
            (FaceId::D, FaceId::L) | (FaceId::L, FaceId::D) => Self::DL,
            _ => panic!("invalid edge slot face pair: {first:?}/{second:?}"),
        }
    }
}

#[derive(Debug)]
pub struct EdgePairingStage {
    slots: [EdgeSlot; 12],
    sub_stages: [SubStageSpec; 6],
    cache: Option<PreparedEdgeStage>,
}

const EDGE_STAGE_STANDARD_PRECONDITIONS: &[&str] =
    &["none; the edge stage may start from any cube state"];
const EDGE_STAGE_STANDARD_POSTCONDITIONS: &[&str] =
    &["all edge facelets are solved when the stage returns success"];
const EDGE_STAGE_CONTRACT: StageContract = StageContract::new(
    StageSideLengthSupport::all(),
    false,
    EDGE_STAGE_STANDARD_PRECONDITIONS,
    EDGE_STAGE_STANDARD_POSTCONDITIONS,
    StageExecutionSupport::StandardAndOptimized,
);

impl Default for EdgePairingStage {
    fn default() -> Self {
        Self {
            slots: EdgeSlot::ALL,
            sub_stages: [
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "edge orbit tables",
                    "precompute exact wing three-cycle setup tables for each edge orbit",
                ),
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "edge wing cycles",
                    "solve each wing orbit to the home edge-slot colors using exact sparse three-cycles",
                ),
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "edge wing validation",
                    "verify that every wing orbit matches the solved edge-slot colors",
                ),
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "middle edge setup",
                    "prepare exact middle-edge setup tables and canonical edge-slot setup moves",
                ),
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "middle edge solve",
                    "solve odd-cube middle edges to their home slots using exact middle-edge cycles",
                ),
                SubStageSpec::new(
                    SolvePhase::Edges,
                    "edge validation",
                    "verify that all edge facelets are in their home positions",
                ),
            ],
            cache: None,
        }
    }
}

impl EdgePairingStage {
    pub fn slots(&self) -> &[EdgeSlot; 12] {
        &self.slots
    }

    fn ensure_cache(&mut self, side_length: usize) -> &mut PreparedEdgeStage {
        let rebuild = self
            .cache
            .as_ref()
            .map(|cache| cache.side_length != side_length)
            .unwrap_or(true);
        if rebuild {
            self.cache = Some(PreparedEdgeStage::new(side_length));
        }
        self.cache
            .as_mut()
            .expect("edge pairing cache must be initialized")
    }
}

impl<S: FaceletArray> SolverStage<S> for EdgePairingStage {
    fn phase(&self) -> SolvePhase {
        SolvePhase::Edges
    }

    fn name(&self) -> &'static str {
        "edge pairing"
    }

    fn contract(&self) -> StageContract {
        EDGE_STAGE_CONTRACT
    }

    fn sub_stages(&self) -> &[SubStageSpec] {
        &self.sub_stages
    }

    fn run(&mut self, cube: &mut Cube<S>, context: &mut SolveContext) -> SolveResult<()> {
        if cube.side_len() < 3 {
            return Ok(());
        }

        let profile = std::env::var_os("RUBIK_EDGE_PROFILE").is_some();
        let mut wing_slot_total = Duration::ZERO;
        let mut wing_orientation_total = Duration::ZERO;
        let mut middle_total = Duration::ZERO;
        let mut wing_resolve_after_middle_total = Duration::ZERO;

        let slot_keys = solved_edge_slot_keys();
        let cache = self.ensure_cache(cube.side_len());

        for orbit in &mut cache.wing_orbits {
            let slot_start = Instant::now();
            solve_wing_orbit(cube, context, orbit, &slot_keys)?;
            wing_slot_total += slot_start.elapsed();
            if let Some(slot_setups) = &cache.slot_setups {
                let orientation_start = Instant::now();
                solve_wing_orientations(cube, context, orbit, slot_setups)?;
                wing_orientation_total += orientation_start.elapsed();
            }
        }

        if !wings_match_solved_slots_from_cube(cache, cube, &slot_keys) {
            return Err(SolveError::StageFailed {
                stage: "edge pairing",
                reason: "wing solving left a home edge-slot orbit unsolved",
            });
        }

        if let (Some(middle_orbit), Some(slot_setups)) =
            (&mut cache.middle_orbit, &cache.slot_setups)
        {
            let middle_start = Instant::now();
            solve_middle_edges(cube, context, middle_orbit, slot_setups)?;
            middle_total += middle_start.elapsed();

            let refreshed_slot_keys = solved_edge_slot_keys();
            for orbit in &mut cache.wing_orbits {
                let rewing_start = Instant::now();
                solve_wing_orbit(cube, context, orbit, &refreshed_slot_keys)?;
                solve_wing_orientations(cube, context, orbit, slot_setups)?;
                wing_resolve_after_middle_total += rewing_start.elapsed();
            }
        }

        if profile {
            eprintln!(
                "edge profile: n={} wing_slot={:.3?} wing_orientation={:.3?} middle={:.3?} wing_after_middle={:.3?}",
                cube.side_len(),
                wing_slot_total,
                wing_orientation_total,
                middle_total,
                wing_resolve_after_middle_total,
            );
        }

        if all_edge_facelets_solved(cube) {
            Ok(())
        } else {
            Err(SolveError::StageFailed {
                stage: "edge pairing",
                reason: "edge stage left some edge facelets unsolved",
            })
        }
    }
}
