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
use crate::inventory::container::Container as _;
use crate::test_support::{fresh_test_world, insert_ready_full_chunk};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;

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

#[test]
fn player_mounts_rideable_minecart_on_interact() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("minecart_mount_interact");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let minecart: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::MINECART,
            500,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create rideable minecart");
    world.try_add_entity(Arc::clone(&minecart)).unwrap();

    let player: SharedEntity = crate::test_support::TestPlayerBuilder::new(world.clone(), "rider", 501).build();
    world.try_add_entity(Arc::clone(&player)).unwrap();

    let player_ref = player.as_player().unwrap();
    let result = minecart.interact(player_ref, steel_utils::types::InteractionHand::MainHand, DVec3::ZERO);
    assert_eq!(result, crate::behavior::InteractionResult::Success);
    assert!(minecart.is_vehicle());
    assert_eq!(player.vehicle().map(|v| v.id()), Some(500));
}

#[test]
fn minecart_moves_along_rail_when_pushed() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("minecart_pushed_movement");
    let chunk_pos = ChunkPos::new(0, 0);
    insert_ready_full_chunk(&world, chunk_pos);

    for x in 0..10 {
        let rail_pos = BlockPos::new(x, 64, 1);
        world.set_block(rail_pos.below(), vanilla_blocks::STONE.default_state(), UpdateFlags::UPDATE_NONE);
        let state = vanilla_blocks::RAIL
            .default_state()
            .set_value(&steel_registry::blocks::properties::BlockStateProperties::RAIL_SHAPE, steel_registry::blocks::properties::RailShape::EastWest);
        world.set_block(rail_pos, state, UpdateFlags::UPDATE_NONE);
    }

    let minecart: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::MINECART,
            600,
            DVec3::new(1.5, 64.0625, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create minecart");
    world.try_add_entity(Arc::clone(&minecart)).unwrap();

    assert!(minecart.is_on_rails());

    // Give push impulse along +X
    minecart.push_impulse(DVec3::new(0.2, 0.0, 0.0));
    assert!(minecart.velocity().x > 0.0);

    // Tick minecart
    minecart.tick();

    // Minecart position should have moved in +X direction
    assert!(minecart.position().x > 1.5);
}

#[test]
fn furnace_minecart_fuel_and_push_force() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("furnace_minecart_fuel");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    for x in 0..10 {
        let rail_pos = BlockPos::new(x, 64, 1);
        world.set_block(rail_pos.below(), vanilla_blocks::STONE.default_state(), UpdateFlags::UPDATE_NONE);
        let state = vanilla_blocks::RAIL
            .default_state()
            .set_value(&steel_registry::blocks::properties::BlockStateProperties::RAIL_SHAPE, steel_registry::blocks::properties::RailShape::EastWest);
        world.set_block(rail_pos, state, UpdateFlags::UPDATE_NONE);
    }

    let minecart: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::FURNACE_MINECART,
            700,
            DVec3::new(1.5, 64.0625, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create furnace minecart");
    world.try_add_entity(Arc::clone(&minecart)).unwrap();

    let player: SharedEntity = crate::test_support::TestPlayerBuilder::new(world.clone(), "stoker", 701).build();
    world.try_add_entity(Arc::clone(&player)).unwrap();

    let player_ref = player.as_player().unwrap();
    let _ = player_ref.try_set_position(DVec3::new(0.5, 64.0, 1.5));
    player_ref.inventory.lock().set_item(0, steel_registry::item_stack::ItemStack::new(&steel_registry::vanilla_items::COAL));

    let result = minecart.interact(player_ref, steel_utils::types::InteractionHand::MainHand, DVec3::ZERO);
    assert_eq!(result, crate::behavior::InteractionResult::Success);

    // Tick furnace minecart
    let start_x = minecart.position().x;
    minecart.tick();
    assert!(minecart.position().x > start_x);
}

