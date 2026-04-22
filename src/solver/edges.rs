use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

use crate::{
    conventions::{face_layer_move, face_outer_move, home_facelet_for_face, normalize_face_pair},
    cube::{
        edge_cubie_for_facelet_location, edge_cubie_orbit_index,
        edge_three_cycle_plan_from_updates, trace_edge_cubie_through_move, Cube, EdgeCubieLocation,
        EdgeThreeCycle, EdgeThreeCycleDirection, EdgeThreeCyclePlan, FaceletLocation,
        FaceletUpdate,
    },
    face::FaceId,
    facelet::Facelet,
    geometry,
    moves::{Move, MoveAngle},
    storage::{Byte, FaceletArray},
};

use super::{SolveContext, SolveError, SolvePhase, SolveResult, SolverStage, SubStageSpec};

const EDGE_WING_POSITION_COUNT: usize = 24;
#[cfg(test)]
const EDGE_WING_TRIPLE_COUNT: usize = 24 * 23 * 22;
const EDGE_MIDDLE_POSITION_COUNT: usize = 12;
#[cfg(test)]
const EDGE_MIDDLE_TRIPLE_COUNT: usize = 12 * 11 * 10;

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

#[derive(Debug)]
struct PreparedEdgeStage {
    side_length: usize,
    wing_orbits: Vec<WingOrbitTable>,
    middle_orbit: Option<MiddleOrbitTable>,
    slot_setups: Option<EdgeSlotSetupTable>,
}

impl PreparedEdgeStage {
    fn new(side_length: usize) -> Self {
        let profile = std::env::var_os("RUBIK_EDGE_PROFILE").is_some();
        let start = Instant::now();
        let slot_setups = if side_length >= 3 {
            Some(EdgeSlotSetupTable::new(side_length))
        } else {
            None
        };
        let slot_setup_elapsed = start.elapsed();

        let wing_start = Instant::now();
        let mut wing_orbits = Vec::new();
        if side_length >= 4 {
            let setup_template = WingOrbitSetupTemplate::new(side_length);
            for row in 1..=(side_length - 2) / 2 {
                wing_orbits.push(WingOrbitTable::new(side_length, row, &setup_template));
            }
        }
        let wing_elapsed = wing_start.elapsed();

        let orientation_start = Instant::now();
        if let Some(slot_setups) = &slot_setups {
            if let Some((first_orbit, remaining_orbits)) = wing_orbits.split_first_mut() {
                let cache = first_orbit.build_orientation_cache(slot_setups);
                for orbit in remaining_orbits {
                    orbit.set_orientation_cache(cache.clone());
                }
            }
        }
        let orientation_elapsed = orientation_start.elapsed();

        let middle_start = Instant::now();
        let middle_orbit = if side_length >= 3 && side_length % 2 == 1 {
            Some(MiddleOrbitTable::new(
                side_length,
                slot_setups
                    .as_ref()
                    .expect("slot setups must exist for odd middle-edge solving"),
            ))
        } else {
            None
        };
        let middle_elapsed = middle_start.elapsed();

        if profile {
            eprintln!(
                "edge prepare: n={} slot_setups={:.3?} wing_orbits={:.3?} wing_orientation_cache={:.3?} middle={:.3?}",
                side_length,
                slot_setup_elapsed,
                wing_elapsed,
                orientation_elapsed,
                middle_elapsed,
            );
        }

        Self {
            side_length,
            wing_orbits,
            middle_orbit,
            slot_setups,
        }
    }
}

#[derive(Debug)]
struct WingOrbitTable {
    row: usize,
    side_length: usize,
    positions: Vec<FixedEdgePosition>,
    slot_positions: [[usize; 2]; 12],
    planner: OrbitCyclePlanner,
    orientation_generators: Vec<OrientationGenerator>,
    orientation_masks: Vec<u16>,
    orientation_nodes: Vec<Option<MaskNode>>,
    #[cfg_attr(not(test), allow(dead_code))]
    reachable_ordered_triples: usize,
}

#[derive(Clone, Debug)]
struct WingOrientationCache {
    orientation_generators: Vec<OrientationGenerator>,
    orientation_masks: Vec<u16>,
    orientation_nodes: Vec<Option<MaskNode>>,
}

#[derive(Clone, Debug)]
struct WingOrbitSetupTemplate {
    setup_nodes: Arc<[Option<SetupNode>]>,
    start_key: usize,
    reachable_ordered_triples: usize,
    base_plan: WingBasePlanTemplate,
}

#[derive(Copy, Clone, Debug)]
struct WingBasePlanTemplate {
    sources: [usize; 3],
    destinations: [usize; 3],
    reversed: [bool; 3],
}

#[derive(Debug)]
struct MiddleOrbitTable {
    side_length: usize,
    positions: Vec<FixedEdgePosition>,
    slot_positions: [usize; 12],
    target_slot_index: BTreeMap<EdgeColorKey, usize>,
    planner: OrbitCyclePlanner,
    orientation_generators: Vec<OrientationGenerator>,
    orientation_masks: Vec<u16>,
    orientation_nodes: Vec<Option<MaskNode>>,
    #[cfg_attr(not(test), allow(dead_code))]
    reachable_ordered_triples: usize,
}

#[derive(Debug)]
struct EdgeSlotSetupTable {
    to_destination: [Vec<Move>; 12],
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct OrbitThreeCycleSpec {
    position_count: usize,
    ordered_positions: [usize; 3],
}

impl OrbitThreeCycleSpec {
    fn new(position_count: usize, ordered_positions: [usize; 3]) -> Self {
        Self::try_new(position_count, ordered_positions)
            .expect("orbit three-cycle positions must be distinct valid orbit indices")
    }

    fn try_new(position_count: usize, ordered_positions: [usize; 3]) -> Option<Self> {
        if ordered_positions
            .iter()
            .any(|index| *index >= position_count)
        {
            return None;
        }
        if ordered_positions[0] == ordered_positions[1]
            || ordered_positions[0] == ordered_positions[2]
            || ordered_positions[1] == ordered_positions[2]
        {
            return None;
        }

        Some(Self {
            position_count,
            ordered_positions,
        })
    }

    fn ordered_positions(self) -> [usize; 3] {
        self.ordered_positions
    }

    fn reversed(self) -> Self {
        Self {
            position_count: self.position_count,
            ordered_positions: [
                self.ordered_positions[0],
                self.ordered_positions[2],
                self.ordered_positions[1],
            ],
        }
    }

    fn encode(self) -> usize {
        encode_triple_with_base(self.position_count, self.ordered_positions)
    }

    fn decode(position_count: usize, key: usize) -> Self {
        Self::new(position_count, decode_triple_with_base(position_count, key))
    }
}

#[derive(Clone, Debug)]
struct OrbitCyclePlanner {
    position_count: usize,
    setup_moves: Vec<Move>,
    setup_nodes: Arc<[Option<SetupNode>]>,
    start_key: usize,
    base_plan: EdgeThreeCyclePlan,
    inverse_base_plan: EdgeThreeCyclePlan,
    plan_cache: HashMap<usize, EdgeThreeCyclePlan>,
}

impl OrbitCyclePlanner {
    fn new(
        position_count: usize,
        setup_moves: Vec<Move>,
        setup_nodes: Arc<[Option<SetupNode>]>,
        start_key: usize,
        base_plan: EdgeThreeCyclePlan,
    ) -> Self {
        let inverse_base_plan = base_plan.inverted();

        Self {
            position_count,
            setup_moves,
            setup_nodes,
            start_key,
            base_plan,
            inverse_base_plan,
            plan_cache: HashMap::new(),
        }
    }

    fn plan_for_cycle(
        &mut self,
        cycle: OrbitThreeCycleSpec,
        positions: &[FixedEdgePosition],
        side_length: usize,
    ) -> Option<&EdgeThreeCyclePlan> {
        if cycle.position_count != self.position_count {
            return None;
        }

        let direct_key = cycle.encode();
        if self.has_setup(direct_key) {
            return self.plan_for_encoded_cycle(direct_key, false, positions, side_length);
        }

        let reverse_key = cycle.reversed().encode();
        if self.has_setup(reverse_key) {
            return self.plan_for_encoded_cycle(reverse_key, true, positions, side_length);
        }

        None
    }

    fn has_setup(&self, setup_key: usize) -> bool {
        self.setup_nodes
            .get(setup_key)
            .and_then(|node| *node)
            .is_some()
    }

    fn plan_for_encoded_cycle(
        &mut self,
        setup_key: usize,
        use_inverse_base: bool,
        positions: &[FixedEdgePosition],
        side_length: usize,
    ) -> Option<&EdgeThreeCyclePlan> {
        let cache_key = setup_key * 2 + usize::from(use_inverse_base);
        if !self.plan_cache.contains_key(&cache_key) {
            let setup_moves = self.reconstruct_setup_moves(setup_key)?;
            let base_plan = if use_inverse_base {
                &self.inverse_base_plan
            } else {
                &self.base_plan
            };

            let plan = base_plan.conjugated_by_moves(&setup_moves);
            let actual = plan_cubie_positions_in_orbit(side_length, positions, plan.cubies())?;
            let expected = OrbitThreeCycleSpec::decode(self.position_count, setup_key);
            if !same_cyclic_order(actual, expected.ordered_positions()) {
                return None;
            }

            self.plan_cache.insert(cache_key, plan);
        }

        self.plan_cache.get(&cache_key)
    }

    fn reconstruct_setup_moves(&self, target_key: usize) -> Option<Vec<Move>> {
        let mut current_key = target_key;
        let mut reversed = Vec::new();

        while current_key != self.start_key {
            let node = self.setup_nodes.get(current_key).and_then(|entry| *entry)?;
            reversed.push(self.setup_moves[node.move_index as usize]);
            current_key = node.prev as usize;
        }

        reversed.reverse();
        Some(reversed)
    }
}

impl WingOrbitTable {
    fn new(side_length: usize, row: usize, setup_template: &WingOrbitSetupTemplate) -> Self {
        let positions = wing_orbit_positions(side_length, row);
        assert_eq!(
            positions.len(),
            EDGE_WING_POSITION_COUNT,
            "wing orbit must contain 24 positions",
        );

        let slot_positions = wing_slot_positions();
        let base_plan = setup_template
            .base_plan
            .instantiate(side_length, row, &positions);
        let base_triple = base_plan.cubies().map(|cubie| {
            position_index(&positions, fixed_edge_position(side_length, cubie))
                .expect("base cubie must be in orbit")
        });
        let setup_moves = orbit_setup_moves(side_length, row);
        let start_key = encode_triple(base_triple);
        debug_assert_eq!(
            start_key, setup_template.start_key,
            "shared wing setup template must match every row-specific base triple",
        );
        let planner = OrbitCyclePlanner::new(
            positions.len(),
            setup_moves,
            setup_template.setup_nodes.clone(),
            setup_template.start_key,
            base_plan,
        );

        Self {
            row,
            side_length,
            positions,
            slot_positions,
            planner,
            orientation_generators: Vec::new(),
            orientation_masks: Vec::new(),
            orientation_nodes: Vec::new(),
            reachable_ordered_triples: setup_template.reachable_ordered_triples,
        }
    }

    fn target_keys(&self, slot_keys: &[EdgeColorKey; 12]) -> Vec<EdgeColorKey> {
        let mut target = vec![slot_keys[0]; self.positions.len()];
        for slot in EdgeSlot::ALL {
            let key = slot_keys[slot.index()];
            let [first, second] = self.slot_positions[slot.index()];
            target[first] = key;
            target[second] = key;
        }
        target
    }

    #[cfg_attr(not(test), allow(dead_code))]
    fn current_keys(&self, view: &EdgeScanView) -> Vec<EdgeColorKey> {
        self.positions
            .iter()
            .copied()
            .map(|position| {
                read_edge_key_from_view(
                    view,
                    self.side_length,
                    cubie_from_fixed_position(self.side_length, position),
                )
            })
            .collect()
    }

