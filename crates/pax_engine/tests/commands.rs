//! Commands (DECISIONS.md D21).

mod common;

use common::{d, random_world};
use pax_engine::tick::{run, step_with};
use pax_engine::{Command, CommandError, Fixed};

fn world_with_nation() -> pax_engine::World {
    (0..200).map(random_world).find(|w| !w.nations.is_empty()).expect("some random world has a nation")
}

#[test]
fn commands_apply_before_the_systems_of_their_tick() {
    let mut world = world_with_nation();
    let command = Command::SetIncomeTax { nation: 0, rate: d("0.37") };
    let (_, results) = step_with(&mut world, &[command]);
    assert_eq!(results, vec![Ok(())]);
    assert_eq!(world.nations.income_tax_rate[0], d("0.37"));
}

#[test]
fn invalid_commands_change_nothing() {
    let mut world = world_with_nation();
    let before = world.clone();
    for (command, error) in [
        (Command::SetIncomeTax { nation: 99, rate: d("0.1") }, CommandError::UnknownNation(99)),
        (Command::SetTransferRate { nation: 0, rate: d("1.5") }, CommandError::RateOutOfRange(d("1.5"))),
        (Command::SetIncomeTax { nation: 0, rate: -Fixed::EPSILON }, CommandError::RateOutOfRange(-Fixed::EPSILON)),
    ] {
        assert_eq!(world.apply(command), Err(error));
    }
    assert_eq!(world, before);
}

#[test]
fn a_consumption_rate_needs_a_basket() {
    let mut world = world_with_nation();
    let goods = world.defs.good_count();
    world.nations.basket[..goods].fill(Fixed::ZERO);
    assert_eq!(world.apply(Command::SetConsumptionRate { nation: 0, rate: d("0.1") }), Err(CommandError::NoBasket(0)));
    assert_eq!(world.apply(Command::SetConsumptionRate { nation: 0, rate: Fixed::ZERO }), Ok(()));
}

#[test]
fn commands_change_outcomes_deterministically() {
    let base = world_with_nation();
    let replay = |rate: Fixed| {
        let mut w = base.clone();
        step_with(&mut w, &[Command::SetIncomeTax { nation: 0, rate }]);
        run(&mut w, 60)
    };
    assert_eq!(replay(d("0.5")), replay(d("0.5")), "same command log, same hashes");
    assert_ne!(replay(d("0.5")), replay(d("0.0")), "a different policy gives a different history");
}