#[test]
fn minecart_moves_along_closed_curve_rail_loop() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("minecart_curve_loop");
    let chunk_pos = ChunkPos::new(0, 0);
    insert_ready_full_chunk(&world, chunk_pos);

    // Create a 2x2 closed loop of curved rails:
    // (0,0): SouthEast  (1,0): SouthWest
    // (0,1): NorthEast  (1,1): NorthWest
    let loop_rails = [
        (BlockPos::new(0, 64, 0), steel_registry::blocks::properties::RailShape::SouthEast),
        (BlockPos::new(1, 64, 0), steel_registry::blocks::properties::RailShape::SouthWest),
        (BlockPos::new(1, 64, 1), steel_registry::blocks::properties::RailShape::NorthWest),
        (BlockPos::new(0, 64, 1), steel_registry::blocks::properties::RailShape::NorthEast),
    ];

    for (pos, shape) in loop_rails {
        world.set_block(pos.below(), vanilla_blocks::STONE.default_state(), UpdateFlags::UPDATE_NONE);
        let state = vanilla_blocks::RAIL
            .default_state()
            .set_value(&steel_registry::blocks::properties::BlockStateProperties::RAIL_SHAPE, shape);
        world.set_block(pos, state, UpdateFlags::UPDATE_NONE);
    }

    let minecart: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::MINECART,
            900,
            DVec3::new(0.5, 64.0625, 0.5),
            Arc::downgrade(&world),
        )
        .expect("should create minecart");
    world.try_add_entity(Arc::clone(&minecart)).unwrap();

    assert!(minecart.is_on_rails());

    // Push the minecart in +X direction
    minecart.push_impulse(DVec3::new(0.3, 0.0, 0.0));

    // Tick minecart multiple times around the loop and track visited positions
    let mut visited_blocks = std::collections::HashSet::new();
    for _ in 0..40 {
        minecart.tick();
        assert!(minecart.is_on_rails(), "minecart should stay on rails while traversing loop");
        let pos = minecart.position();
        let block_pos = BlockPos::containing(pos.x, pos.y, pos.z);
        visited_blocks.insert((block_pos.x(), block_pos.z()));
    }

    assert_eq!(
        visited_blocks.len(),
        4,
        "minecart should visit all 4 blocks in loop, visited {:?}",
        visited_blocks
    );
}

#[test]
fn minecart_break_and_item_drop_on_hurt() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("minecart_hurt_break");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let minecart: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::MINECART,
            800,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create minecart");
    world.try_add_entity(Arc::clone(&minecart)).unwrap();

    let dmg = crate::entity::DamageSource::environment(&steel_registry::vanilla_damage_types::GENERIC);
    let hurt_result = minecart.hurt(&world, &dmg, 10.0);
    assert!(hurt_result);
    assert!(minecart.is_removed());

    // Check dropped item
    let dropped_items = world.get_entities_in_aabb(&steel_utils::WorldAabb::new(1.0, 63.0, 1.0, 2.0, 65.0, 2.0));
    assert_eq!(dropped_items.len(), 1);
    assert_eq!(dropped_items[0].entity_type(), &vanilla_entities::ITEM);
}

#[test]
fn minecart_curve_movement_preserves_smooth_velocity_components() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("minecart_curve_smooth");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    // Place a SouthEast rail at (0, 64, 0)
    let pos = BlockPos::new(0, 64, 0);
    world.set_block(pos.below(), vanilla_blocks::STONE.default_state(), UpdateFlags::UPDATE_NONE);
    let state = vanilla_blocks::RAIL
        .default_state()
        .set_value(
            &steel_registry::blocks::properties::BlockStateProperties::RAIL_SHAPE,
            steel_registry::blocks::properties::RailShape::SouthEast,
        );
    world.set_block(pos, state, UpdateFlags::UPDATE_NONE);

    let minecart: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::MINECART,
            950,
            DVec3::new(0.5, 64.0625, 0.5),
            Arc::downgrade(&world),
        )
        .expect("should create minecart");
    world.try_add_entity(Arc::clone(&minecart)).unwrap();

    // Push minecart in +X
    minecart.push_impulse(DVec3::new(0.2, 0.0, 0.0));
    minecart.tick();

    let vel = minecart.velocity();
    // On SouthEast curve (+Z to +X), both X and Z velocity components must be non-zero (smooth curve motion)
    assert!(vel.x > 0.0, "vel.x should be positive on curve, got {}", vel.x);
    assert!(vel.z < 0.0, "vel.z should be negative on curve, got {}", vel.z);
    assert!(
        minecart.position().x > 0.5,
        "x position should advance smoothly, got {}",
        minecart.position().x
    );
    assert!(
        minecart.position().z < 0.75,
        "z position should advance smoothly towards 0.5, got {}",
        minecart.position().z
    );
}

