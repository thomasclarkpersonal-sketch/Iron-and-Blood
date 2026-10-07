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

use crate::alloc::allocate_raw;
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
        let rows = layout.nation.members(n);
        let sizes: Vec<i64> = rows.iter().map(|&r| world.pops.size[r as usize] as i64).collect();
        let Some(shares) = allocate_raw(amount.raw(), &sizes) else { continue };
        world.nations.treasury[n] -= amount;
        for (&r, share) in rows.iter().zip(shares) {
            world.pops.cash[r as usize] += Fixed::from_raw(share);
        }
        total += amount;
    }
    total
}
