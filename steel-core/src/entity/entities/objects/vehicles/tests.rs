use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;
use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos};

use super::*;
use crate::behavior::init_behaviors;
use crate::behavior::BLOCK_BEHAVIORS;
use crate::entity::{Entity as _, InsideBlockEffectCollector, SharedEntity, init_entities};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

#[test]
fn all_minecart_entities_have_vanilla_abstract_minecart_flags() {
    init_vanilla_registry();
    init_entities();

    let pos = DVec3::new(10.0, 64.0, 10.0);
    let weak = Weak::new();

    let minecarts: Vec<Box<dyn crate::entity::Entity>> = vec![
        Box::new(MinecartEntity::new(&vanilla_entities::MINECART, 1, pos, weak.clone())),
        Box::new(ChestMinecartEntity::new(&vanilla_entities::CHEST_MINECART, 2, pos, weak.clone())),
        Box::new(FurnaceMinecartEntity::new(&vanilla_entities::FURNACE_MINECART, 3, pos, weak.clone())),
        Box::new(TntMinecartEntity::new(&vanilla_entities::TNT_MINECART, 4, pos, weak.clone())),
        Box::new(HopperMinecartEntity::new(&vanilla_entities::HOPPER_MINECART, 5, pos, weak.clone())),
        Box::new(SpawnerMinecartEntity::new(&vanilla_entities::SPAWNER_MINECART, 6, pos, weak.clone())),
        Box::new(CommandBlockMinecartEntity::new(&vanilla_entities::COMMAND_BLOCK_MINECART, 7, pos, weak)),
    ];

    for minecart in minecarts {
        assert!(minecart.is_pickable(), "minecart {} should be pickable", minecart.entity_type().key);
        assert!(minecart.is_pushable(), "minecart {} should be pushable", minecart.entity_type().key);
        assert!(minecart.blocks_building(), "minecart {} should block building", minecart.entity_type().key);
        assert_eq!(minecart.dimension_changing_delay(), 10, "minecart {} portal delay", minecart.entity_type().key);
    }
}

#[test]
fn furnace_minecart_nbt_persistence() {
    let minecart = FurnaceMinecartEntity::new(
        &vanilla_entities::FURNACE_MINECART,
        1,
        DVec3::ZERO,
        Weak::new(),
    );

    let mut nbt = NbtCompound::new();
    minecart.save_additional(&mut nbt);
    assert_eq!(nbt.double("PushX"), Some(0.0));
    assert_eq!(nbt.double("PushZ"), Some(0.0));
    assert_eq!(nbt.short("Fuel"), Some(0));

    // Test loading
    let mut save = NbtCompound::new();
    save.insert("PushX", 1.5_f64);
    save.insert("PushZ", -2.5_f64);
    save.insert("Fuel", 300_i16);
    save.insert("HasTicked", 1_i8);

    let mut bytes = Vec::new();
    save.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut std::io::Cursor::new(&bytes)).unwrap();
    minecart.load_additional((&borrowed).into());

    let mut loaded_nbt = NbtCompound::new();
    minecart.save_additional(&mut loaded_nbt);
    assert_eq!(loaded_nbt.double("PushX"), Some(1.5));
    assert_eq!(loaded_nbt.double("PushZ"), Some(-2.5));
    assert_eq!(loaded_nbt.short("Fuel"), Some(300));
}

#[test]
fn tnt_minecart_nbt_persistence() {
    let minecart = TntMinecartEntity::new(
        &vanilla_entities::TNT_MINECART,
        1,
        DVec3::ZERO,
        Weak::new(),
    );

    let mut nbt = NbtCompound::new();
    minecart.save_additional(&mut nbt);
    assert_eq!(nbt.int("TNTFuse"), Some(-1));

    let mut save = NbtCompound::new();
    save.insert("TNTFuse", 80_i32);
    let mut bytes = Vec::new();
    save.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut std::io::Cursor::new(&bytes)).unwrap();
    minecart.load_additional((&borrowed).into());

    let mut loaded_nbt = NbtCompound::new();
    minecart.save_additional(&mut loaded_nbt);
    assert_eq!(loaded_nbt.int("TNTFuse"), Some(80));
}

