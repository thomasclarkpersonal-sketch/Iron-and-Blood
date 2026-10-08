//! D22's size budgets for a `DayUpdate` at the D13 long-term scale (NETWORK_PROTOCOL §4):
//! at most 16 KiB with no views subscribed, at most 128 KiB with every view.

mod common;

use common::*;

#[test]
fn summary_only_update_fits_its_budget() {
    let frame = day_update(&Scale { map: false, market: false, province: false, ..LONG_TERM });
    assert!(frame.len() <= 16 * 1024, "summary-only DayUpdate is {} bytes", frame.len());
}

#[test]
fn update_with_every_view_fits_its_budget() {
    let frame = day_update(&LONG_TERM);
    assert!(frame.len() <= 128 * 1024, "full DayUpdate is {} bytes", frame.len());
}
