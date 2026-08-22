use std::sync::Weak;

use glam::DVec3;
use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};

use crate::behavior::init_behaviors;
use crate::entity::{Entity, LivingEntity};
use crate::entity::entities::ChickenEntity;
use crate::world::World;

#[test]
fn chicken_slow_falling_reduces_vertical_velocity() {
    init_vanilla_registry();
    init_behaviors();

    let chicken = ChickenEntity::new(
        &vanilla_entities::CHICKEN,
        1,
        DVec3::new(0.0, 100.0, 0.0),
        Weak::<World>::new(),
    );

    chicken.set_velocity(DVec3::new(0.0, -1.0, 0.0));
    chicken.set_on_ground(false);

    chicken.ai_step();

    assert!(chicken.velocity().y > -1.0, "vertical falling velocity should be slowed by flap physics");
    assert_eq!(chicken.fall_distance(), 0.0, "fall distance should be reset so chicken takes no fall damage");
}

#[test]
fn chicken_food_check() {
    init_vanilla_registry();
    init_behaviors();

    let wheat_seeds = steel_registry::item_stack::ItemStack::new(&vanilla_items::WHEAT_SEEDS);
    let dirt = steel_registry::item_stack::ItemStack::new(&vanilla_items::DIRT);

    assert!(ChickenEntity::is_food(&wheat_seeds), "wheat seeds should be valid chicken food");
    assert!(!ChickenEntity::is_food(&dirt), "dirt should not be chicken food");
}
