//! Read-only aggregates for presentation: the server's views (D22). They are
//! **derived data, never state** (D7). They are computed on demand from the POP table
//! and a day's labour report, and never stored in the `World`.
//!
//! The engine owns every rule here, so callers only serialise numbers:
//! * size weighting and the definition of *deprived* come from
//!   [`LifeNeedsSummary::record`] and [`MilitancySummary::record`], the same calls
//!   the market's own tally makes;
//! * unemployment comes from [`labor::unemployment`], which counts only professions
//!   some producer type employs;
//! * a POP's identity for display is `(province, profession)` (D7), applied by
//!   [`province_pops`].
//!
//! A map or a panel can then never disagree with the tick's `DayReport` or with
//! `pax_cli report` about what a number means.

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

/// Whole-world totals of a [`ProvinceStats`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldTotals {
    pub population: u64,
    pub life_needs: LifeNeedsSummary,
    pub militancy: MilitancySummary,
    pub unemployed: u64,
    pub workforce: u64,
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
            let province = p.province[i] as usize;
            s.population[province] += p.size[i] as u64;
            s.life_needs[province].record(p.size[i], p.life_needs[i]);
            s.militancy[province].record(p.size[i], p.militancy[i]);
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

    /// The whole world's figures: the provinces' summaries added up.
    pub fn totals(&self) -> WorldTotals {
        let mut t = WorldTotals::default();
        for p in 0..self.population.len() {
            t.population += self.population[p];
            t.life_needs = t.life_needs + self.life_needs[p];
            t.militancy = t.militancy + self.militancy[p];
            t.unemployed += self.unemployment[p].0;
            t.workforce += self.unemployment[p].1;
        }
        t
    }
}

/// The POPs of one profession in one province, merged. For display, a POP is its
/// identity `(province, profession)` (D7): rows that share it between month-end
/// compactions are shown as compaction will merge them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfessionGroup {
    pub profession: u16,
    pub people: u64,
    /// Total cash. Every subset of the world's money fits in `Fixed` (D5).
    pub cash: Fixed,
    pub life_needs: LifeNeedsSummary,
    pub militancy: MilitancySummary,
}

/// A province's POPs by profession, in profession order, omitting professions with
/// nobody. O(POP rows).
pub fn province_pops(world: &World, province: u32) -> Vec<ProfessionGroup> {
    let p = &world.pops;
    let mut groups: Vec<ProfessionGroup> = (0..world.defs.professions.len())
        .map(|c| ProfessionGroup {
            profession: c as u16,
            people: 0,
            cash: Fixed::ZERO,
            life_needs: LifeNeedsSummary::default(),
            militancy: MilitancySummary::default(),
        })
        .collect();
    for i in (0..p.size.len()).filter(|&i| p.province[i] == province && p.size[i] > 0) {
        let g = &mut groups[p.profession[i] as usize];
        g.people += p.size[i] as u64;
        g.cash += p.cash[i];
        g.life_needs.record(p.size[i], p.life_needs[i]);
        g.militancy.record(p.size[i], p.militancy[i]);
    }
    groups.retain(|g| g.people > 0);
    groups
}
