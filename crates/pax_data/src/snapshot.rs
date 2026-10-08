//! Binary world snapshots (D10, D23): the complete simulation state at one day, so a
//! save loads without replaying its whole history.
//!
//! **Format** (all integers little-endian):
//!
//! ```text
//! "PAXW"  u32 format  u64 content_hash  u64 state_hash  u64 seed  u64 day
//! geography:  strings province_keys, u32s province_market, strings market_keys,
//!             options market_nation
//! pops:       u32s size, fixeds cash, u16s profession, u32s province,
//!             fixeds life_needs, fixeds militancy
//! producers:  u16s kind, u32s province, u32s capacity, u32s employed, fixeds cash,
//!             fixeds wage, fixeds value_added_avg, fixeds output_stock, fixeds input_stock
//! markets:    fixeds price
//! nations:    strings key, fixeds treasury, fixeds income_tax_rate,
//!             fixeds transfer_rate, fixeds consumption_rate, fixeds basket
//!
//! u32s/u16s/fixeds = u64 count, then the values (a Fixed is its raw i64)
//! strings          = u64 count, then per string: u64 byte length, UTF-8 bytes
//! options          = u64 count, then per value: u8 present (0/1), u32 value
//! ```
//!
//! **Completeness:** the writer destructures every table without `..`, so adding a
//! column is a compile error here until the snapshot carries it. The reader checks
//! every length against the others, then recomputes `World::state_hash`. It must
//! equal the hash in the header, so a snapshot can only restore exactly the state
//! that was written. Definitions (`Defs`) aren't stored: they come from the
//! scenario, whose content hash must match. The scenario tables the snapshot does
//! store (geography, nation keys, seed) must equal the scenario's, and the restored
//! world must pass the engine's own table rules (`World::check_tables`: lengths, ids,
//! signs, rates), so a crafted file is refused, never trusted.
//!
//! It is a save-file format only, never sent over the wire (D22).

use std::path::Path;

use pax_engine::world::{Geography, Markets, Nations, Pops, Producers};
use pax_engine::{Fixed, World};

use crate::LoadError;

const MAGIC: &[u8; 4] = b"PAXW";
/// Version of the snapshot format; a different version is refused.
pub const SNAPSHOT_FORMAT: u32 = 1;

struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn len(&mut self, n: usize) {
        self.u64(n as u64);
    }
    fn u16s(&mut self, v: &[u16]) {
        self.len(v.len());
        v.iter().for_each(|x| self.0.extend_from_slice(&x.to_le_bytes()));
    }
    fn u32s(&mut self, v: &[u32]) {
        self.len(v.len());
        v.iter().for_each(|&x| self.u32(x));
    }
    fn fixeds(&mut self, v: &[Fixed]) {
        self.len(v.len());
        v.iter().for_each(|x| self.0.extend_from_slice(&x.raw().to_le_bytes()));
    }
    fn strings(&mut self, v: &[String]) {
        self.len(v.len());
        for s in v {
            self.len(s.len());
            self.0.extend_from_slice(s.as_bytes());
        }
    }
    fn options(&mut self, v: &[Option<u32>]) {
        self.len(v.len());
        for x in v {
            self.u8(u8::from(x.is_some()));
            self.u32(x.unwrap_or(0));
        }
    }
}