    fn current_keys_from_cube<S: FaceletArray>(&self, cube: &Cube<S>) -> Vec<EdgeColorKey> {
        self.positions
            .iter()
            .copied()
            .map(|position| {
                read_edge_key_from_cube(cube, cubie_from_fixed_position(self.side_length, position))
            })
            .collect()
    }

    fn plan_for_cycle(&mut self, cycle: OrbitThreeCycleSpec) -> Option<&EdgeThreeCyclePlan> {
        self.planner
            .plan_for_cycle(cycle, &self.positions, self.side_length)
    }

    fn set_orientation_cache(&mut self, cache: WingOrientationCache) {
        self.orientation_generators = cache.orientation_generators;
        self.orientation_masks = cache.orientation_masks;
        self.orientation_nodes = cache.orientation_nodes;
    }

    fn build_orientation_cache(
        &mut self,
        slot_setups: &EdgeSlotSetupTable,
    ) -> WingOrientationCache {
        for face_map in all_face_maps() {
            for slot in EdgeSlot::ALL {
                let mut cube = Cube::<Byte>::new_solved_with_threads(self.side_length, 1);
                let setup = &slot_setups.to_destination[slot.index()];
                if !setup.is_empty() {
                    cube.apply_moves_untracked_with_threads(setup.iter().copied(), 1);
                }
                cube.apply_moves_untracked_with_threads(
                    wing_row_parity_fix_moves_with_map(self.side_length, self.row, face_map),
                    1,
                );
                if !setup.is_empty() {
                    cube.apply_moves_untracked_with_threads(inverted_moves(setup), 1);
                }
                let mask = wing_flip_mask(self, &EdgeScanView::from_cube(&cube))
                    .expect("wing orientation generator must preserve slot placement");
                if mask == 0 || self.orientation_masks.contains(&mask) {
                    continue;
                }
                self.orientation_generators.push(OrientationGenerator {
                    setup_slot: slot,
                    face_map,
                    kind: OrientationGeneratorKind::WingParityFix,
                });
                self.orientation_masks.push(mask);
            }
        }
        self.orientation_nodes = build_mask_solution_table(&self.orientation_masks);

        WingOrientationCache {
            orientation_generators: self.orientation_generators.clone(),
            orientation_masks: self.orientation_masks.clone(),
            orientation_nodes: self.orientation_nodes.clone(),
        }
    }

    fn orientation_solution(&self, mask: u16) -> Option<Vec<OrientationGenerator>> {
        let mut current = mask as usize;
        let mut reversed = Vec::new();

        while current != 0 {
            let node = self
                .orientation_nodes
                .get(current)
                .and_then(|entry| *entry)?;
            reversed.push(self.orientation_generators[node.generator_index as usize]);
            current = node.prev as usize;
        }

        reversed.reverse();
        Some(reversed)
    }
}

impl WingOrbitSetupTemplate {
    fn new(side_length: usize) -> Self {
        assert!(
            side_length >= 4,
            "wing setup template requires side length at least four",
        );

        let representative_row = 1usize;
        let positions = wing_orbit_positions(side_length, representative_row);
        let base_plan = EdgeThreeCyclePlan::from_cycle(
            side_length,
            EdgeThreeCycle::front_right_wing(representative_row),
        );
        let base_triple = base_plan.cubies().map(|cubie| {
            position_index(&positions, fixed_edge_position(side_length, cubie))
                .expect("representative wing base cubie must stay in the orbit")
        });
        let setup_moves = orbit_setup_moves(side_length, representative_row);
        let transitions = orbit_move_transitions(side_length, &positions, &setup_moves);
        let (setup_nodes, start_key, reachable_ordered_triples) =
            build_setup_table(EDGE_WING_POSITION_COUNT, base_triple, &transitions);
        let base_plan_template =
            WingBasePlanTemplate::from_plan(side_length, &positions, &base_plan);

        Self {
            setup_nodes: Arc::from(setup_nodes),
            start_key,
            reachable_ordered_triples,
            base_plan: base_plan_template,
        }
    }
}

impl WingBasePlanTemplate {
    fn from_plan(
        side_length: usize,
        positions: &[FixedEdgePosition],
        plan: &EdgeThreeCyclePlan,
    ) -> Self {
        let sources = plan.cubies().map(|cubie| {
            position_index(positions, fixed_edge_position(side_length, cubie))
                .expect("representative wing base cubie must stay in the orbit")
        });

        let source_cubies =
            sources.map(|index| cubie_from_fixed_position(side_length, positions[index]));
        let destinations = sources.map(|source_index| {
            let source_cubie = cubie_from_fixed_position(side_length, positions[source_index]);
            sources
                .iter()
                .copied()
                .find(|destination_index| {
                    let destination_cubie =
                        cubie_from_fixed_position(side_length, positions[*destination_index]);
                    edge_transfer_orientation(plan, source_cubie, destination_cubie).is_some()
                })
                .expect("every representative wing source cubie must map to a destination cubie")
        });
        let reversed = std::array::from_fn(|index| {
            edge_transfer_orientation(
                plan,
                source_cubies[index],
                cubie_from_fixed_position(side_length, positions[destinations[index]]),
            )
            .expect("representative wing transfer must define an orientation")
        });

        Self {
            sources,
            destinations,
            reversed,
        }
    }

    fn instantiate(
        self,
        side_length: usize,
        row: usize,
        positions: &[FixedEdgePosition],
    ) -> EdgeThreeCyclePlan {
        let moves = wing_cycle_literal_moves(side_length, row);
        let updates = self
            .sources
            .iter()
            .copied()
            .zip(self.destinations.iter().copied())
            .zip(self.reversed.iter().copied())
            .flat_map(|((source_index, destination_index), reversed)| {
                edge_transfer_updates(
                    cubie_from_fixed_position(side_length, positions[source_index]),
                    cubie_from_fixed_position(side_length, positions[destination_index]),
                    reversed,
                )
            })
            .collect::<Vec<_>>();

        edge_three_cycle_plan_from_updates(
            side_length,
            Some(EdgeThreeCycle::front_right_wing(row)),
            moves,
            updates,
        )
    }
}

impl MiddleOrbitTable {
    fn new(side_length: usize, slot_setups: &EdgeSlotSetupTable) -> Self {
        assert!(
            side_length >= 3 && side_length % 2 == 1,
            "middle orbit table requires an odd side length of at least 3",
        );

        let positions = middle_orbit_positions(side_length);
        assert_eq!(
            positions.len(),
            EDGE_MIDDLE_POSITION_COUNT,
            "middle orbit must contain 12 positions",
        );

        let slot_positions = build_middle_slot_positions(&positions);
        let target_slot_index = EdgeSlot::ALL
            .into_iter()
            .map(|slot| (slot.solved_key(), slot_positions[slot.index()]))
            .collect::<BTreeMap<_, _>>();

        let base_plan = EdgeThreeCyclePlan::from_cycle(
            side_length,
            EdgeThreeCycle::front_right_middle(EdgeThreeCycleDirection::Positive),
        );
        let base_triple = base_plan.cubies().map(|cubie| {
            position_index(&positions, fixed_edge_position(side_length, cubie))
                .expect("base middle-edge cubie must be in orbit")
        });
        let setup_moves = middle_setup_moves(side_length);
        let transitions = middle_move_transitions(side_length, &positions, &setup_moves);
        let (setup_nodes, start_key, reachable_ordered_triples) =
            build_setup_table(EDGE_MIDDLE_POSITION_COUNT, base_triple, &transitions);
        let planner = OrbitCyclePlanner::new(
            positions.len(),
            setup_moves,
            Arc::from(setup_nodes),
            start_key,
            base_plan,
        );

        let mut this = Self {
            side_length,
            positions,
            slot_positions,
            target_slot_index,
            planner,
            orientation_generators: Vec::new(),
            orientation_masks: Vec::new(),
            orientation_nodes: Vec::new(),
            reachable_ordered_triples,
        };
        this.build_orientation_cache(slot_setups);
        this
    }

    #[cfg_attr(not(test), allow(dead_code))]
    #[cfg_attr(not(test), allow(dead_code))]
    #[allow(dead_code)]
    fn current_slot_keys(&self, view: &EdgeScanView) -> Vec<EdgeColorKey> {
        self.positions
            .iter()
            .copied()
            .map(|position| {
                read_edge_key_from_view(
                    view,
                    self.side_length,
                    cubie_from_fixed_position(self.side_length, position),
                )
            })
            .collect()
    }

    fn current_slot_keys_from_cube<S: FaceletArray>(&self, cube: &Cube<S>) -> Vec<EdgeColorKey> {
        self.positions
            .iter()
            .copied()
            .map(|position| {
                read_edge_key_from_cube(cube, cubie_from_fixed_position(self.side_length, position))
            })
            .collect()
    }

    fn plan_for_cycle(&mut self, cycle: OrbitThreeCycleSpec) -> Option<&EdgeThreeCyclePlan> {
        self.planner
            .plan_for_cycle(cycle, &self.positions, self.side_length)
    }

    fn build_orientation_cache(&mut self, slot_setups: &EdgeSlotSetupTable) {
        let side_length = self.side_length;
        for face_map in all_face_maps() {
            for slot in EdgeSlot::ALL {
                let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                let setup = &slot_setups.to_destination[slot.index()];
                if !setup.is_empty() {
                    cube.apply_moves_untracked_with_threads(setup.iter().copied(), 1);
                }
                cube.apply_moves_untracked_with_threads(
                    middle_edge_precheck_moves_with_map(side_length, face_map),
                    1,
                );
                if !setup.is_empty() {
                    cube.apply_moves_untracked_with_threads(inverted_moves(setup), 1);
                }
                let mask = middle_flip_mask(self, &EdgeScanView::from_cube(&cube))
                    .expect("middle orientation generator must preserve slot placement");
                if mask == 0 || self.orientation_masks.contains(&mask) {
                    continue;
                }
                self.orientation_generators.push(OrientationGenerator {
                    setup_slot: slot,
                    face_map,
                    kind: OrientationGeneratorKind::MiddlePrecheck,
                });
                self.orientation_masks.push(mask);
            }
        }
        self.orientation_nodes = build_mask_solution_table(&self.orientation_masks);
    }

