use std::sync::Weak;

use glam::DVec3;
use steel_registry::{init_vanilla_registry, vanilla_damage_types, vanilla_entities};

use crate::behavior::init_behaviors;
use crate::entity::damage::DamageSource;
use crate::entity::entities::EnderManEntity;
use crate::entity::Entity;
use crate::world::World;

#[test]
fn enderman_is_immune_to_projectiles() {
    init_vanilla_registry();
    init_behaviors();

    let enderman = EnderManEntity::new(
        &vanilla_entities::ENDERMAN,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    );

    let proj_source = DamageSource::environment(&vanilla_damage_types::THROWN)
        .with_direct_entity(2)
        .with_causing_entity(3);

    let world = fresh_test_world("enderman_proj_test");
    let result = enderman.hurt(&world, &proj_source, 5.0);

    assert!(!result, "Enderman should reject projectile damage");
}

#[test]
fn enderman_creepy_and_carried_block_state() {
    init_vanilla_registry();
    init_behaviors();

    let enderman = EnderManEntity::new(
        &vanilla_entities::ENDERMAN,
        1,
        DVec3::ZERO,
        Weak::<World>::new(),
    );

    assert!(!enderman.is_creepy());
    enderman.set_creepy(true);
    assert!(enderman.is_creepy());

    assert_eq!(enderman.carried_block(), None);
    enderman.set_carried_block(Some(steel_utils::BlockStateId(10)));
    assert_eq!(enderman.carried_block(), Some(steel_utils::BlockStateId(10)));
}

fn fresh_test_world(name: &'static str) -> std::sync::Arc<World> {
    crate::test_support::fresh_test_world(name)
}
