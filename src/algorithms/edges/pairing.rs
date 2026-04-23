use std::time::{Duration, Instant};

use crate::{
    algorithms::{
        AlgorithmContract, AlgorithmExecutionSupport, AlgorithmSideLengthSupport,
        AlgorithmStepSpec, SolveAlgorithm,
    },
    cube::Cube,
    face::FaceId,
    solver::{SolveContext, SolveError, SolvePhase, SolveResult, StageProgressSpec},
    storage::FaceletArray,
};

use super::core::{
    all_edge_facelets_solved, solve_middle_edges, solve_wing_orbit, solve_wing_orientations,
    solved_edge_slot_keys, wings_match_solved_slots_from_cube, EdgeColorKey,
};
use super::prepared::PreparedEdgeTables;

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

    pub(super) fn solved_key(self) -> EdgeColorKey {
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
pub struct EdgePairingAlgorithm {
    slots: [EdgeSlot; 12],
    steps: [AlgorithmStepSpec; 6],
    cache: Option<PreparedEdgeTables>,
}

const EDGE_STAGE_STANDARD_PRECONDITIONS: &[&str] =
    &["none; the edge stage may start from any cube state"];
const EDGE_STAGE_STANDARD_POSTCONDITIONS: &[&str] =
    &["all edge facelets are solved when the stage returns success"];
const EDGE_ALGORITHM_CONTRACT: AlgorithmContract = AlgorithmContract::new(
    AlgorithmSideLengthSupport::all(),
    false,
    EDGE_STAGE_STANDARD_PRECONDITIONS,
    EDGE_STAGE_STANDARD_POSTCONDITIONS,
    AlgorithmExecutionSupport::StandardAndOptimized,
);

impl Default for EdgePairingAlgorithm {
    fn default() -> Self {
        Self {
            slots: EdgeSlot::ALL,
            steps: [
                AlgorithmStepSpec::new(
                    SolvePhase::Edges,
                    "edge orbit tables",
                    "precompute exact wing three-cycle setup tables for each edge orbit",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Edges,
                    "edge wing cycles",
                    "solve each wing orbit to the home edge-slot colors using exact sparse three-cycles",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Edges,
                    "edge wing validation",
                    "verify that every wing orbit matches the solved edge-slot colors",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Edges,
                    "middle edge setup",
                    "prepare exact middle-edge setup tables and canonical edge-slot setup moves",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Edges,
                    "middle edge solve",
                    "solve odd-cube middle edges to their home slots using exact middle-edge cycles",
                ),
                AlgorithmStepSpec::new(
                    SolvePhase::Edges,
                    "edge validation",
                    "verify that all edge facelets are in their home positions",
                ),
            ],
            cache: None,
        }
    }
}

impl EdgePairingAlgorithm {
    pub fn slots(&self) -> &[EdgeSlot; 12] {
        &self.slots
    }

    fn ensure_cache(&mut self, side_length: usize) -> &mut PreparedEdgeTables {
        let rebuild = self
            .cache
            .as_ref()
            .map(|cache| cache.side_length != side_length)
            .unwrap_or(true);
        if rebuild {
            self.cache = Some(PreparedEdgeTables::new(side_length));
        }
        self.cache
            .as_mut()
            .expect("edge pairing cache must be initialized")
    }
}

#[derive(Clone, Debug)]
pub(super) struct EdgeStageProgressTracker {
    processed_orbits: Vec<bool>,
}

impl EdgeStageProgressTracker {
    fn new(side_length: usize) -> Self {
        Self {
            processed_orbits: vec![false; wing_orbit_count(side_length)],
        }
    }

    fn total_work(&self) -> usize {
        self.processed_orbits.len() * EdgeSlot::ALL.len() * 2
    }

    pub(super) fn observe_wing_orbit(&mut self, row: usize) -> usize {
        let index = row
            .checked_sub(1)
            .expect("wing orbit rows are one-indexed inner layers");
        let Some(processed) = self.processed_orbits.get_mut(index) else {
            return 0;
        };

        if std::mem::replace(processed, true) {
            0
        } else {
            EdgeSlot::ALL.len() * 2
        }
    }
}

impl<S: FaceletArray> SolveAlgorithm<S> for EdgePairingAlgorithm {
    fn phase(&self) -> SolvePhase {
        SolvePhase::Edges
    }

    fn name(&self) -> &'static str {
        "edge pairing"
    }

    fn contract(&self) -> AlgorithmContract {
        EDGE_ALGORITHM_CONTRACT
    }

    fn steps(&self) -> &[AlgorithmStepSpec] {
        &self.steps
    }

    fn run(&mut self, cube: &mut Cube<S>, context: &mut SolveContext) -> SolveResult<()> {
        if cube.side_len() < 3 {
            return Ok(());
        }

        let mut progress = context
            .progress_enabled()
            .then(|| EdgeStageProgressTracker::new(cube.side_len()));
        let total_work = progress
            .as_ref()
            .map_or(0, EdgeStageProgressTracker::total_work);

        context.with_stage_progress(
            StageProgressSpec::new(SolvePhase::Edges, "edge pairing", total_work, "wing pieces"),
            |context| {
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
                        solve_wing_orientations(
                            cube,
                            context,
                            orbit,
                            slot_setups,
                            progress.as_mut(),
                        )?;
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
                        solve_wing_orientations(
                            cube,
                            context,
                            orbit,
                            slot_setups,
                            progress.as_mut(),
                        )?;
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
        )
    }
}

pub type EdgePairingStage = EdgePairingAlgorithm;

fn wing_orbit_count(side_length: usize) -> usize {
    side_length.saturating_sub(2) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wing_orbit_count_matches_scaling_rows() {
        assert_eq!(wing_orbit_count(3), 0);
        assert_eq!(wing_orbit_count(4), 1);
        assert_eq!(wing_orbit_count(5), 1);
        assert_eq!(wing_orbit_count(6), 2);
        assert_eq!(wing_orbit_count(7), 2);
    }

    #[test]
    fn edge_progress_tracker_counts_each_wing_orbit_once() {
        let mut tracker = EdgeStageProgressTracker::new(8);

        assert_eq!(tracker.total_work(), 72);
        assert_eq!(tracker.observe_wing_orbit(1), 24);
        assert_eq!(tracker.observe_wing_orbit(1), 0);
        assert_eq!(tracker.observe_wing_orbit(2), 24);
        assert_eq!(tracker.observe_wing_orbit(3), 24);
    }
}