    fn orientation_solution(&self, mask: u16) -> Option<Vec<OrientationGenerator>> {
        let mut current = mask as usize;
        let mut reversed = Vec::new();

        while current != 0 {
            let node = self
                .orientation_nodes
                .get(current)
                .and_then(|entry| *entry)?;
            reversed.push(self.orientation_generators[node.generator_index as usize]);
            current = node.prev as usize;
        }

        reversed.reverse();
        Some(reversed)
    }
}

impl EdgeSlotSetupTable {
    fn new(side_length: usize) -> Self {
        let representative_orbit = if side_length % 2 == 1 {
            side_length / 2
        } else {
            1
        };
        let positions = enumerate_edge_orbit_positions(side_length, representative_orbit);
        let slot_positions = build_slot_representatives(&positions);
        let to_destination = build_slot_setup_paths(side_length, &slot_positions, EdgeSlot::FR);

        Self { to_destination }
    }
}

fn rotate_face_x(face: FaceId) -> FaceId {
    match face {
        FaceId::U => FaceId::B,
        FaceId::B => FaceId::D,
        FaceId::D => FaceId::F,
        FaceId::F => FaceId::U,
        FaceId::R => FaceId::R,
        FaceId::L => FaceId::L,
    }
}

fn rotate_face_y(face: FaceId) -> FaceId {
    match face {
        FaceId::F => FaceId::R,
        FaceId::R => FaceId::B,
        FaceId::B => FaceId::L,
        FaceId::L => FaceId::F,
        FaceId::U => FaceId::U,
        FaceId::D => FaceId::D,
    }
}

fn rotate_face_map_x() -> FaceMap {
    FaceMap {
        mapping: FaceId::ALL.map(rotate_face_x),
    }
}

fn rotate_face_map_y() -> FaceMap {
    FaceMap {
        mapping: FaceId::ALL.map(rotate_face_y),
    }
}

fn all_face_maps() -> Vec<FaceMap> {
    let mut maps = Vec::new();
    let mut queue = VecDeque::new();
    let identity = FaceMap::identity();
    maps.push(identity);
    queue.push_back(identity);

    let generators = [rotate_face_map_x(), rotate_face_map_y()];
    while let Some(current) = queue.pop_front() {
        for generator in generators {
            let next = generator.compose(current);
            if maps.contains(&next) {
                continue;
            }
            maps.push(next);
            queue.push_back(next);
        }
    }

    maps
}

#[derive(Copy, Clone, Debug)]
struct SetupNode {
    prev: u16,
    move_index: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct MaskNode {
    prev: u16,
    generator_index: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
struct FaceMap {
    mapping: [FaceId; 6],
}

impl FaceMap {
    const fn identity() -> Self {
        Self {
            mapping: [
                FaceId::U,
                FaceId::D,
                FaceId::R,
                FaceId::L,
                FaceId::F,
                FaceId::B,
            ],
        }
    }

    const fn apply(self, face: FaceId) -> FaceId {
        self.mapping[face.index()]
    }

    fn compose(self, after: Self) -> Self {
        Self {
            mapping: FaceId::ALL.map(|face| self.apply(after.apply(face))),
        }
    }
}

#[derive(Copy, Clone, Debug)]
struct OrientationGenerator {
    setup_slot: EdgeSlot,
    face_map: FaceMap,
    kind: OrientationGeneratorKind,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum OrientationGeneratorKind {
    WingParityFix,
    MiddlePrecheck,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct EdgeColorKey {
    first: u8,
    second: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct OrientedEdgeKey {
    first: u8,
    second: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SlotOrientationState {
    Solved,
    Flipped,
    Invalid,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct FixedEdgePosition {
    coord: geometry::Coord3,
    faces: (FaceId, FaceId),
}

#[derive(Clone, Debug)]
struct EdgeScanView {
    faces: [FaceBoundary; 6],
}

#[derive(Clone, Debug)]
struct FaceBoundary {
    top: Vec<Facelet>,
    bottom: Vec<Facelet>,
    left: Vec<Facelet>,
    right: Vec<Facelet>,
}

impl EdgeColorKey {
    const fn new(first: u8, second: u8) -> Self {
        if first <= second {
            Self { first, second }
        } else {
            Self {
                first: second,
                second: first,
            }
        }
    }

    const fn from_face_ids(first: FaceId, second: FaceId) -> Self {
        Self::new(first as u8, second as u8)
    }

    fn from_facelets(first: Facelet, second: Facelet) -> Self {
        Self::new(first.as_u8(), second.as_u8())
    }
}

impl OrientedEdgeKey {
    const fn from_face_ids(first: FaceId, second: FaceId) -> Self {
        Self {
            first: first as u8,
            second: second as u8,
        }
    }

    fn from_facelets(first: Facelet, second: Facelet) -> Self {
        Self {
            first: first.as_u8(),
            second: second.as_u8(),
        }
    }
}

fn reverse_oriented_edge_key(key: OrientedEdgeKey) -> OrientedEdgeKey {
    OrientedEdgeKey {
        first: key.second,
        second: key.first,
    }
}

fn slot_position_target_key(side_length: usize, position: FixedEdgePosition) -> OrientedEdgeKey {
    home_oriented_edge_key(cubie_from_fixed_position(side_length, position))
}

impl EdgeScanView {
    fn from_cube<S: FaceletArray>(cube: &Cube<S>) -> Self {
        let side_length = cube.side_len();
        let faces = FaceId::ALL.map(|face| {
            let face_ref = cube.face(face);
            let mut top = Vec::with_capacity(side_length.saturating_sub(2));
            let mut bottom = Vec::with_capacity(side_length.saturating_sub(2));
            let mut left = Vec::with_capacity(side_length.saturating_sub(2));
            let mut right = Vec::with_capacity(side_length.saturating_sub(2));

            for offset in 1..side_length.saturating_sub(1) {
                top.push(face_ref.get(0, offset));
                bottom.push(face_ref.get(side_length - 1, offset));
                left.push(face_ref.get(offset, 0));
                right.push(face_ref.get(offset, side_length - 1));
            }

            FaceBoundary {
                top,
                bottom,
                left,
                right,
            }
        });

        Self { faces }
    }

    fn get(&self, location: FaceletLocation, side_length: usize) -> Facelet {
        assert!(location.row < side_length, "row out of bounds");
        assert!(location.col < side_length, "col out of bounds");
        assert!(
            location.row == 0
                || location.row + 1 == side_length
                || location.col == 0
                || location.col + 1 == side_length,
            "edge scan view only stores boundary facelets",
        );
        assert!(
            location.row > 0 || (location.col > 0 && location.col + 1 < side_length),
            "edge scan view excludes corners",
        );
        assert!(
            location.col > 0 || (location.row > 0 && location.row + 1 < side_length),
            "edge scan view excludes corners",
        );

        let boundary = &self.faces[location.face.index()];
        let index = if location.row == 0 {
            location.col - 1
        } else if location.row + 1 == side_length {
            location.col - 1
        } else if location.col == 0 {
            location.row - 1
        } else {
            location.row - 1
        };

        if location.row == 0 {
            boundary.top[index]
        } else if location.row + 1 == side_length {
            boundary.bottom[index]
        } else if location.col == 0 {
            boundary.left[index]
        } else {
            boundary.right[index]
        }
    }
}

fn solve_wing_orbit<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    orbit: &mut WingOrbitTable,
    slot_keys: &[EdgeColorKey; 12],
) -> SolveResult<()> {
    let target = orbit.target_keys(slot_keys);
    let current = orbit.current_keys_from_cube(cube);
    if current == target {
        return Ok(());
    }

    let assignment = build_even_assignment(&current, &target).ok_or(SolveError::StageFailed {
        stage: "edge pairing",
        reason: "could not assign reduced edge targets for a wing orbit",
    })?;
    let cycles =
        ordered_three_cycles_for_assignment(&assignment).ok_or(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "could not decompose a wing orbit assignment into exact three-cycles",
        })?;

    for cycle in cycles {
        let plan = orbit.plan_for_cycle(cycle).ok_or(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "missing exact setup table entry for a wing orbit three-cycle",
        })?;
        context.apply_edge_three_cycle_plan(cube, &plan);
    }

    if orbit.current_keys_from_cube(cube) == target {
        Ok(())
    } else {
        Err(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "wing orbit pairing did not reach its reduced edge target",
        })
    }
}

fn solve_middle_edges<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    orbit: &mut MiddleOrbitTable,
    slot_setups: &EdgeSlotSetupTable,
) -> SolveResult<()> {
    for _attempt in 0..4 {
        solve_middle_slots(cube, context, orbit)?;

        match solve_middle_orientations(cube, context, orbit, slot_setups) {
            Ok(()) => return Ok(()),
            Err(SolveError::StageFailed {
                stage: "edge pairing",
                reason: "middle-edge orientation mask is unreachable from the solved state",
            }) => {
                context.apply_moves(cube, middle_edge_parity_fix_moves(cube.side_len()));
            }
            Err(error) => return Err(error),
        }
    }

    Err(SolveError::StageFailed {
        stage: "edge pairing",
        reason: "middle-edge solving did not converge",
    })
}

fn solve_middle_slots<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    orbit: &mut MiddleOrbitTable,
) -> SolveResult<()> {
    for _attempt in 0..4 {
        let current = orbit.current_slot_keys_from_cube(cube);
        if build_unique_assignment_by_key(&current, &orbit.target_slot_index)
            .is_some_and(|assignment| permutation_is_identity(&assignment))
        {
            return Ok(());
        }

        let assignment = build_unique_assignment_by_key(&current, &orbit.target_slot_index).ok_or(
            SolveError::StageFailed {
                stage: "edge pairing",
                reason: "middle-edge slot state could not be mapped to solved targets",
            },
        )?;

        if !permutation_is_even(&assignment) {
            context.apply_moves(cube, middle_edge_parity_fix_moves(cube.side_len()));
            continue;
        }

        let cycles =
            ordered_three_cycles_for_assignment(&assignment).ok_or(SolveError::StageFailed {
                stage: "edge pairing",
                reason: "could not decompose the middle-edge assignment into exact three-cycles",
            })?;

        for cycle in cycles {
            let plan = orbit.plan_for_cycle(cycle).ok_or(SolveError::StageFailed {
                stage: "edge pairing",
                reason: "missing exact setup table entry for a middle-edge three-cycle",
            })?;
            context.apply_edge_three_cycle_plan(cube, plan);
        }

        if build_unique_assignment_by_key(
            &orbit.current_slot_keys_from_cube(cube),
            &orbit.target_slot_index,
        )
        .is_some_and(|assignment| permutation_is_identity(&assignment))
        {
            return Ok(());
        }
    }

    Err(SolveError::StageFailed {
        stage: "edge pairing",
        reason: "odd middle-edge slot solving did not converge to the solved edge targets",
    })
}

fn solve_wing_orientations<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    orbit: &WingOrbitTable,
    slot_setups: &EdgeSlotSetupTable,
) -> SolveResult<()> {
    let mask = wing_flip_mask_from_cube(orbit, cube).ok_or(SolveError::StageFailed {
        stage: "edge pairing",
        reason: "wing slot solving left an invalid slot orientation state",
    })?;
    if mask == 0 {
        return Ok(());
    }

    let solution = orbit
        .orientation_solution(mask)
        .ok_or(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "wing orientation mask is unreachable from the solved state",
        })?;

    for generator in solution {
        let setup = &slot_setups.to_destination[generator.setup_slot.index()];
        if !setup.is_empty() {
            context.apply_moves(cube, setup.iter().copied());
        }
        context.apply_moves(
            cube,
            wing_row_parity_fix_moves_with_map(cube.side_len(), orbit.row, generator.face_map),
        );

        if !setup.is_empty() {
            context.apply_moves(cube, inverted_moves(setup));
        }
    }

    if wing_flip_mask_from_cube(orbit, cube) == Some(0) {
        Ok(())
    } else {
        Err(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "wing orientation fixing did not converge",
        })
    }
}

fn solve_middle_orientations<S: FaceletArray>(
    cube: &mut Cube<S>,
    context: &mut SolveContext,
    orbit: &MiddleOrbitTable,
    slot_setups: &EdgeSlotSetupTable,
) -> SolveResult<()> {
    let mask = middle_flip_mask_from_cube(orbit, cube).ok_or(SolveError::StageFailed {
        stage: "edge pairing",
        reason: "middle-edge slot solving left an invalid orientation state",
    })?;
    if mask == 0 {
        return Ok(());
    }

    let solution = orbit
        .orientation_solution(mask)
        .ok_or(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "middle-edge orientation mask is unreachable from the solved state",
        })?;

    for generator in solution {
        let setup = &slot_setups.to_destination[generator.setup_slot.index()];
        if !setup.is_empty() {
            context.apply_moves(cube, setup.iter().copied());
        }
        let moves = match generator.kind {
            OrientationGeneratorKind::MiddlePrecheck => {
                middle_edge_precheck_moves_with_map(cube.side_len(), generator.face_map)
            }
            OrientationGeneratorKind::WingParityFix => {
                unreachable!("wing generator used in middle stage")
            }
        };
        context.apply_moves(cube, moves);

        if !setup.is_empty() {
            context.apply_moves(cube, inverted_moves(setup));
        }
    }

    if middle_flip_mask_from_cube(orbit, cube) == Some(0) {
        Ok(())
    } else {
        Err(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "middle-edge orientation fixing did not converge",
        })
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[allow(dead_code)]
fn wings_match_solved_slots(
    cache: &PreparedEdgeStage,
    view: &EdgeScanView,
    slot_keys: &[EdgeColorKey; 12],
) -> bool {
    cache
        .wing_orbits
        .iter()
        .all(|orbit| orbit.current_keys(view) == orbit.target_keys(slot_keys))
}