/// The snapshot bytes for `world`, and the state hash they record. Every column is
/// listed by name: a table that gains a column fails to compile here until the
/// format includes it.
pub fn encode(world: &World, content_hash: u64) -> (Vec<u8>, u64) {
    let World { defs: _, seed, day, geography, pops, producers, markets, nations, layout: _ } = world;
    let state_hash = world.state_hash();
    let mut o = Out(Vec::new());
    o.0.extend_from_slice(MAGIC);
    o.u32(SNAPSHOT_FORMAT);
    o.u64(content_hash);
    o.u64(state_hash);
    o.u64(*seed);
    o.u64(*day);

    let Geography { province_keys, province_market, market_keys, market_nation } = geography;
    o.strings(province_keys);
    o.u32s(province_market);
    o.strings(market_keys);
    o.options(market_nation);

    let Pops { size, cash, profession, province, life_needs, militancy } = pops;
    o.u32s(size);
    o.fixeds(cash);
    o.u16s(profession);
    o.u32s(province);
    o.fixeds(life_needs);
    o.fixeds(militancy);

    let Producers { kind, province, capacity, employed, cash, wage, value_added_avg, output_stock, input_stock } =
        producers;
    o.u16s(kind);
    o.u32s(province);
    o.u32s(capacity);
    o.u32s(employed);
    o.fixeds(cash);
    o.fixeds(wage);
    o.fixeds(value_added_avg);
    o.fixeds(output_stock);
    o.fixeds(input_stock);

    let Markets { price } = markets;
    o.fixeds(price);

    let Nations { key, treasury, income_tax_rate, transfer_rate, consumption_rate, basket } = nations;
    o.strings(key);
    o.fixeds(treasury);
    o.fixeds(income_tax_rate);
    o.fixeds(transfer_rate);
    o.fixeds(consumption_rate);
    o.fixeds(basket);
    (o.0, state_hash)
}

struct In<'a> {
    buf: &'a [u8],
    at: usize,
}

impl<'a> In<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.buf.len()).ok_or("the snapshot is truncated")?;
        let bytes = &self.buf[self.at..end];
        self.at = end;
        Ok(bytes)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4 bytes")))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().expect("8 bytes")))
    }
    /// A count, refused if it can't fit in the bytes left (so a corrupt count can't
    /// make the reader allocate gigabytes).
    fn len(&mut self, item_bytes: usize) -> Result<usize, String> {
        let n = self.u64()?;
        let left = (self.buf.len() - self.at) as u64;
        if n.saturating_mul(item_bytes.max(1) as u64) > left {
            return Err(format!("the snapshot is truncated or corrupt: it claims {n} items, more than it holds"));
        }
        Ok(n as usize)
    }
    fn u16s(&mut self) -> Result<Vec<u16>, String> {
        let n = self.len(2)?;
        Ok(self.take(n * 2)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect())
    }
    fn u32s(&mut self) -> Result<Vec<u32>, String> {
        let n = self.len(4)?;
        Ok(self.take(n * 4)?.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect())
    }
    fn fixeds(&mut self) -> Result<Vec<Fixed>, String> {
        let n = self.len(8)?;
        Ok(self.take(n * 8)?.as_chunks::<8>().0.iter().map(|c| Fixed::from_raw(i64::from_le_bytes(*c))).collect())
    }
    fn strings(&mut self) -> Result<Vec<String>, String> {
        let n = self.len(8)?;
        (0..n)
            .map(|_| {
                let len = self.len(1)?;
                String::from_utf8(self.take(len)?.to_vec())
                    .map_err(|_| "a string in the snapshot is not UTF-8".to_owned())
            })
            .collect()
    }
    fn options(&mut self) -> Result<Vec<Option<u32>>, String> {
        let n = self.len(5)?;
        (0..n)
            .map(|_| {
                let present = self.u8()?;
                let value = self.u32()?;
                match present {
                    0 => Ok(None),
                    1 => Ok(Some(value)),
                    other => Err(format!("an option flag is {other}, not 0 or 1")),
                }
            })
            .collect()
    }
}

