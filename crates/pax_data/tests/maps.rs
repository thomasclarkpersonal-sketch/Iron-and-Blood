//! Province map validation (M3-7, DATA_FORMAT "Province map").

mod common;

const SEA: [u8; 3] = [0, 0, 128];
const RIVERLANDS: [u8; 3] = [10, 200, 10];
const COAST: [u8; 3] = [10, 150, 150];
const DALE: [u8; 3] = [200, 150, 50];
const PEAKS: [u8; 3] = [150, 150, 160];

/// A temporary copy of two_states whose map is `pixels` (rows of colours) with `toml`
/// as its provinces.toml. Removed when dropped.
fn scenario_with_map(name: &str, pixels: &[&[[u8; 3]]], toml: &str) -> common::TempScenario {
    let copy = common::TempScenario::copy("two_states", &format!("map-{name}"));
    let (width, height) = (pixels[0].len() as u32, pixels.len() as u32);
    let file = std::fs::File::create(copy.dir.join("map/provinces.png")).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = pixels.iter().flat_map(|row| row.iter().flatten().copied()).collect();
    encoder.write_header().unwrap().write_image_data(&data).unwrap();
    std::fs::write(copy.dir.join("map/provinces.toml"), toml).unwrap();
    copy
}

fn entry(key: &str, color: [u8; 3], label: [u32; 2]) -> String {
    format!("\n[[province]]\nkey = \"{key}\"\ncolor = {color:?}\nlabel = {label:?}\n")
}

fn standard_toml() -> String {
    format!("background = {SEA:?}\n")
        + &entry("riverlands", RIVERLANDS, [0, 0])
        + &entry("coast", COAST, [1, 0])
        + &entry("dale", DALE, [2, 0])
        + &entry("peaks", PEAKS, [3, 0])
}

fn errors(scenario: &common::TempScenario) -> String {
    match pax_data::load_scenario(&scenario.dir) {
        Ok(_) => panic!("the map should have been refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_valid_map_loads_with_colours_in_scenario_order() {
    let s = scenario_with_map("valid", &[&[RIVERLANDS, COAST, DALE, PEAKS], &[SEA, SEA, SEA, SEA]], &standard_toml());
    let map = pax_data::load_scenario(&s.dir).unwrap().map.expect("two_states names a map").map;
    assert_eq!((map.width, map.height, map.background), (4, 2, Some(SEA)));
    assert_eq!(map.colors, [RIVERLANDS, COAST, DALE, PEAKS]);
    assert_eq!(map.labels[3], [3, 0]);
}

#[test]
fn many_stray_colours_are_summarised() {
    let row: Vec<[u8; 3]> = (1..=8).map(|c| [c, c, c]).collect();
    let s = scenario_with_map(
        "many-stray",
        &[&[RIVERLANDS, COAST, DALE, PEAKS, SEA, SEA, SEA, SEA], &row],
        &standard_toml(),
    );
    let e = errors(&s);
    assert_eq!(e.matches("is not a province").count(), 5, "only the first five are listed: {e}");
    assert!(e.contains("...and 3 more stray colours"), "{e}");
}

#[test]
fn a_stray_colour_is_refused() {
    let s =
        scenario_with_map("stray", &[&[RIVERLANDS, COAST, DALE, PEAKS], &[SEA, [1, 2, 3], SEA, SEA]], &standard_toml());
    assert!(errors(&s).contains("colour [1, 2, 3] (first at pixel 1,1) is not a province"));
}

#[test]
fn a_province_without_pixels_is_refused() {
    let toml = format!("background = {SEA:?}\n")
        + &entry("riverlands", RIVERLANDS, [0, 0])
        + &entry("coast", COAST, [1, 0])
        + &entry("dale", DALE, [2, 0])
        + &entry("peaks", PEAKS, [2, 0]);
    let s = scenario_with_map("unpainted", &[&[RIVERLANDS, COAST, DALE, DALE]], &toml);
    let e = errors(&s);
    assert!(e.contains("province 'peaks' has no pixels"), "{e}");
}

#[test]
fn shared_colours_misplaced_labels_and_missing_provinces_are_refused() {
    let toml = format!("background = {SEA:?}\n")
        + &entry("riverlands", RIVERLANDS, [0, 0])
        + &entry("coast", RIVERLANDS, [1, 0])
        + &entry("dale", DALE, [0, 0]);
    let s = scenario_with_map("broken", &[&[RIVERLANDS, COAST, DALE, PEAKS]], &toml);
    let e = errors(&s);
    assert!(e.contains("share the colour"), "{e}");
    assert!(e.contains("province 'peaks' has no colour"), "{e}");
}

#[test]
fn a_label_must_sit_on_its_own_province() {
    let mut toml = standard_toml();
    toml = toml.replace("label = [3, 0]", "label = [0, 1]"); // peaks' label on the sea
    let s = scenario_with_map("label", &[&[RIVERLANDS, COAST, DALE, PEAKS], &[SEA, SEA, SEA, SEA]], &toml);
    assert!(errors(&s).contains("province 'peaks' has its label at 0,1, outside its own pixels"));
}

#[test]
fn the_shipped_two_states_map_is_valid() {
    let map = pax_data::load_scenario(&common::repo().join("scenarios/two_states")).unwrap().map.unwrap().map;
    assert_eq!((map.width, map.height, map.colors.len()), (640, 400, 4));
}