fn wings_match_solved_slots_from_cube<S: FaceletArray>(
    cache: &PreparedEdgeStage,
    cube: &Cube<S>,
    slot_keys: &[EdgeColorKey; 12],
) -> bool {
    cache
        .wing_orbits
        .iter()
        .all(|orbit| orbit.current_keys_from_cube(cube) == orbit.target_keys(slot_keys))
}

fn all_edge_facelets_solved<S: FaceletArray>(cube: &Cube<S>) -> bool {
    let side_length = cube.side_len();

    for face in FaceId::ALL {
        let target = home_facelet_for_face(face);
        for offset in 1..side_length.saturating_sub(1) {
            for (row, col) in [
                (0, offset),
                (side_length - 1, offset),
                (offset, 0),
                (offset, side_length - 1),
            ] {
                if cube.face(face).get(row, col) != target {
                    return false;
                }
            }
        }
    }

    true
}

fn build_even_assignment(current: &[EdgeColorKey], target: &[EdgeColorKey]) -> Option<Vec<usize>> {
    if current.len() != target.len() {
        return None;
    }

    let mut current_positions = BTreeMap::<EdgeColorKey, Vec<usize>>::new();
    let mut target_positions = BTreeMap::<EdgeColorKey, Vec<usize>>::new();

    for (index, key) in current.iter().copied().enumerate() {
        current_positions.entry(key).or_default().push(index);
    }
    for (index, key) in target.iter().copied().enumerate() {
        target_positions.entry(key).or_default().push(index);
    }

    if current_positions.len() != target_positions.len() {
        return None;
    }

    let mut assignment = vec![usize::MAX; current.len()];
    let mut parity_toggle_sources = None;
    let mut destinations_used = vec![false; current.len()];

    for (key, sources) in current_positions {
        let destinations = target_positions.get(&key)?;
        if sources.len() != destinations.len() {
            return None;
        }

        for (source, destination) in sources.iter().copied().zip(destinations.iter().copied()) {
            assignment[source] = destination;
            if destinations_used[destination] {
                return None;
            }
            destinations_used[destination] = true;
        }

        if sources.len() >= 2 {
            parity_toggle_sources = Some((sources[0], sources[1]));
        }
    }

    if assignment
        .iter()
        .any(|destination| *destination == usize::MAX)
    {
        return None;
    }
    if destinations_used.iter().any(|used| !used) {
        return None;
    }

    if !permutation_is_even(&assignment) {
        let (first, second) = parity_toggle_sources?;
        assignment.swap(first, second);
        if !permutation_is_even(&assignment) {
            return None;
        }
    }

    Some(assignment)
}