#[test]
fn player_dismounts_minecart_on_shift_input() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("minecart_dismount_shift");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    // Place stone floor for safe dismount
    for x in 0..3 {
        for z in 0..3 {
            world.set_block(
                BlockPos::new(x, 63, z),
                vanilla_blocks::STONE.default_state(),
                UpdateFlags::UPDATE_NONE,
            );
        }
    }

    let minecart: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::MINECART,
            1000,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create rideable minecart");
    world.try_add_entity(Arc::clone(&minecart)).unwrap();

    let player: SharedEntity = crate::test_support::TestPlayerBuilder::new(world.clone(), "rider", 1001).build();
    world.try_add_entity(Arc::clone(&player)).unwrap();

    let player_ref = player.as_player().unwrap();
    player_ref.set_client_loaded(true);

    let result = minecart.interact(player_ref, steel_utils::types::InteractionHand::MainHand, DVec3::ZERO);
    assert_eq!(result, crate::behavior::InteractionResult::Success);
    assert!(player_ref.is_passenger());

    // Send SPlayerInput with shift bit (0x20)
    let packet = steel_protocol::packets::game::SPlayerInput { flags: 0x20 };
    player_ref.handle_player_input(packet);

    assert!(!player_ref.is_passenger(), "player should no longer be a passenger");
    assert!(!minecart.is_vehicle(), "minecart should no longer have passengers");

    // Verify player is placed at a safe location
    let dismount_pos = player_ref.position();
    assert!(
        (dismount_pos - minecart.position()).length() > 0.1,
        "player should be placed at dismount position relative to minecart"
    );
}

#[test]
fn boat_item_use_spawns_boat_entity() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("boat_item_spawn");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let water_pos = BlockPos::new(1, 64, 1);
    world.set_block(water_pos, vanilla_blocks::WATER.default_state(), UpdateFlags::UPDATE_NONE);

    let player: SharedEntity = crate::test_support::TestPlayerBuilder::new(world.clone(), "boat_user", 100).build();
    world.try_add_entity(Arc::clone(&player)).unwrap();
    let player_ref = player.as_player().unwrap();
    let _ = player_ref.try_set_position(DVec3::new(1.5, 66.0, 1.5));
    player_ref.set_rotation((0.0, 90.0)); // looking straight down at water

    let stack = steel_registry::item_stack::ItemStack::new(&steel_registry::vanilla_items::OAK_BOAT);
    player_ref.inventory.lock().set_item(0, stack);

    let mut context = crate::behavior::UseItemContext::new(
        player_ref,
        steel_utils::types::InteractionHand::MainHand,
        &world,
        player_ref.inventory.clone(),
    );

    let behavior = crate::behavior::items::BoatItem::new(&vanilla_entities::OAK_BOAT);
    let result = crate::behavior::ItemBehavior::use_item(&behavior, &mut context);

    assert_eq!(result, crate::behavior::InteractionResult::Success);

    let entities = world.get_entities_in_aabb(&steel_utils::WorldAabb::new(0.0, 60.0, 0.0, 3.0, 70.0, 3.0));
    let boats: Vec<_> = entities.into_iter().filter(|e| e.entity_type() == &vanilla_entities::OAK_BOAT).collect();
    assert_eq!(boats.len(), 1);
}

#[test]
fn standard_boat_allows_two_passengers_including_mob_and_player() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("standard_boat_passengers");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let boat: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::OAK_BOAT,
            1100,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create boat");
    world.try_add_entity(Arc::clone(&boat)).unwrap();

    let pig: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::PIG,
            1101,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create pig");
    world.try_add_entity(Arc::clone(&pig)).unwrap();

    let player: SharedEntity = crate::test_support::TestPlayerBuilder::new(world.clone(), "boat_driver", 1102).build();
    world.try_add_entity(Arc::clone(&player)).unwrap();

    // Pig mounts boat
    assert!(boat.can_add_passenger(pig.as_ref()));
    assert!(pig.start_riding(&boat));
    assert_eq!(boat.passengers().len(), 1);

    // Player mounts boat as 2nd passenger
    assert!(boat.can_add_passenger(player.as_ref()));
    let player_ref = player.as_player().unwrap();
    let result = boat.interact(player_ref, steel_utils::types::InteractionHand::MainHand, DVec3::ZERO);
    assert_eq!(result, crate::behavior::InteractionResult::Success);
    assert_eq!(boat.passengers().len(), 2);

    // 3rd entity cannot mount
    let zombie: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::ZOMBIE,
            1103,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create zombie");
    world.try_add_entity(Arc::clone(&zombie)).unwrap();

    assert!(!boat.can_add_passenger(zombie.as_ref()));
    assert!(!zombie.start_riding(&boat));
    assert_eq!(boat.passengers().len(), 2);
}

