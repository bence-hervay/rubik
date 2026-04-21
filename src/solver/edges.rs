use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::{
    cube::{
        edge_cubie_for_facelet_location, edge_cubie_orbit_index, trace_edge_cubie_through_move,
        Cube, EdgeCubieLocation, EdgeThreeCycle, EdgeThreeCyclePlan, FaceletLocation,
    },
    face::FaceId,
    facelet::Facelet,
    geometry,
    moves::{Move, MoveAngle},
    storage::FaceletArray,
};

use super::{
    SolveContext, SolveError, SolvePhase, SolveResult, SolverStage, SubStageSpec,
};

const EDGE_WING_POSITION_COUNT: usize = 24;
const EDGE_TRIPLE_STATE_COUNT: usize =
    EDGE_WING_POSITION_COUNT * EDGE_WING_POSITION_COUNT * EDGE_WING_POSITION_COUNT;
#[cfg(test)]
const EDGE_WING_TRIPLE_COUNT: usize = 24 * 23 * 22;

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
    sub_stages: [SubStageSpec; 3],
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
        if cube.side_len() < 4 {
            return Ok(());
        }

        let slot_keys = solved_edge_slot_keys();
        let cache = self.ensure_cache(cube.side_len());

        for orbit in &mut cache.wing_orbits {
            solve_wing_orbit(cube, context, orbit, &slot_keys)?;
        }

        let view = EdgeScanView::from_cube(cube);
        if wings_match_solved_slots(cache, &view, &slot_keys) {
            Ok(())
        } else {
            Err(SolveError::StageFailed {
                stage: "edge pairing",
                reason: "wing solving left a home edge-slot orbit unsolved",
            })
        }
    }
}

#[derive(Debug)]
struct PreparedEdgeStage {
    side_length: usize,
    wing_orbits: Vec<WingOrbitTable>,
}

impl PreparedEdgeStage {
    fn new(side_length: usize) -> Self {
        let mut wing_orbits = Vec::new();
        if side_length >= 4 {
            for row in 1..=(side_length - 2) / 2 {
                wing_orbits.push(WingOrbitTable::new(side_length, row));
            }
        }

        Self {
            side_length,
            wing_orbits,
        }
    }
}

#[derive(Debug)]
struct WingOrbitTable {
    side_length: usize,
    positions: Vec<FixedEdgePosition>,
    slot_positions: [[usize; 2]; 12],
    setup_moves: Vec<Move>,
    setup_nodes: Vec<Option<SetupNode>>,
    start_key: usize,
    base_moves: Vec<Move>,
    inverse_base_moves: Vec<Move>,
    plan_cache: HashMap<usize, EdgeThreeCyclePlan>,
    #[cfg_attr(not(test), allow(dead_code))]
    reachable_ordered_triples: usize,
}