fn decode(bytes: &[u8], scenario: &World, content_hash: u64) -> Result<(World, u64), String> {
    let defs = scenario.defs.clone();
    let mut i = In { buf: bytes, at: 0 };
    if i.take(4)? != MAGIC {
        return Err("not a world snapshot (bad magic)".to_owned());
    }
    let format = i.u32()?;
    if format != SNAPSHOT_FORMAT {
        return Err(format!("snapshot format {format} is not supported (expected {SNAPSHOT_FORMAT})"));
    }
    let written_for = i.u64()?;
    if written_for != content_hash {
        return Err(format!(
            "the snapshot was written for other content ({written_for:#018x}, the scenario is {content_hash:#018x})"
        ));
    }
    let state_hash = i.u64()?;
    let seed = i.u64()?;
    let day = i.u64()?;

    let geography = Geography {
        province_keys: i.strings()?,
        province_market: i.u32s()?,
        market_keys: i.strings()?,
        market_nation: i.options()?,
    };
    let pops = Pops {
        size: i.u32s()?,
        cash: i.fixeds()?,
        profession: i.u16s()?,
        province: i.u32s()?,
        life_needs: i.fixeds()?,
        militancy: i.fixeds()?,
    };
    let producers = Producers {
        kind: i.u16s()?,
        province: i.u32s()?,
        capacity: i.u32s()?,
        employed: i.u32s()?,
        cash: i.fixeds()?,
        wage: i.fixeds()?,
        value_added_avg: i.fixeds()?,
        output_stock: i.fixeds()?,
        input_stock: i.fixeds()?,
    };
    let markets = Markets { price: i.fixeds()? };
    let nations = Nations {
        key: i.strings()?,
        treasury: i.fixeds()?,
        income_tax_rate: i.fixeds()?,
        transfer_rate: i.fixeds()?,
        consumption_rate: i.fixeds()?,
        basket: i.fixeds()?,
    };
    if i.at != bytes.len() {
        return Err(format!("{} unexpected bytes after the snapshot", bytes.len() - i.at));
    }

    // The scenario's own tables (its content hash already matched): the snapshot's
    // copies must be the same, so a crafted file can't swap them.
    if geography != scenario.geography || nations.key != scenario.nations.key || seed != scenario.seed {
        return Err("the snapshot's provinces, markets, nations or seed differ from the scenario's".to_owned());
    }

    let mut world = World::new(defs, geography, seed);
    world.day = day;
    world.pops = pops;
    world.producers = producers;
    world.markets = markets;
    world.nations = nations;
    // The engine's own table rules (lengths, ids, signs, rates), before anything
    // indexes the restored columns: a crafted file is refused, never trusted.
    world.check_tables().map_err(|e| format!("the snapshot breaks a table rule: {e}"))?;
    let actual = world.state_hash();
    if actual != state_hash {
        return Err(format!(
            "the restored state's hash is {actual:#018x}, but the snapshot recorded {state_hash:#018x}"
        ));
    }
    Ok((world, state_hash))
}

/// Writes the snapshot of `world` to `path` (not atomically: callers that replace a
/// file write to a temporary path and rename, as `SaveData::write` does). Returns
/// the state hash it records.
pub fn write(path: &Path, world: &World, content_hash: u64) -> std::io::Result<u64> {
    let (bytes, state_hash) = encode(world, content_hash);
    std::fs::write(path, bytes)?;
    Ok(state_hash)
}