fn build_unique_assignment_by_key<K: Copy + Ord>(
    current: &[K],
    target_index: &BTreeMap<K, usize>,
) -> Option<Vec<usize>> {
    let mut assignment = vec![usize::MAX; current.len()];
    let mut used = vec![false; current.len()];

    for (source, key) in current.iter().copied().enumerate() {
        let destination = *target_index.get(&key)?;
        if used[destination] {
            return None;
        }
        used[destination] = true;
        assignment[source] = destination;
    }

    if used.iter().all(|entry| *entry) {
        Some(assignment)
    } else {
        None
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn count_keys(keys: &[EdgeColorKey]) -> BTreeMap<EdgeColorKey, usize> {
    let mut counts = BTreeMap::new();
    for key in keys {
        *counts.entry(*key).or_insert(0) += 1;
    }
    counts
}

fn permutation_is_even(permutation: &[usize]) -> bool {
    let mut visited = vec![false; permutation.len()];
    let mut parity = 0usize;

    for start in 0..permutation.len() {
        if visited[start] {
            continue;
        }

        let mut position = start;
        let mut length = 0usize;
        while !visited[position] {
            visited[position] = true;
            position = permutation[position];
            length += 1;
        }

        if length > 0 {
            parity ^= (length - 1) & 1;
        }
    }

    parity == 0
}

fn permutation_is_identity(permutation: &[usize]) -> bool {
    permutation
        .iter()
        .enumerate()
        .all(|(index, target)| index == *target)
}

fn ordered_three_cycles_for_assignment(assignment: &[usize]) -> Option<Vec<OrbitThreeCycleSpec>> {
    let mut piece_at_position = assignment.to_vec();
    let mut cycles = Vec::new();
    let position_count = assignment.len();

    while piece_at_position
        .iter()
        .enumerate()
        .any(|(position, piece)| position != *piece)
    {
        if let Some(cycle) = first_cycle_with_min_len(&piece_at_position, 3) {
            let triple = OrbitThreeCycleSpec::new(position_count, [cycle[0], cycle[1], cycle[2]]);
            apply_ordered_three_cycle(&mut piece_at_position, triple);
            cycles.push(triple);
            continue;
        }

        let two_cycles = collect_two_cycles(&piece_at_position);
        if two_cycles.is_empty() {
            break;
        }
        if two_cycles.len() < 2 || two_cycles.len() % 2 != 0 {
            return None;
        }

        let [a, b] = two_cycles[0];
        let [c, d] = two_cycles[1];
        let first = OrbitThreeCycleSpec::new(position_count, [a, c, b]);
        let second = OrbitThreeCycleSpec::new(position_count, [c, b, d]);
        apply_ordered_three_cycle(&mut piece_at_position, first);
        apply_ordered_three_cycle(&mut piece_at_position, second);
        cycles.push(first);
        cycles.push(second);
    }

    if piece_at_position
        .iter()
        .enumerate()
        .all(|(position, piece)| position == *piece)
    {
        Some(cycles)
    } else {
        None
    }
}

fn first_cycle_with_min_len(piece_at_position: &[usize], minimum: usize) -> Option<Vec<usize>> {
    let mut seen = vec![false; piece_at_position.len()];

    for start in 0..piece_at_position.len() {
        if seen[start] || piece_at_position[start] == start {
            continue;
        }

        let mut cycle = Vec::new();
        let mut position = start;
        while !seen[position] {
            seen[position] = true;
            cycle.push(position);
            position = piece_at_position[position];
        }

        if cycle.len() >= minimum {
            return Some(cycle);
        }
    }

    None
}

fn collect_two_cycles(piece_at_position: &[usize]) -> Vec<[usize; 2]> {
    let mut pairs = Vec::new();

    for first in 0..piece_at_position.len() {
        let second = piece_at_position[first];
        if first < second && second < piece_at_position.len() && piece_at_position[second] == first
        {
            pairs.push([first, second]);
        }
    }

    pairs
}

fn apply_ordered_three_cycle(piece_at_position: &mut [usize], cycle: OrbitThreeCycleSpec) {
    debug_assert_eq!(cycle.position_count, piece_at_position.len());

    let [first, second, third] = cycle.ordered_positions();
    let first_piece = piece_at_position[first];
    piece_at_position[first] = piece_at_position[third];
    piece_at_position[third] = piece_at_position[second];
    piece_at_position[second] = first_piece;
}

fn build_mask_solution_table(generator_masks: &[u16]) -> Vec<Option<MaskNode>> {
    let mut nodes = vec![None; 1usize << EdgeSlot::ALL.len()];
    nodes[0] = Some(MaskNode {
        prev: 0,
        generator_index: u8::MAX,
    });

    let mut queue = VecDeque::new();
    queue.push_back(0u16);

    while let Some(mask) = queue.pop_front() {
        for (generator_index, generator_mask) in generator_masks.iter().copied().enumerate() {
            let next = mask ^ generator_mask;
            let next_index = next as usize;
            if nodes[next_index].is_some() {
                continue;
            }
            nodes[next_index] = Some(MaskNode {
                prev: mask,
                generator_index: generator_index as u8,
            });
            queue.push_back(next);
        }
    }

    nodes
}

fn solved_edge_slot_keys() -> [EdgeColorKey; 12] {
    EdgeSlot::ALL.map(EdgeSlot::solved_key)
}

fn read_edge_key_from_view(
    view: &EdgeScanView,
    side_length: usize,
    cubie: EdgeCubieLocation,
) -> EdgeColorKey {
    let [first, second] = cubie.stickers();
    EdgeColorKey::from_facelets(view.get(first, side_length), view.get(second, side_length))
}

fn read_edge_key_from_cube<S: FaceletArray>(
    cube: &Cube<S>,
    cubie: EdgeCubieLocation,
) -> EdgeColorKey {
    let [first, second] = cubie.stickers();
    EdgeColorKey::from_facelets(
        cube.face(first.face).get(first.row, first.col),
        cube.face(second.face).get(second.row, second.col),
    )
}

fn read_oriented_edge_key_from_view(
    view: &EdgeScanView,
    side_length: usize,
    cubie: EdgeCubieLocation,
) -> OrientedEdgeKey {
    let [first, second] = cubie.stickers();
    OrientedEdgeKey::from_facelets(view.get(first, side_length), view.get(second, side_length))
}

fn read_oriented_edge_key_from_cube<S: FaceletArray>(
    cube: &Cube<S>,
    cubie: EdgeCubieLocation,
) -> OrientedEdgeKey {
    let [first, second] = cubie.stickers();
    OrientedEdgeKey::from_facelets(
        cube.face(first.face).get(first.row, first.col),
        cube.face(second.face).get(second.row, second.col),
    )
}

fn home_oriented_edge_key(cubie: EdgeCubieLocation) -> OrientedEdgeKey {
    let [first, second] = cubie.stickers();
    OrientedEdgeKey::from_face_ids(first.face, second.face)
}

fn edge_transfer_orientation(
    plan: &EdgeThreeCyclePlan,
    source: EdgeCubieLocation,
    destination: EdgeCubieLocation,
) -> Option<bool> {
    let [source_first, source_second] = source.stickers();
    let [destination_first, destination_second] = destination.stickers();
    let mut mapped_first = None;
    let mut mapped_second = None;

    for update in plan.updates() {
        if update.from == source_first {
            mapped_first = Some(update.to);
        } else if update.from == source_second {
            mapped_second = Some(update.to);
        }
    }

    match (mapped_first?, mapped_second?) {
        (first, second) if first == destination_first && second == destination_second => {
            Some(false)
        }
        (first, second) if first == destination_second && second == destination_first => Some(true),
        _ => None,
    }
}

fn edge_transfer_updates(
    source: EdgeCubieLocation,
    destination: EdgeCubieLocation,
    reversed: bool,
) -> [FaceletUpdate; 2] {
    let [source_first, source_second] = source.stickers();
    let [destination_first, destination_second] = destination.stickers();
    let (to_first, to_second) = if reversed {
        (destination_second, destination_first)
    } else {
        (destination_first, destination_second)
    };

    [
        FaceletUpdate {
            from: source_first,
            to: to_first,
        },
        FaceletUpdate {
            from: source_second,
            to: to_second,
        },
    ]
}

fn wing_cycle_literal_moves(side_length: usize, row: usize) -> Vec<Move> {
    let mirror = side_length - 1 - row;
    let mut moves = Vec::with_capacity(18);
    moves.push(face_layer_move(
        side_length,
        FaceId::D,
        row,
        MoveAngle::Positive,
    ));
    moves.push(face_layer_move(
        side_length,
        FaceId::D,
        mirror,
        MoveAngle::Positive,
    ));
    moves.extend(flip_right_edge_moves_with_map(
        side_length,
        FaceMap::identity(),
    ));
    moves.push(face_layer_move(
        side_length,
        FaceId::D,
        row,
        MoveAngle::Negative,
    ));
    moves.extend(unflip_right_edge_moves_with_map(
        side_length,
        FaceMap::identity(),
    ));
    moves.push(face_layer_move(
        side_length,
        FaceId::D,
        mirror,
        MoveAngle::Negative,
    ));
    moves
}

fn fixed_edge_position_from_coord(
    coord: geometry::Coord3,
    first: FaceId,
    second: FaceId,
) -> FixedEdgePosition {
    FixedEdgePosition {
        coord,
        faces: normalize_face_pair(first, second),
    }
}

fn coord3(x: usize, y: usize, z: usize) -> geometry::Coord3 {
    geometry::Coord3 { x, y, z }
}

fn wing_slot_positions() -> [[usize; 2]; 12] {
    std::array::from_fn(|slot_index| [slot_index * 2, slot_index * 2 + 1])
}

fn wing_orbit_positions(side_length: usize, row: usize) -> Vec<FixedEdgePosition> {
    assert!(
        side_length >= 4,
        "wing orbit positions require side length at least four",
    );
    assert!(row > 0 && row + 1 < side_length, "wing row must be inner");
    if side_length % 2 == 1 {
        assert!(
            row != side_length / 2,
            "wing row cannot be the odd middle layer"
        );
    }

    let last = side_length - 1;
    let mirror = last - row;

    vec![
        fixed_edge_position_from_coord(coord3(row, last, last), FaceId::U, FaceId::F),
        fixed_edge_position_from_coord(coord3(mirror, last, last), FaceId::U, FaceId::F),
        fixed_edge_position_from_coord(coord3(last, last, row), FaceId::U, FaceId::R),
        fixed_edge_position_from_coord(coord3(last, last, mirror), FaceId::U, FaceId::R),
        fixed_edge_position_from_coord(coord3(row, last, 0), FaceId::U, FaceId::B),
        fixed_edge_position_from_coord(coord3(mirror, last, 0), FaceId::U, FaceId::B),
        fixed_edge_position_from_coord(coord3(0, last, row), FaceId::U, FaceId::L),
        fixed_edge_position_from_coord(coord3(0, last, mirror), FaceId::U, FaceId::L),
        fixed_edge_position_from_coord(coord3(last, row, last), FaceId::F, FaceId::R),
        fixed_edge_position_from_coord(coord3(last, mirror, last), FaceId::F, FaceId::R),
        fixed_edge_position_from_coord(coord3(0, row, last), FaceId::F, FaceId::L),
        fixed_edge_position_from_coord(coord3(0, mirror, last), FaceId::F, FaceId::L),
        fixed_edge_position_from_coord(coord3(last, row, 0), FaceId::B, FaceId::R),
        fixed_edge_position_from_coord(coord3(last, mirror, 0), FaceId::B, FaceId::R),
        fixed_edge_position_from_coord(coord3(0, row, 0), FaceId::B, FaceId::L),
        fixed_edge_position_from_coord(coord3(0, mirror, 0), FaceId::B, FaceId::L),
        fixed_edge_position_from_coord(coord3(row, 0, last), FaceId::D, FaceId::F),
        fixed_edge_position_from_coord(coord3(mirror, 0, last), FaceId::D, FaceId::F),
        fixed_edge_position_from_coord(coord3(last, 0, row), FaceId::D, FaceId::R),
        fixed_edge_position_from_coord(coord3(last, 0, mirror), FaceId::D, FaceId::R),
        fixed_edge_position_from_coord(coord3(row, 0, 0), FaceId::D, FaceId::B),
        fixed_edge_position_from_coord(coord3(mirror, 0, 0), FaceId::D, FaceId::B),
        fixed_edge_position_from_coord(coord3(0, 0, row), FaceId::D, FaceId::L),
        fixed_edge_position_from_coord(coord3(0, 0, mirror), FaceId::D, FaceId::L),
    ]
}

fn middle_orbit_positions(side_length: usize) -> Vec<FixedEdgePosition> {
    assert!(
        side_length >= 3 && side_length % 2 == 1,
        "middle orbit positions require an odd side length of at least three",
    );

    let middle = side_length / 2;
    let last = side_length - 1;

    vec![
        fixed_edge_position_from_coord(coord3(middle, last, last), FaceId::U, FaceId::F),
        fixed_edge_position_from_coord(coord3(last, last, middle), FaceId::U, FaceId::R),
        fixed_edge_position_from_coord(coord3(middle, last, 0), FaceId::U, FaceId::B),
        fixed_edge_position_from_coord(coord3(0, last, middle), FaceId::U, FaceId::L),
        fixed_edge_position_from_coord(coord3(last, middle, last), FaceId::F, FaceId::R),
        fixed_edge_position_from_coord(coord3(0, middle, last), FaceId::F, FaceId::L),
        fixed_edge_position_from_coord(coord3(last, middle, 0), FaceId::B, FaceId::R),
        fixed_edge_position_from_coord(coord3(0, middle, 0), FaceId::B, FaceId::L),
        fixed_edge_position_from_coord(coord3(middle, 0, last), FaceId::D, FaceId::F),
        fixed_edge_position_from_coord(coord3(last, 0, middle), FaceId::D, FaceId::R),
        fixed_edge_position_from_coord(coord3(middle, 0, 0), FaceId::D, FaceId::B),
        fixed_edge_position_from_coord(coord3(0, 0, middle), FaceId::D, FaceId::L),
    ]
}

fn enumerate_edge_orbit_positions(side_length: usize, orbit: usize) -> Vec<FixedEdgePosition> {
    let mut positions = Vec::new();

    for face in FaceId::ALL {
        for offset in 1..side_length.saturating_sub(1) {
            for (row, col) in [
                (0, offset),
                (side_length - 1, offset),
                (offset, 0),
                (offset, side_length - 1),
            ] {
                let location = FaceletLocation { face, row, col };
                let Some(cubie) = edge_cubie_for_facelet_location(side_length, location) else {
                    continue;
                };
                if edge_cubie_orbit_index(side_length, cubie) != Some(orbit) {
                    continue;
                }
                let position = fixed_edge_position(side_length, cubie);
                if !positions.contains(&position) {
                    positions.push(position);
                }
            }
        }
    }

    positions.sort_by_key(edge_position_key);
    positions
}

fn build_middle_slot_positions(positions: &[FixedEdgePosition]) -> [usize; 12] {
    let mut slot_positions = [usize::MAX; 12];

    for (index, position) in positions.iter().copied().enumerate() {
        let slot = slot_of_position(position);
        let entry = &mut slot_positions[slot.index()];
        assert_eq!(
            *entry,
            usize::MAX,
            "middle orbit slot must contain exactly one position",
        );
        *entry = index;
    }

    assert!(
        slot_positions.iter().all(|index| *index != usize::MAX),
        "middle orbit must contain one position for every edge slot",
    );
    slot_positions
}

fn build_slot_representatives(positions: &[FixedEdgePosition]) -> [FixedEdgePosition; 12] {
    let mut slot_positions = [None; 12];

    for position in positions.iter().copied() {
        let slot = slot_of_position(position);
        slot_positions[slot.index()].get_or_insert(position);
    }

    slot_positions.map(|position| position.expect("every edge slot must have a representative"))
}

fn slot_of_position(position: FixedEdgePosition) -> EdgeSlot {
    EdgeSlot::from_faces(position.faces.0, position.faces.1)
}

fn edge_position_key(position: &FixedEdgePosition) -> ((usize, usize, usize), (usize, usize)) {
    (
        (position.coord.x, position.coord.y, position.coord.z),
        (position.faces.0.index(), position.faces.1.index()),
    )
}

fn orbit_setup_moves(side_length: usize, row: usize) -> Vec<Move> {
    let mirror = side_length - 1 - row;
    let mut moves = Vec::new();
    let mut depths = vec![0usize, row];
    if mirror != row {
        depths.push(mirror);
    }

    for face in FaceId::ALL {
        for &depth in &depths {
            for angle in MoveAngle::ALL {
                moves.push(face_layer_move(side_length, face, depth, angle));
            }
        }
    }

    moves
}

fn middle_setup_moves(side_length: usize) -> Vec<Move> {
    let mut moves = Vec::new();

    for face in FaceId::ALL {
        for angle in MoveAngle::ALL {
            moves.push(face_outer_move(side_length, face, angle));
        }
    }

    moves
}

fn orbit_move_transitions(
    side_length: usize,
    positions: &[FixedEdgePosition],
    setup_moves: &[Move],
) -> Vec<[u8; EDGE_WING_POSITION_COUNT]> {
    move_transitions_for_positions::<EDGE_WING_POSITION_COUNT>(
        side_length,
        positions,
        setup_moves,
        "setup move must preserve orbit",
    )
}

fn middle_move_transitions(
    side_length: usize,
    positions: &[FixedEdgePosition],
    setup_moves: &[Move],
) -> Vec<[u8; EDGE_MIDDLE_POSITION_COUNT]> {
    move_transitions_for_positions::<EDGE_MIDDLE_POSITION_COUNT>(
        side_length,
        positions,
        setup_moves,
        "middle setup move must preserve orbit",
    )
}

fn move_transitions_for_positions<const POSITION_COUNT: usize>(
    side_length: usize,
    positions: &[FixedEdgePosition],
    setup_moves: &[Move],
    context: &'static str,
) -> Vec<[u8; POSITION_COUNT]> {
    assert_eq!(
        positions.len(),
        POSITION_COUNT,
        "orbit transition builder position count mismatch",
    );

    setup_moves
        .iter()
        .copied()
        .map(|mv| {
            let mut next = [0u8; POSITION_COUNT];
            for (index, position) in positions.iter().copied().enumerate() {
                let cubie = cubie_from_fixed_position(side_length, position);
                let traced = fixed_edge_position(
                    side_length,
                    trace_edge_cubie_through_move(side_length, cubie, mv),
                );
                next[index] = position_index(positions, traced).unwrap_or_else(|| {
                    panic!(
                        "{context}: n={side_length}, mv={mv}, position={position:?}, traced={traced:?}",
                    )
                }) as u8;
            }
            next
        })
        .collect()
}

fn build_setup_table<const POSITION_COUNT: usize>(
    position_count: usize,
    start_triple: [usize; 3],
    transitions: &[[u8; POSITION_COUNT]],
) -> (Vec<Option<SetupNode>>, usize, usize) {
    let mut nodes = vec![None; position_count * position_count * position_count];
    let start_key = encode_triple_with_base(position_count, start_triple);
    nodes[start_key] = Some(SetupNode {
        prev: start_key as u16,
        move_index: u8::MAX,
    });

    let mut queue = VecDeque::new();
    queue.push_back(start_triple.map(|index| index as u8));
    let mut visited = 1usize;

    while let Some(state) = queue.pop_front() {
        let state_key = encode_triple_with_base(position_count, state.map(usize::from));
        for (move_index, transition) in transitions.iter().enumerate() {
            let next = [
                transition[state[0] as usize],
                transition[state[1] as usize],
                transition[state[2] as usize],
            ];
            let next_key = encode_triple_with_base(position_count, next.map(usize::from));
            if nodes[next_key].is_some() {
                continue;
            }

            nodes[next_key] = Some(SetupNode {
                prev: state_key as u16,
                move_index: move_index as u8,
            });
            visited += 1;
            queue.push_back(next);
        }
    }

    (nodes, start_key, visited)
}

fn encode_triple(triple: [usize; 3]) -> usize {
    triple[0] * EDGE_WING_POSITION_COUNT * EDGE_WING_POSITION_COUNT
        + triple[1] * EDGE_WING_POSITION_COUNT
        + triple[2]
}

fn encode_triple_with_base(base: usize, triple: [usize; 3]) -> usize {
    triple[0] * base * base + triple[1] * base + triple[2]
}

fn decode_triple_with_base(base: usize, value: usize) -> [usize; 3] {
    [value / (base * base), (value / base) % base, value % base]
}

fn same_cyclic_order(left: [usize; 3], right: [usize; 3]) -> bool {
    left == right
        || left == [right[1], right[2], right[0]]
        || left == [right[2], right[0], right[1]]
}

fn position_index(positions: &[FixedEdgePosition], target: FixedEdgePosition) -> Option<usize> {
    positions.iter().position(|position| *position == target)
}

fn plan_cubie_positions_in_orbit(
    side_length: usize,
    positions: &[FixedEdgePosition],
    cubies: &[EdgeCubieLocation; 3],
) -> Option<[usize; 3]> {
    let mut mapped = [0usize; 3];
    for (index, cubie) in cubies.iter().copied().enumerate() {
        mapped[index] = position_index(positions, fixed_edge_position(side_length, cubie))?;
    }
    Some(mapped)
}

fn inverted_moves(moves: &[Move]) -> Vec<Move> {
    moves.iter().rev().copied().map(Move::inverse).collect()
}

fn fixed_edge_position(side_length: usize, cubie: EdgeCubieLocation) -> FixedEdgePosition {
    let [first, second] = cubie.stickers();
    let coord = geometry::logical_to_coord(first.face, first.row, first.col, side_length);
    debug_assert_eq!(
        coord,
        geometry::logical_to_coord(second.face, second.row, second.col, side_length),
        "edge cubie stickers must share a single 3D coordinate",
    );

    FixedEdgePosition {
        coord,
        faces: normalize_face_pair(first.face, second.face),
    }
}

fn cubie_from_fixed_position(side_length: usize, position: FixedEdgePosition) -> EdgeCubieLocation {
    let (face, _) = position.faces;
    let (row, col) = geometry::coord_to_logical(face, position.coord, side_length);
    let location = FaceletLocation { face, row, col };
    edge_cubie_for_facelet_location(side_length, location)
        .expect("fixed edge position must decode to a valid edge cubie")
}

fn build_slot_setup_paths(
    side_length: usize,
    slot_positions: &[FixedEdgePosition; 12],
    target: EdgeSlot,
) -> [Vec<Move>; 12] {
    let setup_moves = middle_setup_moves(side_length);
    let mut previous = [None::<(usize, usize)>; 12];
    let mut seen = [false; 12];
    let mut queue = VecDeque::new();

    seen[target.index()] = true;
    queue.push_back(target.index());

    while let Some(current) = queue.pop_front() {
        let current_position = slot_positions[current];
        let current_cubie = cubie_from_fixed_position(side_length, current_position);

        for (move_index, mv) in setup_moves.iter().copied().enumerate() {
            let traced = fixed_edge_position(
                side_length,
                trace_edge_cubie_through_move(side_length, current_cubie, mv),
            );
            let next_slot = slot_of_position(traced).index();
            if seen[next_slot] {
                continue;
            }

            seen[next_slot] = true;
            previous[next_slot] = Some((current, move_index));
            queue.push_back(next_slot);
        }
    }

    assert!(
        seen.iter().all(|reached| *reached),
        "outer-face setup moves must reach every edge slot",
    );

    std::array::from_fn(|slot_index| {
        if slot_index == target.index() {
            return Vec::new();
        }

        let mut path = Vec::new();
        let mut current = slot_index;
        while current != target.index() {
            let (prev_slot, move_index) =
                previous[current].expect("every slot must have a BFS predecessor");
            path.push(setup_moves[move_index]);
            current = prev_slot;
        }
        inverted_moves(&path)
    })
}

fn slot_orientation_state_for_position(
    actual: OrientedEdgeKey,
    target: OrientedEdgeKey,
) -> SlotOrientationState {
    if actual == target {
        SlotOrientationState::Solved
    } else if actual == reverse_oriented_edge_key(target) {
        SlotOrientationState::Flipped
    } else {
        SlotOrientationState::Invalid
    }
}

fn wing_slot_orientation_state(
    orbit: &WingOrbitTable,
    view: &EdgeScanView,
    slot: EdgeSlot,
) -> SlotOrientationState {
    let [first_target, second_target] = orbit.slot_positions[slot.index()].map(|position_index| {
        slot_position_target_key(orbit.side_length, orbit.positions[position_index])
    });
    wing_slot_orientation_state_for_target(
        wing_slot_pair_for_slot(orbit, view, slot),
        [first_target, second_target],
    )
}

fn wing_slot_pair_for_slot(
    orbit: &WingOrbitTable,
    view: &EdgeScanView,
    slot: EdgeSlot,
) -> [OrientedEdgeKey; 2] {
    orbit.slot_positions[slot.index()].map(|position_index| {
        read_oriented_edge_key_from_view(
            view,
            orbit.side_length,
            cubie_from_fixed_position(orbit.side_length, orbit.positions[position_index]),
        )
    })
}

fn wing_slot_pair_for_slot_from_cube<S: FaceletArray>(
    orbit: &WingOrbitTable,
    cube: &Cube<S>,
    slot: EdgeSlot,
) -> [OrientedEdgeKey; 2] {
    orbit.slot_positions[slot.index()].map(|position_index| {
        read_oriented_edge_key_from_cube(
            cube,
            cubie_from_fixed_position(orbit.side_length, orbit.positions[position_index]),
        )
    })
}

fn wing_slot_orientation_state_for_target(
    actual: [OrientedEdgeKey; 2],
    target: [OrientedEdgeKey; 2],
) -> SlotOrientationState {
    match (
        slot_orientation_state_for_position(actual[0], target[0]),
        slot_orientation_state_for_position(actual[1], target[1]),
    ) {
        (SlotOrientationState::Solved, SlotOrientationState::Solved) => {
            SlotOrientationState::Solved
        }
        (SlotOrientationState::Flipped, SlotOrientationState::Flipped) => {
            SlotOrientationState::Flipped
        }
        _ => SlotOrientationState::Invalid,
    }
}

fn middle_slot_orientation_state(
    orbit: &MiddleOrbitTable,
    view: &EdgeScanView,
    slot: EdgeSlot,
) -> SlotOrientationState {
    let position = orbit.positions[orbit.slot_positions[slot.index()]];
    let actual = read_oriented_edge_key_from_view(
        view,
        orbit.side_length,
        cubie_from_fixed_position(orbit.side_length, position),
    );
    let target = slot_position_target_key(orbit.side_length, position);
    slot_orientation_state_for_position(actual, target)
}

fn wing_slot_orientation_state_from_cube<S: FaceletArray>(
    orbit: &WingOrbitTable,
    cube: &Cube<S>,
    slot: EdgeSlot,
) -> SlotOrientationState {
    let [first_target, second_target] = orbit.slot_positions[slot.index()].map(|position_index| {
        slot_position_target_key(orbit.side_length, orbit.positions[position_index])
    });
    wing_slot_orientation_state_for_target(
        wing_slot_pair_for_slot_from_cube(orbit, cube, slot),
        [first_target, second_target],
    )
}

fn middle_slot_orientation_state_from_cube<S: FaceletArray>(
    orbit: &MiddleOrbitTable,
    cube: &Cube<S>,
    slot: EdgeSlot,
) -> SlotOrientationState {
    let position = orbit.positions[orbit.slot_positions[slot.index()]];
    let actual = read_oriented_edge_key_from_cube(
        cube,
        cubie_from_fixed_position(orbit.side_length, position),
    );
    let target = slot_position_target_key(orbit.side_length, position);
    slot_orientation_state_for_position(actual, target)
}

fn middle_flip_mask(orbit: &MiddleOrbitTable, view: &EdgeScanView) -> Option<u16> {
    let mut mask = 0u16;

    for slot in EdgeSlot::ALL {
        match middle_slot_orientation_state(orbit, view, slot) {
            SlotOrientationState::Solved => {}
            SlotOrientationState::Flipped => mask |= 1u16 << slot.index(),
            SlotOrientationState::Invalid => return None,
        }
    }

    Some(mask)
}

fn middle_flip_mask_from_cube<S: FaceletArray>(
    orbit: &MiddleOrbitTable,
    cube: &Cube<S>,
) -> Option<u16> {
    let mut mask = 0u16;

    for slot in EdgeSlot::ALL {
        match middle_slot_orientation_state_from_cube(orbit, cube, slot) {
            SlotOrientationState::Solved => {}
            SlotOrientationState::Flipped => mask |= 1u16 << slot.index(),
            SlotOrientationState::Invalid => return None,
        }
    }

    Some(mask)
}

fn wing_flip_mask(orbit: &WingOrbitTable, view: &EdgeScanView) -> Option<u16> {
    let mut mask = 0u16;

    for slot in EdgeSlot::ALL {
        match wing_slot_orientation_state(orbit, view, slot) {
            SlotOrientationState::Solved => {}
            SlotOrientationState::Flipped => mask |= 1u16 << slot.index(),
            SlotOrientationState::Invalid => return None,
        }
    }

    Some(mask)
}

fn wing_flip_mask_from_cube<S: FaceletArray>(
    orbit: &WingOrbitTable,
    cube: &Cube<S>,
) -> Option<u16> {
    let mut mask = 0u16;

    for slot in EdgeSlot::ALL {
        match wing_slot_orientation_state_from_cube(orbit, cube, slot) {
            SlotOrientationState::Solved => {}
            SlotOrientationState::Flipped => mask |= 1u16 << slot.index(),
            SlotOrientationState::Invalid => return None,
        }
    }

    Some(mask)
}

fn mapped_face_layer_move(
    side_length: usize,
    face_map: FaceMap,
    face: FaceId,
    depth_from_face: usize,
    angle: MoveAngle,
) -> Move {
    face_layer_move(side_length, face_map.apply(face), depth_from_face, angle)
}

fn flip_right_edge_moves_with_map(side_length: usize, face_map: FaceMap) -> [Move; 7] {
    [
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Positive),
        mapped_face_layer_move(side_length, face_map, FaceId::U, 0, MoveAngle::Positive),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::F, 0, MoveAngle::Positive),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::F, 0, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Positive),
    ]
}