impl WingOrbitTable {
    fn new(side_length: usize, row: usize) -> Self {
        let positions = enumerate_edge_orbit_positions(side_length, row);
        assert_eq!(
            positions.len(),
            EDGE_WING_POSITION_COUNT,
            "wing orbit must contain 24 positions",
        );

        let slot_positions = build_slot_positions(&positions);
        let base_plan = EdgeThreeCyclePlan::from_cycle(side_length, EdgeThreeCycle::front_right_wing(row));
        let base_triple = base_plan
            .cubies()
            .map(|cubie| {
                position_index(&positions, fixed_edge_position(side_length, cubie))
                    .expect("base cubie must be in orbit")
            });
        let setup_moves = orbit_setup_moves(side_length, row);
        let transitions = orbit_move_transitions(side_length, &positions, &setup_moves);
        let (setup_nodes, start_key, reachable_ordered_triples) =
            build_setup_table(base_triple, &transitions);
        let base_moves = base_plan.moves().to_vec();
        let inverse_base_moves = inverted_moves(&base_moves);

        Self {
            side_length,
            positions,
            slot_positions,
            setup_moves,
            setup_nodes,
            start_key,
            base_moves,
            inverse_base_moves,
            plan_cache: HashMap::new(),
            reachable_ordered_triples,
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

    fn current_keys(&self, view: &EdgeScanView) -> Vec<EdgeColorKey> {
        self.positions
            .iter()
            .copied()
            .map(|position| read_edge_key_from_view(view, self.side_length, cubie_from_fixed_position(self.side_length, position)))
            .collect()
    }

    fn plan_for_cycle(&mut self, ordered_positions: [usize; 3]) -> Option<&EdgeThreeCyclePlan> {
        let direct_key = encode_triple(ordered_positions);
        if self.setup_nodes.get(direct_key).and_then(|node| *node).is_some() {
            return self.plan_for_encoded_cycle(direct_key, false);
        }

        let reverse_key = encode_triple([
            ordered_positions[0],
            ordered_positions[2],
            ordered_positions[1],
        ]);
        if self.setup_nodes.get(reverse_key).and_then(|node| *node).is_some() {
            return self.plan_for_encoded_cycle(reverse_key, true);
        }

        None
    }

    fn plan_for_encoded_cycle(
        &mut self,
        setup_key: usize,
        use_inverse_base: bool,
    ) -> Option<&EdgeThreeCyclePlan> {
        let cache_key = setup_key * 2 + usize::from(use_inverse_base);
        if !self.plan_cache.contains_key(&cache_key) {
            let setup_moves = self.reconstruct_setup_moves(setup_key)?;
            let base_moves = if use_inverse_base {
                &self.inverse_base_moves
            } else {
                &self.base_moves
            };

            let mut moves = Vec::with_capacity(setup_moves.len() * 2 + base_moves.len());
            moves.extend(setup_moves.iter().rev().copied().map(Move::inverse));
            moves.extend(base_moves.iter().copied());
            moves.extend(setup_moves.iter().copied());

            let plan = EdgeThreeCyclePlan::try_from_moves(self.side_length, moves)?;
            let actual = plan
                .cubies()
                .map(|cubie| {
                    position_index(&self.positions, fixed_edge_position(self.side_length, cubie))
                        .expect("cached cubie must stay in orbit")
                });
            let expected = decode_triple(setup_key);
            if !same_cyclic_order(actual, expected) {
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

#[derive(Copy, Clone, Debug)]
struct SetupNode {
    prev: u16,
    move_index: u8,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
struct EdgeColorKey {
    first: u8,
    second: u8,
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
    let current = orbit.current_keys(&EdgeScanView::from_cube(cube));
    if current == target {
        return Ok(());
    }

    let assignment = build_even_assignment(&current, &target).ok_or(SolveError::StageFailed {
        stage: "edge pairing",
        reason: "could not assign reduced edge targets for a wing orbit",
    })?;
    let cycles = ordered_three_cycles_for_assignment(&assignment).ok_or(SolveError::StageFailed {
        stage: "edge pairing",
        reason: "could not decompose a wing orbit assignment into exact three-cycles",
    })?;

    for cycle in cycles {
        let plan = orbit.plan_for_cycle(cycle).ok_or(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "missing exact setup table entry for a wing orbit three-cycle",
        })?;
        context.apply_edge_three_cycle_plan(cube, plan);
    }

    if orbit.current_keys(&EdgeScanView::from_cube(cube)) == target {
        Ok(())
    } else {
        Err(SolveError::StageFailed {
            stage: "edge pairing",
            reason: "wing orbit pairing did not reach its reduced edge target",
        })
    }
}

fn wings_match_solved_slots(
    cache: &PreparedEdgeStage,
    view: &EdgeScanView,
    slot_keys: &[EdgeColorKey; 12],
) -> bool {
    cache.wing_orbits
        .iter()
        .all(|orbit| orbit.current_keys(view) == orbit.target_keys(slot_keys))
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

    if assignment.iter().any(|destination| *destination == usize::MAX) {
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

fn ordered_three_cycles_for_assignment(assignment: &[usize]) -> Option<Vec<[usize; 3]>> {
    let mut piece_at_position = assignment.to_vec();
    let mut cycles = Vec::new();

    while piece_at_position
        .iter()
        .enumerate()
        .any(|(position, piece)| position != *piece)
    {
        if let Some(cycle) = first_cycle_with_min_len(&piece_at_position, 3) {
            let triple = [cycle[0], cycle[1], cycle[2]];
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
        let first = [a, c, b];
        let second = [c, b, d];
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
        if first < second
            && second < piece_at_position.len()
            && piece_at_position[second] == first
        {
            pairs.push([first, second]);
        }
    }

    pairs
}

fn apply_ordered_three_cycle(piece_at_position: &mut [usize], cycle: [usize; 3]) {
    let [first, second, third] = cycle;
    let first_piece = piece_at_position[first];
    piece_at_position[first] = piece_at_position[third];
    piece_at_position[third] = piece_at_position[second];
    piece_at_position[second] = first_piece;
}

fn solved_edge_slot_keys() -> [EdgeColorKey; 12] {
    EdgeSlot::ALL.map(EdgeSlot::solved_key)
}

#[cfg(test)]
fn home_facelet_for_face(face: FaceId) -> Facelet {
    Facelet::from_u8(face.index() as u8)
}

fn read_edge_key_from_view(
    view: &EdgeScanView,
    side_length: usize,
    cubie: EdgeCubieLocation,
) -> EdgeColorKey {
    let [first, second] = cubie.stickers();
    EdgeColorKey::from_facelets(
        view.get(first, side_length),
        view.get(second, side_length),
    )
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

fn build_slot_positions(positions: &[FixedEdgePosition]) -> [[usize; 2]; 12] {
    let mut slot_positions = [[usize::MAX; 2]; 12];
    let mut counts = [0usize; 12];

    for (index, position) in positions.iter().copied().enumerate() {
        let slot = slot_of_position(position);
        let entry = &mut slot_positions[slot.index()];
        let count = &mut counts[slot.index()];
        assert!(*count < 2, "wing orbit slot must contain exactly two positions");
        entry[*count] = index;
        *count += 1;
    }

    assert_eq!(
        counts,
        [2; 12],
        "wing orbit must contain two positions for every edge slot",
    );
    slot_positions
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
                moves.push(super::face_layer_move(side_length, face, depth, angle));
            }
        }
    }

    moves
}

fn orbit_move_transitions(
    side_length: usize,
    positions: &[FixedEdgePosition],
    setup_moves: &[Move],
) -> Vec<[u8; EDGE_WING_POSITION_COUNT]> {
    setup_moves
        .iter()
        .copied()
        .map(|mv| {
            let mut next = [0u8; EDGE_WING_POSITION_COUNT];
            for (index, position) in positions.iter().copied().enumerate() {
                let cubie = cubie_from_fixed_position(side_length, position);
                let traced = fixed_edge_position(
                    side_length,
                    trace_edge_cubie_through_move(side_length, cubie, mv),
                );
                next[index] = position_index(positions, traced).unwrap_or_else(|| {
                    panic!(
                        "setup move must preserve orbit: n={side_length}, mv={mv}, position={position:?}, traced={traced:?}",
                    )
                }) as u8;
            }
            next
        })
        .collect()
}

fn build_setup_table(
    start_triple: [usize; 3],
    transitions: &[[u8; EDGE_WING_POSITION_COUNT]],
) -> (Vec<Option<SetupNode>>, usize, usize) {
    let mut nodes = vec![None; EDGE_TRIPLE_STATE_COUNT];
    let start_key = encode_triple(start_triple);
    nodes[start_key] = Some(SetupNode {
        prev: start_key as u16,
        move_index: u8::MAX,
    });

    let mut queue = VecDeque::new();
    queue.push_back(start_triple.map(|index| index as u8));
    let mut visited = 1usize;

    while let Some(state) = queue.pop_front() {
        let state_key = encode_triple(state.map(usize::from));
        for (move_index, transition) in transitions.iter().enumerate() {
            let next = [
                transition[state[0] as usize],
                transition[state[1] as usize],
                transition[state[2] as usize],
            ];
            let next_key = encode_triple(next.map(usize::from));
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

fn decode_triple(value: usize) -> [usize; 3] {
    [
        value / (EDGE_WING_POSITION_COUNT * EDGE_WING_POSITION_COUNT),
        (value / EDGE_WING_POSITION_COUNT) % EDGE_WING_POSITION_COUNT,
        value % EDGE_WING_POSITION_COUNT,
    ]
}

fn same_cyclic_order(left: [usize; 3], right: [usize; 3]) -> bool {
    left == right
        || left == [right[1], right[2], right[0]]
        || left == [right[2], right[0], right[1]]
}

fn position_index(positions: &[FixedEdgePosition], target: FixedEdgePosition) -> Option<usize> {
    positions.iter().position(|position| *position == target)
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

fn normalize_face_pair(first: FaceId, second: FaceId) -> (FaceId, FaceId) {
    if first.index() <= second.index() {
        (first, second)
    } else {
        (second, first)
    }
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
            for row in 1..=(side_length - 2) / 2 {
                let orbit = WingOrbitTable::new(side_length, row);
                assert_eq!(
                    orbit.reachable_ordered_triples,
                    EDGE_WING_TRIPLE_COUNT,
                    "missing setup table entries for n={side_length}, row={row}",
                );
            }
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
    fn edge_stage_solves_wings_to_home_slots_on_odd_cubes() {
        for side_length in [5usize, 7] {
            run_edge_stage_for_storage::<Byte>(side_length, 0xE000_1000 ^ side_length as u64);
        }
    }

    #[test]
    fn edge_stage_does_not_claim_to_solve_odd_middle_edges() {
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
            .expect("wing stage should still succeed");

        assert!(stage_wings_match_solved_slots(&cube));
        assert!(!middle_edges_match_home_slots(&cube));
    }

    #[test]
    fn wing_orbit_scan_preserves_legal_color_multiset_after_single_moves() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1);
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
        let orbit = WingOrbitTable::new(side_length, 1);
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
        let orbit = WingOrbitTable::new(side_length, 1);
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_move_untracked(moves[0]);
        cube.apply_move_untracked(moves[1]);
        let view = EdgeScanView::from_cube(&cube);

        for position in orbit.positions.iter().copied() {
            let cubie = cubie_from_fixed_position(side_length, position);
            let [first, second] = cubie.stickers();
            let traced_first =
                trace_facelet_location_through_moves(side_length, first, &inverse);
            let traced_second =
                trace_facelet_location_through_moves(side_length, second, &inverse);
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
        let orbit = WingOrbitTable::new(side_length, 1);
        let target = orbit.target_keys(&EdgeSlot::ALL.map(EdgeSlot::solved_key));
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_move_untracked(crate::Move::new(
            crate::Axis::X,
            0,
            MoveAngle::Positive,
        ));
        cube.apply_move_untracked(crate::Move::new(
            crate::Axis::Y,
            1,
            MoveAngle::Positive,
        ));
        materialize_face_rotations(&mut cube);

        let current = orbit.current_keys(&EdgeScanView::from_cube(&cube));
        assert_eq!(count_keys(&current), count_keys(&target));
    }

    #[test]
    fn materialized_outer_then_outer_sequence_still_has_a_legal_edge_multiset() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1);
        let target = orbit.target_keys(&EdgeSlot::ALL.map(EdgeSlot::solved_key));
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_move_untracked(crate::Move::new(
            crate::Axis::X,
            0,
            MoveAngle::Positive,
        ));
        cube.apply_move_untracked(crate::Move::new(
            crate::Axis::Y,
            0,
            MoveAngle::Positive,
        ));
        materialize_face_rotations(&mut cube);

        let current = orbit.current_keys(&EdgeScanView::from_cube(&cube));
        assert_eq!(count_keys(&current), count_keys(&target));
    }

    #[test]
    fn direct_static_cubie_reads_after_two_moves_are_legal_or_not() {
        let side_length = 4;
        let orbit = WingOrbitTable::new(side_length, 1);
        let target = orbit.target_keys(&EdgeSlot::ALL.map(EdgeSlot::solved_key));
        let mut cube = Cube::<Byte>::new_solved_with_threads(side_length, 1);
        cube.apply_move_untracked(crate::Move::new(
            crate::Axis::X,
            0,
            MoveAngle::Positive,
        ));
        cube.apply_move_untracked(crate::Move::new(
            crate::Axis::Y,
            0,
            MoveAngle::Positive,
        ));

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
    }

    fn stage_wings_match_solved_slots<S: FaceletArray>(cube: &Cube<S>) -> bool {
        let slot_keys = solved_edge_slot_keys();
        let cache = PreparedEdgeStage::new(cube.side_len());
        wings_match_solved_slots(&cache, &EdgeScanView::from_cube(cube), &slot_keys)
    }

    fn middle_edges_match_home_slots<S: FaceletArray>(cube: &Cube<S>) -> bool {
        if cube.side_len() < 3 || cube.side_len() % 2 == 0 {
            return true;
        }

        let side_length = cube.side_len();
        let middle = side_length / 2;
        let view = EdgeScanView::from_cube(cube);

        enumerate_edge_orbit_positions(side_length, middle)
            .into_iter()
            .all(|position| {
                let cubie = cubie_from_fixed_position(side_length, position);
                let [first, second] = cubie.stickers();
                view.get(first, side_length) == home_facelet_for_face(first.face)
                    && view.get(second, side_length) == home_facelet_for_face(second.face)
            })
    }

    fn assert_cubes_match<S: FaceletArray>(left: &Cube<S>, right: &Cube<S>) {
        assert_eq!(left.side_len(), right.side_len(), "cube side lengths differ");
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

    fn next_permutation(values: &mut [usize]) -> bool {
        if values.len() < 2 {
            return false;
        }

        let Some(pivot) = (0..values.len() - 1).rfind(|&index| values[index] < values[index + 1]) else {
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
                    face_ref.matrix_mut().set(row, col, values[row * side_length + col]);
                }
            }
            face_ref.set_rotation(crate::FaceAngle::new(0));
        }
    }
}