#[test]
fn chest_boat_allows_one_passenger_and_opens_inventory_when_full_or_sneaking() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("chest_boat_passengers_inventory");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let chest_boat: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::OAK_CHEST_BOAT,
            1200,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create chest boat");
    world.try_add_entity(Arc::clone(&chest_boat)).unwrap();

    let pig: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::PIG,
            1201,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create pig");
    world.try_add_entity(Arc::clone(&pig)).unwrap();

    let player: SharedEntity = crate::test_support::TestPlayerBuilder::new(world.clone(), "chest_boat_driver", 1202).build();
    world.try_add_entity(Arc::clone(&player)).unwrap();

    // Pig mounts chest boat
    assert!(chest_boat.can_add_passenger(pig.as_ref()));
    assert!(pig.start_riding(&chest_boat));
    assert_eq!(chest_boat.passengers().len(), 1);

    // 2nd passenger cannot mount chest boat
    assert!(!chest_boat.can_add_passenger(player.as_ref()));

    // Interacting with full chest boat opens container
    let player_ref = player.as_player().unwrap();
    let result = chest_boat.interact(player_ref, steel_utils::types::InteractionHand::MainHand, DVec3::ZERO);
    assert_eq!(result, crate::behavior::InteractionResult::Success);

    // Unride pig
    pig.stop_riding();
    assert_eq!(chest_boat.passengers().len(), 0);

    // Sneaking player opens container instead of mounting
    player_ref.set_crouching(true);
    let result = chest_boat.interact(player_ref, steel_utils::types::InteractionHand::MainHand, DVec3::ZERO);
    assert_eq!(result, crate::behavior::InteractionResult::Success);
    assert_eq!(chest_boat.passengers().len(), 0);

    // Non-sneaking player mounts empty chest boat
    player_ref.set_crouching(false);
    let result = chest_boat.interact(player_ref, steel_utils::types::InteractionHand::MainHand, DVec3::ZERO);
    assert_eq!(result, crate::behavior::InteractionResult::Success);
    assert_eq!(chest_boat.passengers().len(), 1);
}

#[test]
fn mob_pushes_into_boat_and_becomes_passenger() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("mob_push_boat");
    insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

    let boat: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::OAK_BOAT,
            1300,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create boat");
    world.try_add_entity(Arc::clone(&boat)).unwrap();

    let cow: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::COW,
            1301,
            DVec3::new(1.5, 64.0, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create cow");
    world.try_add_entity(Arc::clone(&cow)).unwrap();

    // Push cow into boat
    boat.push_entity(cow.as_ref());

    assert_eq!(boat.passengers().len(), 1);
    assert_eq!(cow.vehicle().map(|v| v.id()), Some(1300));
}

#[test]
fn minecart_stops_at_solid_block_obstacle() {
    init_vanilla_registry();
    init_behaviors();
    init_entities();

    let world = fresh_test_world("minecart_obstacle");
    let chunk_pos = ChunkPos::new(0, 0);
    insert_ready_full_chunk(&world, chunk_pos);

    // Place rails on stone at (0, 64, 1) and (1, 64, 1)
    for x in 0..2 {
        let rail_pos = BlockPos::new(x, 64, 1);
        world.set_block(rail_pos.below(), vanilla_blocks::STONE.default_state(), UpdateFlags::UPDATE_NONE);
        let state = vanilla_blocks::RAIL
            .default_state()
            .set_value(&steel_registry::blocks::properties::BlockStateProperties::RAIL_SHAPE, steel_registry::blocks::properties::RailShape::EastWest);
        world.set_block(rail_pos, state, UpdateFlags::UPDATE_NONE);
    }

    // Place a solid STONE block at (2, 64, 1) blocking the rail path
    world.set_block(BlockPos::new(2, 64, 1), vanilla_blocks::STONE.default_state(), UpdateFlags::UPDATE_NONE);

    let minecart: SharedEntity = crate::entity::ENTITIES
        .create(
            &vanilla_entities::MINECART,
            1400,
            DVec3::new(0.5, 64.0625, 1.5),
            Arc::downgrade(&world),
        )
        .expect("should create minecart");
    world.try_add_entity(Arc::clone(&minecart)).unwrap();

    assert!(minecart.is_on_rails());

    // Push minecart toward +X into the solid wall at x=2
    minecart.push_impulse(DVec3::new(0.4, 0.0, 0.0));

    // Tick minecart multiple times
    for _ in 0..10 {
        minecart.tick();
    }

    // Solid block is at x=2. Minecart width is 0.98 (half-width 0.49).
    // The minecart bounding box cannot enter x=2.0, so position.x must be <= 1.51
    assert!(
        minecart.position().x < 2.0,
        "minecart should not enter solid block at x=2, got {}",
        minecart.position().x
    );
    assert!(
        minecart.position().x <= 1.51,
        "minecart should stop against solid block wall, got {}",
        minecart.position().x
    );
}