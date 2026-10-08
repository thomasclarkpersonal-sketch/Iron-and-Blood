//! Read-only aggregates per province, for presentation: the server's map views
//! (D22). They are **derived data, never state** (D7). They are computed on demand
//! from the POP table and a day's labour report, and never stored in the `World`.
//!
//! They live in the engine so that the weighting and counting rules are the engine's
//! own:
//! * militancy and life needs use the summaries the tick reports
//!   ([`MilitancySummary`], [`LifeNeedsSummary`]);
//! * unemployment uses [`labor::unemployment`], which counts only professions some
//!   producer type employs.
//!
//! A map can then never disagree with `pax_cli report` about what a number means.

use crate::fixed::Fixed;
use crate::systems::labor::{self, LabourReport};
use crate::systems::market::{LifeNeedsSummary, MilitancySummary};
use crate::world::World;

/// Per-province aggregates, indexed by province id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProvinceStats {
    /// People living in the province now (end of day).
    pub population: Vec<u64>,
    /// Life-needs coverage of the province's POPs, weighted by their current sizes.
    pub life_needs: Vec<LifeNeedsSummary>,
    /// Militancy of the province's POPs, weighted by their current sizes (D19).
    pub militancy: Vec<MilitancySummary>,
    /// `(unemployed, workforce)` by [`labor::unemployment`]'s definition, from the
    /// labour report passed in; zeros if none was passed.
    pub unemployment: Vec<(u64, u64)>,
}

impl ProvinceStats {
    /// Aggregates the POP table, and `labour` (a day's `DayReport::labour`) if
    /// given. O(POP rows + labour pools).
    pub fn of(world: &World, labour: Option<&[LabourReport]>) -> ProvinceStats {
        let provinces = world.geography.province_count();
        let mut s = ProvinceStats {
            population: vec![0; provinces],
            life_needs: vec![LifeNeedsSummary::default(); provinces],
            militancy: vec![MilitancySummary::default(); provinces],
            unemployment: vec![(0, 0); provinces],
        };
        let p = &world.pops;
        for i in 0..p.size.len() {
            let n = p.size[i];
            if n == 0 {
                continue;
            }
            let province = p.province[i] as usize;
            s.population[province] += n as u64;
            let life = &mut s.life_needs[province];
            life.people += n as u64;
            life.weighted_raw += n as i128 * p.life_needs[i].raw() as i128;
            if p.life_needs[i] < Fixed::ONE {
                life.deprived += n as u64;
            }
            let militancy = &mut s.militancy[province];
            militancy.people += n as u64;
            militancy.weighted_raw += n as i128 * p.militancy[i].raw() as i128;
        }
        if let Some(labour) = labour {
            // One call per province, so the definition of "unemployed" stays labor's.
            let mut by_province: Vec<Vec<LabourReport>> = vec![Vec::new(); provinces];
            for pool in labour {
                by_province[pool.province as usize].push(*pool);
            }
            for (province, pools) in by_province.iter().enumerate() {
                s.unemployment[province] = labor::unemployment(&world.defs, pools);
            }
        }
        s
    }

    /// `unemployed / workforce` in a province, or zero if it has no workforce.
    pub fn unemployment_rate(&self, province: usize) -> Fixed {
        let (unemployed, workforce) = self.unemployment[province];
        if workforce == 0 { Fixed::ZERO } else { Fixed::ratio(unemployed as i64, workforce as i64) }
    }
}
