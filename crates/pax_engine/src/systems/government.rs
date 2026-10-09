//! Government transfers (DECISIONS.md D15).
//!
//! Each day a nation pays `treasury × transfer_rate` (rounded down) to its POPs,
//! split by size with the largest-remainder method: a flat per-capita transfer
//! (think poor relief or pensions). Without some spending, a taxing treasury
//! would steadily drain money out of circulation, and with it prices and wages
//! (MACROECONOMICS.md §1). Purchases of goods arrive with M2-2.
//!
//! The treasury is debited exactly what its POPs are credited, so total money is
//! unchanged. A nation with no living POPs keeps its money.

use crate::fixed::Fixed;
use crate::layout::PopLayout;
use crate::world::World;

/// Pays every nation's daily transfer. Returns the total paid (diagnostics).
pub fn pay_transfers(world: &mut World, layout: &PopLayout) -> Fixed {
    let mut total = Fixed::ZERO;
    for n in 0..world.nations.len() {
        let amount = world.nations.treasury[n].mul(world.nations.transfer_rate[n]);
        if !amount.is_positive() {
            continue;
        }
        if world.credit_pops_by_size(layout.nation.members(n), amount) {
            world.nations.treasury[n] -= amount;
            total += amount;
        }
    }
    total
}