fn unflip_right_edge_moves_with_map(side_length: usize, face_map: FaceMap) -> [Move; 7] {
    [
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::F, 0, MoveAngle::Positive),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Positive),
        mapped_face_layer_move(side_length, face_map, FaceId::F, 0, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Positive),
        mapped_face_layer_move(side_length, face_map, FaceId::U, 0, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Negative),
    ]
}

fn middle_edge_precheck_moves_with_map(side_length: usize, face_map: FaceMap) -> Vec<Move> {
    let middle = side_length / 2;
    let mut moves = Vec::with_capacity(16);
    moves.push(mapped_face_layer_move(
        side_length,
        face_map,
        FaceId::D,
        middle,
        MoveAngle::Positive,
    ));
    moves.extend(flip_right_edge_moves_with_map(side_length, face_map));
    moves.push(mapped_face_layer_move(
        side_length,
        face_map,
        FaceId::D,
        middle,
        MoveAngle::Negative,
    ));
    moves.extend(unflip_right_edge_moves_with_map(side_length, face_map));
    moves
}

#[cfg(test)]
fn middle_edge_precheck_moves(side_length: usize) -> Vec<Move> {
    middle_edge_precheck_moves_with_map(side_length, FaceMap::identity())
}

fn wing_row_parity_fix_moves_with_map(
    side_length: usize,
    row: usize,
    face_map: FaceMap,
) -> Vec<Move> {
    vec![
        mapped_face_layer_move(side_length, face_map, FaceId::D, row, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::U, row, MoveAngle::Positive),
        mapped_face_layer_move(side_length, face_map, FaceId::F, 0, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::U, row, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::F, 0, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::D, row, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::D, row, MoveAngle::Positive),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::D, row, MoveAngle::Negative),
        mapped_face_layer_move(side_length, face_map, FaceId::R, 0, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::F, 0, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::D, row, MoveAngle::Double),
        mapped_face_layer_move(side_length, face_map, FaceId::F, 0, MoveAngle::Double),
    ]
}

#[cfg(test)]
fn wing_row_parity_fix_moves(side_length: usize, row: usize) -> Vec<Move> {
    wing_row_parity_fix_moves_with_map(side_length, row, FaceMap::identity())
}

fn middle_edge_parity_fix_moves_with_map(side_length: usize, face_map: FaceMap) -> Vec<Move> {
    wing_row_parity_fix_moves_with_map(side_length, side_length / 2, face_map)
}

