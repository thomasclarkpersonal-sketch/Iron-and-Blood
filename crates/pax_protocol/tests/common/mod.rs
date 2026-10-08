//! Message builders shared by the protocol tests.
#![allow(dead_code)]

use flatbuffers::{FlatBufferBuilder, Vector, WIPOffset};
use pax_protocol::wire::*;

/// Fixed raw values 0, step, 2·step, … so reads can be checked against their index.
pub fn fixeds<'a>(b: &mut FlatBufferBuilder<'a>, n: usize, step: i64) -> WIPOffset<Vector<'a, Fixed>> {
    let v: Vec<Fixed> = (0..n).map(|i| Fixed::new(i as i64 * step)).collect();
    b.create_vector(&v)
}

pub fn client_frame<'a>(
    b: &mut FlatBufferBuilder<'a>,
    payload_type: ClientPayload,
    payload: WIPOffset<flatbuffers::UnionWIPOffset>,
) -> Vec<u8> {
    let msg = ClientMessage::create(b, &ClientMessageArgs { payload_type, payload: Some(payload) });
    finish_size_prefixed_client_message_buffer(b, msg);
    b.finished_data().to_vec()
}

pub fn server_frame<'a>(
    b: &mut FlatBufferBuilder<'a>,
    payload_type: ServerPayload,
    payload: WIPOffset<flatbuffers::UnionWIPOffset>,
) -> Vec<u8> {
    let msg = ServerMessage::create(b, &ServerMessageArgs { payload_type, payload: Some(payload) });
    finish_size_prefixed_server_message_buffer(b, msg);
    b.finished_data().to_vec()
}

/// Which optional views a test `DayUpdate` carries, at what scale.
pub struct Scale {
    pub nations: usize,
    pub provinces: usize,
    pub goods: usize,
    pub pops: usize,
    pub map: bool,
    pub market: bool,
    pub province: bool,
}

/// D13's long-term target: ~3,000 markets, up to ~10,000 provinces, ~50 goods, and
/// a few hundred nations. A province shows a few dozen POP rows.
pub const LONG_TERM: Scale =
    Scale { nations: 200, provinces: 10_000, goods: 50, pops: 40, map: true, market: true, province: true };

pub fn day_update(s: &Scale) -> Vec<u8> {
    let mut b = FlatBufferBuilder::new();
    let world = WorldSummary::create(
        &mut b,
        &WorldSummaryArgs {
            population: 250_000_000,
            workforce: 120_000_000,
            unemployed: 6_000_000,
            household_spending: Some(&Fixed::new(1_234_567_890)),
            life_needs: Some(&Fixed::new(990_000)),
            militancy: Some(&Fixed::new(120_000)),
            deprived: 2_500_000,
            ..Default::default()
        },
    );
    let treasury = fixeds(&mut b, s.nations, 1_000_003);
    let income_tax_rate = fixeds(&mut b, s.nations, 1_000);
    let transfer_rate = fixeds(&mut b, s.nations, 500);
    let consumption_rate = fixeds(&mut b, s.nations, 700);
    let population = b.create_vector(&vec![1_250_000u64; s.nations]);
    let nations = NationTable::create(
        &mut b,
        &NationTableArgs {
            treasury: Some(treasury),
            income_tax_rate: Some(income_tax_rate),
            transfer_rate: Some(transfer_rate),
            consumption_rate: Some(consumption_rate),
            population: Some(population),
        },
    );
    let map = s.map.then(|| {
        let values = fixeds(&mut b, s.provinces, 37);
        MapView::create(&mut b, &MapViewArgs { mode: MapMode::Price, good: 3, values: Some(values) })
    });
    let market = s.market.then(|| {
        let price = fixeds(&mut b, s.goods, 11);
        let supply = fixeds(&mut b, s.goods, 13);
        let demand = fixeds(&mut b, s.goods, 17);
        let traded = fixeds(&mut b, s.goods, 19);
        MarketDetail::create(
            &mut b,
            &MarketDetailArgs {
                market: 7,
                price: Some(price),
                supply: Some(supply),
                demand: Some(demand),
                traded: Some(traded),
            },
        )
    });
    let province = s.province.then(|| {
        let profession = b.create_vector(&(0..s.pops as u16).collect::<Vec<_>>());
        let people = b.create_vector(&vec![5_000u32; s.pops]);
        let cash = fixeds(&mut b, s.pops, 23);
        let life_needs = fixeds(&mut b, s.pops, 29);
        let militancy = fixeds(&mut b, s.pops, 31);
        let pops = PopRows::create(
            &mut b,
            &PopRowsArgs {
                profession: Some(profession),
                people: Some(people),
                cash: Some(cash),
                life_needs: Some(life_needs),
                militancy: Some(militancy),
            },
        );
        ProvinceDetail::create(&mut b, &ProvinceDetailArgs { province: 42, pops: Some(pops), ..Default::default() })
    });
    let update = DayUpdate::create(
        &mut b,
        &DayUpdateArgs {
            day: 7_300,
            speed: Speed::Normal,
            skipped: 2,
            state_hash: Some(0xDEAD_BEEF_0BAD_F00D),
            world: Some(world),
            nations: Some(nations),
            map,
            market,
            province,
        },
    );
    server_frame(&mut b, ServerPayload::DayUpdate, update.as_union_value())
}