#[test]
fn hopper_minecart_nbt_persistence_and_methods() {
    let minecart = HopperMinecartEntity::new(
        &vanilla_entities::HOPPER_MINECART,
        1,
        DVec3::ZERO,
        Weak::new(),
    );

    assert!(minecart.is_enabled());
    minecart.set_enabled(false);
    assert!(!minecart.is_enabled());

    let mut save = NbtCompound::new();
    save.insert("Enabled", 1_i8);
    save.insert("TransferCooldown", 4_i32);
    save.insert("LootTable", "minecraft:chests/simple_dungeon");
    save.insert("LootTableSeed", 12345_i64);

    let mut bytes = Vec::new();
    save.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut std::io::Cursor::new(&bytes)).unwrap();
    minecart.load_additional((&borrowed).into());

    assert!(minecart.is_enabled());

    let mut loaded_nbt = NbtCompound::new();
    minecart.save_additional(&mut loaded_nbt);
    assert_eq!(loaded_nbt.byte("Enabled"), Some(1));
    assert_eq!(loaded_nbt.int("TransferCooldown"), Some(4));
    assert_eq!(loaded_nbt.string("LootTable").map(ToString::to_string), Some("minecraft:chests/simple_dungeon".to_owned()));
    assert_eq!(loaded_nbt.long("LootTableSeed"), Some(12345));
}

#[test]
fn command_block_minecart_nbt_persistence_and_methods() {
    let minecart = CommandBlockMinecartEntity::new(
        &vanilla_entities::COMMAND_BLOCK_MINECART,
        1,
        DVec3::ZERO,
        Weak::new(),
    );

    assert_eq!(minecart.command(), "");
    assert!(minecart.track_output());

    minecart.set_command("say Hello Steel");
    minecart.set_track_output(false);

    assert_eq!(minecart.command(), "say Hello Steel");
    assert!(!minecart.track_output());

    let mut nbt = NbtCompound::new();
    minecart.save_additional(&mut nbt);
    assert_eq!(nbt.string("Command").map(ToString::to_string), Some("say Hello Steel".to_owned()));
    assert_eq!(nbt.byte("TrackOutput"), Some(0));

    let mut save = NbtCompound::new();
    save.insert("Command", "tp @a 0 100 0");
    save.insert("SuccessCount", 5_i32);
    save.insert("LastOutput", "Teleported");
    save.insert("TrackOutput", 1_i8);

    let mut bytes = Vec::new();
    save.write(&mut bytes);
    let borrowed = read_borrowed_compound(&mut std::io::Cursor::new(&bytes)).unwrap();
    minecart.load_additional((&borrowed).into());

    assert_eq!(minecart.command(), "tp @a 0 100 0");
    assert!(minecart.track_output());
}

#[test]
fn detector_rail_powers_for_all_minecart_types() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("detector_rail_all_minecarts");
    let pos = BlockPos::new(8, 64, 8);
    insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

    world.set_block(
        pos.below(),
        vanilla_blocks::STONE.default_state(),
        UpdateFlags::UPDATE_NONE,
    );

    let detector_behavior = BLOCK_BEHAVIORS.get_behavior(&vanilla_blocks::DETECTOR_RAIL);

    let entity_types = [
        &vanilla_entities::MINECART,
        &vanilla_entities::CHEST_MINECART,
        &vanilla_entities::FURNACE_MINECART,
        &vanilla_entities::TNT_MINECART,
        &vanilla_entities::HOPPER_MINECART,
        &vanilla_entities::SPAWNER_MINECART,
        &vanilla_entities::COMMAND_BLOCK_MINECART,
    ];

    for (i, &entity_type) in entity_types.iter().enumerate() {
        let detector_state = vanilla_blocks::DETECTOR_RAIL.default_state();
        world.set_block(pos, detector_state, UpdateFlags::UPDATE_NONE);

        let entity_id = 1000 + i as i32;
        let minecart: SharedEntity = crate::entity::ENTITIES
            .create(
                entity_type,
                entity_id,
                DVec3::new(8.5, 64.0, 8.5),
                Arc::downgrade(&world),
            )
            .unwrap_or_else(|| panic!("failed to create minecart entity for {}", entity_type.key));

        world
            .try_add_entity(Arc::clone(&minecart))
            .expect("minecart entity should enter world");

        let mut effects = InsideBlockEffectCollector::new();
        detector_behavior.entity_inside(
            detector_state,
            &world,
            pos,
            minecart.as_ref(),
            &mut effects,
            true,
        );

        let powered_state = world.get_block_state(pos);
        assert!(
            detector_behavior.get_own_signal(powered_state, &world, pos, crate::world::SignalQueryContext::DEFAULT) > 0,
            "detector rail should activate for minecart type {}",
            entity_type.key
        );

        minecart.set_removed(crate::entity::RemovalReason::Discarded);
        detector_behavior.tick(powered_state, &world, pos);
    }
}