fn middle_edge_parity_fix_moves(side_length: usize) -> Vec<Move> {
    middle_edge_parity_fix_moves_with_map(side_length, FaceMap::identity())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cube::trace_facelet_location_through_moves, Byte, Byte3, Nibble, ThreeBit, XorShift64,
    };

    #[test]
    fn wing_orbit_setup_tables_cover_all_ordered_triples() {
        for side_length in 4..=8 {
            let setup_template = WingOrbitSetupTemplate::new(side_length);
            for row in 1..=(side_length - 2) / 2 {
                let orbit = WingOrbitTable::new(side_length, row, &setup_template);
                assert_eq!(
                    orbit.reachable_ordered_triples, EDGE_WING_TRIPLE_COUNT,
                    "missing setup table entries for n={side_length}, row={row}",
                );
            }
        }
    }

    #[test]
    fn middle_orbit_setup_tables_cover_all_ordered_triples() {
        for side_length in [3usize, 5, 7, 9] {
            let slot_setups = EdgeSlotSetupTable::new(side_length);
            let orbit = MiddleOrbitTable::new(side_length, &slot_setups);
            assert_eq!(
                orbit.reachable_ordered_triples, EDGE_MIDDLE_TRIPLE_COUNT,
                "missing middle setup table entries for n={side_length}",
            );
        }
    }

    #[test]
    fn even_assignment_decomposition_sorts_all_even_permutations_of_six() {
        let mut values = [0usize, 1, 2, 3, 4, 5];
        loop {
            if permutation_is_even(&values) {
                let cycles = ordered_three_cycles_for_assignment(&values)
                    .expect("every even permutation must decompose into three-cycles");
                let mut state = values.to_vec();
                for cycle in cycles {
                    apply_ordered_three_cycle(&mut state, cycle);
                }
                assert_eq!(state, [0, 1, 2, 3, 4, 5]);
            }

            if !next_permutation(&mut values) {
                break;
            }
        }
    }

    #[test]
    fn orbit_three_cycle_spec_validates_positions() {
        assert!(OrbitThreeCycleSpec::try_new(24, [0, 1, 2]).is_some());
        assert!(OrbitThreeCycleSpec::try_new(24, [0, 0, 2]).is_none());
        assert!(OrbitThreeCycleSpec::try_new(24, [0, 1, 24]).is_none());
    }

    #[test]
    fn edge_stage_replays_to_the_same_full_cube_state() {
        for side_length in 4..=8 {
            for seed in [0xED63_AAA1u64, 0xE442_9A7Eu64] {
                let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                let mut rng = XorShift64::new(seed ^ side_length as u64);
                cube.scramble_random_moves(&mut rng, 120);
                let initial = cube.clone();

                let mut stage = EdgePairingStage::default();
                let mut context = SolveContext::new(super::super::SolveOptions {
                    thread_count: 1,
                    record_moves: true,
                });

                <EdgePairingStage as SolverStage<Byte>>::run(&mut stage, &mut cube, &mut context)
                    .unwrap_or_else(|error| {
                        panic!(
                            "edge stage failed for replay test n={side_length}, seed={seed:#x}: {error}\n{}",
                            cube.net_string(),
                        )
                    });

                let mut replay = initial;
                replay.apply_moves_untracked_with_threads(context.moves().iter().copied(), 1);

                assert_cubes_match(&cube, &replay);
                assert!(stage_wings_match_solved_slots(&cube));
            }
        }
    }

    #[test]
    fn edge_stage_pairs_random_scrambles_for_all_storage_backends() {
        run_edge_stage_for_storage::<Byte>(6, 0xE000_0001);
        run_edge_stage_for_storage::<Byte3>(6, 0xE000_0002);
        run_edge_stage_for_storage::<Nibble>(6, 0xE000_0003);
        run_edge_stage_for_storage::<ThreeBit>(6, 0xE000_0004);
    }

    #[test]
    fn edge_stage_pairs_random_scrambles_for_sizes_three_to_five() {
        for side_length in 3..=5 {
            for seed in [
                0xE005_0001u64,
                0xE005_0002u64,
                0xE005_0003u64,
                0xE005_0004u64,
            ] {
                run_edge_stage_for_storage::<Byte>(side_length, seed ^ side_length as u64);
            }
        }
    }

    #[test]
    fn edge_stage_solves_wings_to_home_slots_on_odd_cubes() {
        for side_length in [5usize, 7] {
            run_edge_stage_for_storage::<Byte>(side_length, 0xE000_1000 ^ side_length as u64);
        }
    }

    #[test]
    fn wing_setup_transitions_are_identical_for_all_rows() {
        for side_length in 4..=8 {
            let setup_template = WingOrbitSetupTemplate::new(side_length);
            let reference_positions = wing_orbit_positions(side_length, 1);
            let reference_moves = orbit_setup_moves(side_length, 1);
            let reference_transitions =
                orbit_move_transitions(side_length, &reference_positions, &reference_moves);
            let reference_plan =
                EdgeThreeCyclePlan::from_cycle(side_length, EdgeThreeCycle::front_right_wing(1));
            let reference_key = encode_triple(reference_plan.cubies().map(|cubie| {
                position_index(
                    &reference_positions,
                    fixed_edge_position(side_length, cubie),
                )
                .expect("representative base cubie must stay in the reference orbit")
            }));
            assert_eq!(reference_key, setup_template.start_key);

            for row in 1..=(side_length - 2) / 2 {
                let positions = wing_orbit_positions(side_length, row);
                let moves = orbit_setup_moves(side_length, row);
                let transitions = orbit_move_transitions(side_length, &positions, &moves);
                let plan = EdgeThreeCyclePlan::from_cycle(
                    side_length,
                    EdgeThreeCycle::front_right_wing(row),
                );
                let key = encode_triple(plan.cubies().map(|cubie| {
                    position_index(&positions, fixed_edge_position(side_length, cubie))
                        .expect("row-specific base cubie must stay in the orbit")
                }));

                assert_eq!(
                    transitions, reference_transitions,
                    "wing setup transitions differ for n={side_length}, row={row}",
                );
                assert_eq!(
                    key, setup_template.start_key,
                    "shared wing setup start key differs for n={side_length}, row={row}",
                );
            }
        }
    }

    #[test]
    fn wing_orbit_oriented_keys_can_leave_the_home_key_multiset_after_single_moves() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1, &WingOrbitSetupTemplate::new(side_length));
        let target = orbit
            .positions
            .iter()
            .copied()
            .map(|position| {
                home_oriented_edge_key(cubie_from_fixed_position(side_length, position))
            })
            .collect::<Vec<_>>();
        let target_counts = count_oriented_keys(&target);

        for axis in [crate::Axis::X, crate::Axis::Y, crate::Axis::Z] {
            for depth in 0..side_length {
                for angle in MoveAngle::ALL {
                    let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                    cube.apply_move_untracked(crate::Move::new(axis, depth, angle));
                    let view = EdgeScanView::from_cube(&cube);
                    let current = orbit
                        .positions
                        .iter()
                        .copied()
                        .map(|position| {
                            read_oriented_edge_key_from_view(
                                &view,
                                side_length,
                                cubie_from_fixed_position(side_length, position),
                            )
                        })
                        .collect::<Vec<_>>();
                    if count_oriented_keys(&current) != target_counts {
                        return;
                    }
                }
            }
        }

        panic!("expected some single move to leave the wing orbit home-key multiset");
    }

    #[test]
    fn wing_slot_orientation_states_after_unordered_solve_are_never_invalid() {
        for side_length in [4usize, 6] {
            let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
            let mut rng = XorShift64::new(0xA15E_0000 ^ side_length as u64);
            cube.scramble_random_moves(&mut rng, 120);

            let mut cache = PreparedEdgeStage::new(side_length);
            let mut context = SolveContext::new(super::super::SolveOptions {
                thread_count: 1,
                record_moves: false,
            });
            let slot_keys = solved_edge_slot_keys();

            for orbit in &mut cache.wing_orbits {
                solve_wing_orbit(&mut cube, &mut context, orbit, &slot_keys).unwrap();
                let view = EdgeScanView::from_cube(&cube);
                for slot in EdgeSlot::ALL {
                    assert_ne!(
                        wing_slot_orientation_state(orbit, &view, slot),
                        SlotOrientationState::Invalid,
                        "n={side_length}, row={}, slot={slot:?}",
                        orbit.row,
                    );
                }
            }
        }
    }

    #[test]
    fn wing_parity_fix_flips_only_front_right_slot() {
        let side_length = 6;
        let row = 1;
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_moves_untracked_with_threads(wing_row_parity_fix_moves(side_length, row), 1);
        let orbit =
            WingOrbitTable::new(side_length, row, &WingOrbitSetupTemplate::new(side_length));
        let view = EdgeScanView::from_cube(&cube);
        for slot in EdgeSlot::ALL {
            let expected = if slot == EdgeSlot::FR {
                SlotOrientationState::Flipped
            } else {
                SlotOrientationState::Solved
            };
            assert_eq!(wing_slot_orientation_state(&orbit, &view, slot), expected);
        }
    }

    #[test]
    fn middle_precheck_flip_toggles_front_right_and_front_left_slots() {
        let side_length = 5;
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_moves_untracked_with_threads(middle_edge_precheck_moves(side_length), 1);
        let slot_setups = EdgeSlotSetupTable::new(side_length);
        let orbit = MiddleOrbitTable::new(side_length, &slot_setups);
        let view = EdgeScanView::from_cube(&cube);
        for slot in EdgeSlot::ALL {
            let expected = if matches!(slot, EdgeSlot::FR | EdgeSlot::FL) {
                SlotOrientationState::Flipped
            } else {
                SlotOrientationState::Solved
            };
            assert_eq!(middle_slot_orientation_state(&orbit, &view, slot), expected);
        }
    }

    #[test]
    fn wing_orientation_generators_span_all_masks() {
        let side_length = 4;
        let cache = PreparedEdgeStage::new(side_length);
        let orbit = &cache.wing_orbits[0];
        let reachable = orbit
            .orientation_nodes
            .iter()
            .filter(|entry| entry.is_some())
            .count();
        assert_eq!(reachable, 1usize << EdgeSlot::ALL.len());
    }

    #[test]
    fn middle_orientation_generators_span_even_mask_space() {
        let side_length = 5;
        let cache = PreparedEdgeStage::new(side_length);
        let orbit = cache.middle_orbit.as_ref().unwrap();
        let reachable = orbit
            .orientation_nodes
            .iter()
            .filter(|entry| entry.is_some())
            .count();
        assert_eq!(reachable, 1usize << (EdgeSlot::ALL.len() - 1));
    }

    #[test]
    fn wing_orientation_cache_is_identical_for_all_rows() {
        for side_length in 4..=8 {
            let slot_setups = EdgeSlotSetupTable::new(side_length);
            let setup_template = WingOrbitSetupTemplate::new(side_length);
            let mut rows = (1..=(side_length - 2) / 2)
                .map(|row| WingOrbitTable::new(side_length, row, &setup_template))
                .collect::<Vec<_>>();

            if rows.len() < 2 {
                continue;
            }

            let first_cache = rows[0].build_orientation_cache(&slot_setups);
            for orbit in rows.iter_mut().skip(1) {
                let cache = orbit.build_orientation_cache(&slot_setups);
                assert_eq!(
                    cache.orientation_masks, first_cache.orientation_masks,
                    "wing orientation masks differ for n={side_length}, row={}",
                    orbit.row
                );
                assert_eq!(
                    cache.orientation_generators.len(),
                    first_cache.orientation_generators.len(),
                    "wing orientation generator count differs for n={side_length}, row={}",
                    orbit.row
                );
                assert_eq!(
                    cache.orientation_nodes, first_cache.orientation_nodes,
                    "wing orientation solution table differs for n={side_length}, row={}",
                    orbit.row
                );
            }
        }
    }

    #[test]
    fn edge_stage_solves_middle_edges_after_a_direct_middle_cycle() {
        let side_length = 5;
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_edge_three_cycle_untracked(EdgeThreeCycle::front_right_middle(
            crate::cube::EdgeThreeCycleDirection::Positive,
        ));

        let mut stage = EdgePairingStage::default();
        let mut context = SolveContext::new(super::super::SolveOptions {
            thread_count: 1,
            record_moves: false,
        });

        <EdgePairingStage as SolverStage<Byte>>::run(&mut stage, &mut cube, &mut context)
            .expect("edge stage should solve the direct middle-edge scramble");

        assert!(all_edge_facelets_solved(&cube));
    }

    #[test]
    fn generated_wing_setup_plans_match_literal_moves_on_full_cube_state() {
        for side_length in 4..=5 {
            let setup_template = WingOrbitSetupTemplate::new(side_length);
            for row in 1..=(side_length - 2) / 2 {
                let mut orbit = WingOrbitTable::new(side_length, row, &setup_template);
                let position_count = orbit.positions.len();

                for first in 0..position_count {
                    for second in 0..position_count {
                        if second == first {
                            continue;
                        }
                        for third in 0..position_count {
                            if third == first || third == second {
                                continue;
                            }

                            let cycle =
                                OrbitThreeCycleSpec::new(position_count, [first, second, third]);
                            let plan = orbit
                                .plan_for_cycle(cycle)
                                .expect("every ordered wing triple must have a plan")
                                .clone();

                            let mut expected =
                                patterned_cube::<Byte>(side_length, 41 + row * 7 + side_length);
                            expected.apply_moves_untracked_with_threads(
                                plan.moves().iter().copied(),
                                1,
                            );

                            let mut actual =
                                patterned_cube::<Byte>(side_length, 41 + row * 7 + side_length);
                            actual.apply_edge_three_cycle_plan_untracked(&plan);

                            assert_cubes_match(&actual, &expected);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn generated_middle_setup_plans_match_literal_moves_on_full_cube_state() {
        for side_length in [3usize, 5] {
            let slot_setups = EdgeSlotSetupTable::new(side_length);
            let mut orbit = MiddleOrbitTable::new(side_length, &slot_setups);
            let position_count = orbit.positions.len();

            for first in 0..position_count {
                for second in 0..position_count {
                    if second == first {
                        continue;
                    }
                    for third in 0..position_count {
                        if third == first || third == second {
                            continue;
                        }

                        let cycle =
                            OrbitThreeCycleSpec::new(position_count, [first, second, third]);
                        let plan = orbit
                            .plan_for_cycle(cycle)
                            .expect("every ordered middle triple must have a plan")
                            .clone();

                        let mut expected = patterned_cube::<Byte>(side_length, 53 + side_length);
                        expected
                            .apply_moves_untracked_with_threads(plan.moves().iter().copied(), 1);

                        let mut actual = patterned_cube::<Byte>(side_length, 53 + side_length);
                        actual.apply_edge_three_cycle_plan_untracked(&plan);

                        assert_cubes_match(&actual, &expected);
                    }
                }
            }
        }
    }

    #[test]
    fn wing_orbit_scan_preserves_legal_color_multiset_after_single_moves() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1, &WingOrbitSetupTemplate::new(side_length));
        let target = orbit.target_keys(&EdgeSlot::ALL.map(EdgeSlot::solved_key));

        for axis in [crate::Axis::X, crate::Axis::Y, crate::Axis::Z] {
            for depth in 0..side_length {
                for angle in MoveAngle::ALL {
                    let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                    cube.apply_move_untracked(crate::Move::new(axis, depth, angle));
                    let current = orbit.current_keys(&EdgeScanView::from_cube(&cube));
                    assert_eq!(
                        count_keys(&current),
                        count_keys(&target),
                        "single-move scan mismatch for axis={axis:?}, depth={depth}, angle={angle}",
                    );
                }
            }
        }
    }

    #[test]
    fn wing_orbit_scan_preserves_legal_color_multiset_after_two_move_sequences() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1, &WingOrbitSetupTemplate::new(side_length));
        let target = orbit.target_keys(&EdgeSlot::ALL.map(EdgeSlot::solved_key));
        let moves = [crate::Axis::X, crate::Axis::Y, crate::Axis::Z]
            .into_iter()
            .flat_map(|axis| {
                (0..side_length).flat_map(move |depth| {
                    MoveAngle::ALL
                        .into_iter()
                        .map(move |angle| crate::Move::new(axis, depth, angle))
                })
            })
            .collect::<Vec<_>>();

        for first in moves.iter().copied() {
            for second in moves.iter().copied() {
                let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
                cube.apply_move_untracked(first);
                cube.apply_move_untracked(second);
                let current = orbit.current_keys(&EdgeScanView::from_cube(&cube));
                assert_eq!(
                    count_keys(&current),
                    count_keys(&target),
                    "two-move scan mismatch for first={first}, second={second}",
                );
            }
        }
    }

    #[test]
    fn two_move_sequence_scan_matches_trace_model() {
        let side_length = 4;
        let moves = [
            crate::Move::new(crate::Axis::X, 0, MoveAngle::Positive),
            crate::Move::new(crate::Axis::Y, 1, MoveAngle::Positive),
        ];
        let inverse = inverted_moves(&moves);
        let orbit = WingOrbitTable::new(side_length, 1, &WingOrbitSetupTemplate::new(side_length));
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_move_untracked(moves[0]);
        cube.apply_move_untracked(moves[1]);
        let view = EdgeScanView::from_cube(&cube);

        for position in orbit.positions.iter().copied() {
            let cubie = cubie_from_fixed_position(side_length, position);
            let [first, second] = cubie.stickers();
            let traced_first = trace_facelet_location_through_moves(side_length, first, &inverse);
            let traced_second = trace_facelet_location_through_moves(side_length, second, &inverse);
            let expected = EdgeColorKey::from_facelets(
                Facelet::from_u8(traced_first.face.index() as u8),
                Facelet::from_u8(traced_second.face.index() as u8),
            );
            let actual = read_edge_key_from_view(&view, side_length, cubie);
            assert_eq!(
                actual, expected,
                "trace-model mismatch for position={position:?}",
            );
        }
    }

    #[test]
    fn materialized_two_move_sequence_still_has_a_legal_edge_multiset() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1, &WingOrbitSetupTemplate::new(side_length));
        let target = orbit.target_keys(&EdgeSlot::ALL.map(EdgeSlot::solved_key));
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_move_untracked(crate::Move::new(crate::Axis::X, 0, MoveAngle::Positive));
        cube.apply_move_untracked(crate::Move::new(crate::Axis::Y, 1, MoveAngle::Positive));
        materialize_face_rotations(&mut cube);

        let current = orbit.current_keys(&EdgeScanView::from_cube(&cube));
        assert_eq!(count_keys(&current), count_keys(&target));
    }

    #[test]
    fn materialized_outer_then_outer_sequence_still_has_a_legal_edge_multiset() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1, &WingOrbitSetupTemplate::new(side_length));
        let target = orbit.target_keys(&EdgeSlot::ALL.map(EdgeSlot::solved_key));
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_move_untracked(crate::Move::new(crate::Axis::X, 0, MoveAngle::Positive));
        cube.apply_move_untracked(crate::Move::new(crate::Axis::Y, 0, MoveAngle::Positive));
        materialize_face_rotations(&mut cube);

        let current = orbit.current_keys(&EdgeScanView::from_cube(&cube));
        assert_eq!(count_keys(&current), count_keys(&target));
    }

    #[test]
    fn direct_static_cubie_reads_after_two_moves_are_legal_or_not() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1, &WingOrbitSetupTemplate::new(side_length));
        let target = orbit.target_keys(&EdgeSlot::ALL.map(EdgeSlot::solved_key));
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_move_untracked(crate::Move::new(crate::Axis::X, 0, MoveAngle::Positive));
        cube.apply_move_untracked(crate::Move::new(crate::Axis::Y, 0, MoveAngle::Positive));

        let current = orbit
            .positions
            .iter()
            .copied()
            .map(|position| {
                let cubie = cubie_from_fixed_position(side_length, position);
                let [first, second] = cubie.stickers();
                EdgeColorKey::from_facelets(
                    cube.face(first.face).get(first.row, first.col),
                    cube.face(second.face).get(second.row, second.col),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(count_keys(&current), count_keys(&target));
    }

    fn run_edge_stage_for_storage<S: FaceletArray + 'static>(side_length: usize, seed: u64) {
        let mut cube = Cube::<S>::new_solved_with_threads(side_length, 1);
        let mut rng = XorShift64::new(seed ^ side_length as u64);
        cube.scramble_random_moves(&mut rng, 120);

        let mut stage = EdgePairingStage::default();
        let mut context = SolveContext::new(super::super::SolveOptions {
            thread_count: 1,
            record_moves: false,
        });

        <EdgePairingStage as SolverStage<S>>::run(&mut stage, &mut cube, &mut context)
            .unwrap_or_else(|error| panic!("edge stage failed for n={side_length}: {error}"));

        assert!(stage_wings_match_solved_slots(&cube));
        assert!(all_edge_facelets_solved(&cube));
    }

    fn stage_wings_match_solved_slots<S: FaceletArray>(cube: &Cube<S>) -> bool {
        let slot_keys = solved_edge_slot_keys();
        let cache = PreparedEdgeStage::new(cube.side_len());
        wings_match_solved_slots_from_cube(&cache, cube, &slot_keys)
    }

    fn count_oriented_keys(keys: &[OrientedEdgeKey]) -> BTreeMap<OrientedEdgeKey, usize> {
        let mut counts = BTreeMap::new();
        for key in keys {
            *counts.entry(*key).or_insert(0) += 1;
        }
        counts
    }

    fn assert_cubes_match<S: FaceletArray>(left: &Cube<S>, right: &Cube<S>) {
        assert_eq!(
            left.side_len(),
            right.side_len(),
            "cube side lengths differ"
        );
        for face in FaceId::ALL {
            assert_eq!(
                left.face(face).rotation(),
                right.face(face).rotation(),
                "face rotation metadata differs for {face}",
            );
            for row in 0..left.side_len() {
                for col in 0..left.side_len() {
                    assert_eq!(
                        left.face(face).get(row, col),
                        right.face(face).get(row, col),
                        "cube states differ at {face}({row},{col})",
                    );
                }
            }
        }
    }

    fn patterned_cube<S: FaceletArray>(side_length: usize, seed: usize) -> Cube<S> {
        let mut cube = Cube::<S>::new_solved_with_threads(side_length, 1);

        for face in FaceId::ALL {
            for row in 0..side_length {
                for col in 0..side_length {
                    let color_index =
                        (seed + face.index() * 5 + row * 3 + col * 2) % Facelet::ALL.len();
                    cube.face_mut(face).set(row, col, Facelet::ALL[color_index]);
                }
            }
        }

        cube
    }

    fn next_permutation(values: &mut [usize]) -> bool {
        if values.len() < 2 {
            return false;
        }

        let Some(pivot) = (0..values.len() - 1).rfind(|&index| values[index] < values[index + 1])
        else {
            return false;
        };
        let swap_index = (pivot + 1..values.len())
            .rfind(|&index| values[pivot] < values[index])
            .expect("pivot must have a larger suffix element");
        values.swap(pivot, swap_index);
        values[pivot + 1..].reverse();
        true
    }

    fn materialize_face_rotations<S: FaceletArray>(cube: &mut Cube<S>) {
        let side_length = cube.side_len();

        for face in FaceId::ALL {
            let mut values = Vec::with_capacity(side_length * side_length);
            for row in 0..side_length {
                for col in 0..side_length {
                    values.push(cube.face(face).get(row, col));
                }
            }
            let face_ref = cube.face_mut(face);
            for row in 0..side_length {
                for col in 0..side_length {
                    face_ref
                        .matrix_mut()
                        .set(row, col, values[row * side_length + col]);
                }
            }
            face_ref.set_rotation(crate::FaceAngle::new(0));
        }
    }
}
