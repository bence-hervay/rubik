use std::time::Instant;

use super::core::{EdgeSlotSetupTable, MiddleOrbitTable, WingOrbitSetupTemplate, WingOrbitTable};

#[derive(Debug)]
pub(crate) struct PreparedEdgeTables {
    pub(crate) side_length: usize,
    pub(crate) wing_orbits: Vec<WingOrbitTable>,
    pub(crate) middle_orbit: Option<MiddleOrbitTable>,
    pub(crate) slot_setups: Option<EdgeSlotSetupTable>,
}

impl PreparedEdgeTables {
    pub(crate) fn new(side_length: usize) -> Self {
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
                    orbit.set_orientation_cache(cache.clone(), slot_setups);
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