/// Reads a snapshot written for this content, rebuilding and verifying the world.
/// Returns it with the state hash it was verified against.
/// `scenario` is the scenario's freshly loaded world: it supplies the definitions,
/// and the snapshot's scenario tables must equal its own.
pub fn read(path: &Path, scenario: &World, content_hash: u64) -> Result<(World, u64), LoadError> {
    let bytes = std::fs::read(path).map_err(|e| LoadError::single(format!("{}: {e}", path.display())))?;
    decode(&bytes, scenario, content_hash).map_err(|e| LoadError::single(format!("{}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_states_after(days: u64) -> crate::Scenario {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let mut s = crate::load_scenario(&dir).unwrap();
        for _ in 0..days {
            crate::step_logged(&mut s.world, &s.commands);
        }
        s
    }

    #[test]
    fn a_snapshot_restores_the_exact_state_and_it_runs_on_identically() {
        let mut s = two_states_after(400);
        let (bytes, hash) = encode(&s.world, s.content_hash);
        let (mut restored, verified) = decode(&bytes, &s.world, s.content_hash).unwrap();
        assert_eq!((restored.state_hash(), verified), (s.world.state_hash(), hash));
        // Same future: the snapshot holds all the state there is.
        for _ in 0..100 {
            crate::step_logged(&mut s.world, &s.commands);
            crate::step_logged(&mut restored, &s.commands);
        }
        assert_eq!(restored.state_hash(), s.world.state_hash());
    }

    #[test]
    fn corruption_and_wrong_content_are_refused() {
        let s = two_states_after(31);
        let (bytes, _) = encode(&s.world, s.content_hash);
        assert!(decode(&bytes, &s.world, s.content_hash ^ 1).unwrap_err().contains("other content"));
        assert!(decode(&bytes[..bytes.len() - 3], &s.world, s.content_hash).unwrap_err().contains("truncated"));
        let mut huge = bytes.clone();
        // The province-key count, just after the 36-byte header: claim 2^40 strings.
        huge[36..44].copy_from_slice(&(1u64 << 40).to_le_bytes());
        assert!(decode(&huge, &s.world, s.content_hash).unwrap_err().contains("more than it holds"));
        // A valid state under another state's hash (the header's bytes 16..24).
        let mut richer = s.world.clone();
        richer.pops.cash[0] += Fixed::ONE;
        let (mut swapped, _) = encode(&richer, s.content_hash);
        swapped[16..24].copy_from_slice(&bytes[16..24]);
        assert!(decode(&swapped, &s.world, s.content_hash).unwrap_err().contains("hash"));
    }

    /// A crafted snapshot (with a hash that matches it: the hash is not a defence)
    /// can't swap the scenario's tables or break the engine's table rules: it is
    /// refused before anything indexes the restored columns.
    #[test]
    fn crafted_tables_and_values_are_refused() {
        let s = two_states_after(5);
        let refused = |crafted: World| {
            let (bytes, _) = encode(&crafted, s.content_hash);
            decode(&bytes, &s.world, s.content_hash).unwrap_err()
        };
        let mut w = s.world.clone();
        w.pops.province[0] = 9_999;
        assert!(refused(w).contains("unknown province"));
        let mut w = s.world.clone();
        w.nations.income_tax_rate[0] = Fixed::from_int(2);
        assert!(refused(w).contains("rate outside [0, 1]"));
        let mut w = s.world.clone();
        w.pops.cash[0] = -Fixed::ONE;
        assert!(refused(w).contains("negative cash"));
        let mut w = s.world.clone();
        w.producers.input_stock.push(Fixed::ZERO);
        assert!(refused(w).contains("input stock per good"));
        let mut w = s.world.clone();
        w.geography.market_keys[0].push('x');
        assert!(refused(w).contains("differ from the scenario"));
    }
}

#[cfg(all(test, feature = "bench"))]
mod load_time {
    /// D23's 30-second rule at D13's long-term scale: a snapshot loads in tens of
    /// milliseconds, however long the game ran (D23 records the latest measurement). Run by hand:
    /// `cargo test -p pax_data --release --features bench -- --ignored snapshot_load_time --nocapture`
    #[test]
    #[ignore]
    fn snapshot_load_time() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two_states");
        let base = crate::load_scenario(&dir).unwrap();
        let regions = 1_500;
        let scale = (1_000_000 / (base.world.pops.size.len() as u32 * regions)).max(1);
        let mut world = crate::bench::replicate(&base.world, scale, regions);
        pax_engine::step(&mut world);
        let path = std::env::temp_dir().join(format!("pax-snapshot-time-{}.world", std::process::id()));
        let write = std::time::Instant::now();
        super::write(&path, &world, base.content_hash).unwrap();
        let write_ms = write.elapsed().as_secs_f64() * 1e3;
        let bytes = std::fs::metadata(&path).unwrap().len();
        let read = std::time::Instant::now();
        let (restored, _) = super::read(&path, &world, base.content_hash).unwrap();
        let read_ms = read.elapsed().as_secs_f64() * 1e3;
        std::fs::remove_file(&path).unwrap();
        println!(
            "{} POP rows, {} markets: snapshot {:.1} MB, write {write_ms:.0} ms, load (read + verify) {read_ms:.0} ms",
            world.pops.size.len(),
            world.geography.market_count(),
            bytes as f64 / 1e6
        );
        assert_eq!(restored.state_hash(), world.state_hash());
        assert!(read_ms < 30_000.0, "D23: loading must take under 30 s");
    }
}
