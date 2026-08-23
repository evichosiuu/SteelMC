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
    crate::entity::init_entities();

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

#[test]
fn piglin_zombifies_in_overworld() {
    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use simdnbt::owned::NbtCompound;
    use steel_utils::{ChunkPos, Downcast};
    use crate::entity::entities::mobs::neutral::{PiglinEntity, ZombifiedPiglinEntity};
    use crate::entity::{ENTITIES, next_entity_id};
    use crate::test_support::insert_ready_full_chunk;

    init_vanilla_registry();
    init_behaviors();

    let world = fresh_test_world("piglin_zombify");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let piglin = ENTITIES
        .create(
            &vanilla_entities::PIGLIN,
            next_entity_id(),
            DVec3::new(0.0, 64.0, 0.0),
            std::sync::Arc::downgrade(&world),
        )
        .expect("should create piglin");

    world.try_add_entity(piglin.clone()).expect("should add piglin");

    let piglin_typed = piglin.as_ref().downcast_ref::<PiglinEntity>().unwrap();
    piglin_typed.set_time_in_overworld(299);
    piglin_typed.set_baby(true);
    piglin_typed.set_custom_name(Some(text_components::TextComponent::plain("Pigg")));

    // Tick 1: time becomes 300 (threshold is >300)
    piglin.base_tick();
    assert_eq!(piglin_typed.time_in_overworld(), 300);
    assert!(!piglin.is_removed());

    // Tick 2: time becomes 301, triggers conversion
    piglin.base_tick();
    assert!(piglin.is_removed());

    // Check spawned zombified piglin
    let entities = world.entity_manager().get_accessible_entities();
    let zombified = entities
        .iter()
        .find(|e| e.entity_type() == &vanilla_entities::ZOMBIFIED_PIGLIN)
        .expect("should find zombified piglin in world");

    use crate::entity::DisplayResolutor;
    let zombified_typed = zombified.as_ref().downcast_ref::<ZombifiedPiglinEntity>().unwrap();
    assert!(zombified_typed.is_baby());
    assert_eq!(zombified_typed.custom_name().unwrap().to_plain(&DisplayResolutor), "Pigg");

    // NBT test
    let mut nbt = NbtCompound::new();
    piglin_typed.save_additional(&mut nbt);
    assert_eq!(nbt.int("TimeInOverworld"), Some(301));

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut std::io::Cursor::new(&bytes)).unwrap();

    let loaded_piglin = PiglinEntity::new(&vanilla_entities::PIGLIN, 100, DVec3::ZERO, std::sync::Arc::downgrade(&world));
    loaded_piglin.load_additional((&borrowed).into());
    assert_eq!(loaded_piglin.time_in_overworld(), 301);
    assert!(loaded_piglin.is_baby());
}

#[test]
fn piglin_immune_to_zombification_resets_timer() {
    use steel_utils::{ChunkPos, Downcast};
    use crate::entity::entities::mobs::neutral::PiglinEntity;
    use crate::entity::{ENTITIES, next_entity_id};
    use crate::test_support::insert_ready_full_chunk;

    init_vanilla_registry();
    init_behaviors();

    let world = fresh_test_world("piglin_immune");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let piglin = ENTITIES
        .create(
            &vanilla_entities::PIGLIN,
            next_entity_id(),
            DVec3::new(0.0, 64.0, 0.0),
            std::sync::Arc::downgrade(&world),
        )
        .expect("should create piglin");

    world.try_add_entity(piglin.clone()).expect("should add piglin");

    let piglin_typed = piglin.as_ref().downcast_ref::<PiglinEntity>().unwrap();
    piglin_typed.set_immune_to_zombification(true);
    piglin_typed.set_time_in_overworld(299);

    piglin.base_tick();

    assert_eq!(piglin_typed.time_in_overworld(), 0);
    assert!(!piglin.is_removed());
}

#[test]
fn hoglin_zombifies_into_zoglin_in_overworld() {
    use simdnbt::borrow::read_compound as read_borrowed_compound;
    use simdnbt::owned::NbtCompound;
    use steel_utils::{ChunkPos, Downcast};
    use crate::entity::entities::mobs::hostile::{HoglinEntity, ZoglinEntity};
    use crate::entity::{ENTITIES, next_entity_id};
    use crate::test_support::insert_ready_full_chunk;

    init_vanilla_registry();
    init_behaviors();

    let world = fresh_test_world("hoglin_zombify");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let hoglin = ENTITIES
        .create(
            &vanilla_entities::HOGLIN,
            next_entity_id(),
            DVec3::new(0.0, 64.0, 0.0),
            std::sync::Arc::downgrade(&world),
        )
        .expect("should create hoglin");

    world.try_add_entity(hoglin.clone()).expect("should add hoglin");

    let hoglin_typed = hoglin.as_ref().downcast_ref::<HoglinEntity>().unwrap();
    hoglin_typed.set_time_in_overworld(300);
    hoglin_typed.set_baby(true);

    hoglin.base_tick();

    assert!(hoglin.is_removed());

    let entities = world.entity_manager().get_accessible_entities();
    let zoglin = entities
        .iter()
        .find(|e| e.entity_type() == &vanilla_entities::ZOGLIN)
        .expect("should find zoglin in world");

    let zoglin_typed = zoglin.as_ref().downcast_ref::<ZoglinEntity>().unwrap();
    assert!(zoglin_typed.is_baby());

    // NBT test
    let mut nbt = NbtCompound::new();
    hoglin_typed.save_additional(&mut nbt);
    assert_eq!(nbt.int("TimeInOverworld"), Some(301));

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut std::io::Cursor::new(&bytes)).unwrap();

    let loaded_hoglin = HoglinEntity::new(&vanilla_entities::HOGLIN, 101, DVec3::ZERO, std::sync::Arc::downgrade(&world));
    loaded_hoglin.load_additional((&borrowed).into());
    assert_eq!(loaded_hoglin.time_in_overworld(), 301);
    assert!(loaded_hoglin.is_baby());
}
