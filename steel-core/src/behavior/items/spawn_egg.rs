//! Spawn egg item behavior (`SpawnEggItem`).
//!
//! Spawns entities in the world or updates mob spawners when used on blocks,
//! and spawns baby variants when used on living entities.
//! Mirrors vanilla `SpawnEggItem`.

use std::io::Cursor;
use std::sync::Arc;

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use steel_macros::item_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::vanilla_components::ENTITY_DATA;
use steel_registry::item_stack::ItemStack;
use steel_registry::{vanilla_block_entity_types, vanilla_blocks};
use steel_utils::types::InteractionHand;

use crate::behavior::context::{InteractionResult, UseOnContext};
use crate::behavior::item::ItemBehavior;
use crate::behavior::BlockStateBehaviorExt as _;
use crate::block_entity::BLOCK_ENTITIES;
use crate::entity::{
    ENTITIES, EntitySpawnReason, LivingEntity, next_entity_id,
};
use crate::player::Player;

/// Behavior for spawn egg items.
#[item_behavior(class = "SpawnEggItem")]
pub struct SpawnEggItem;

impl ItemBehavior for SpawnEggItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let entity_data = context
            .inv
            .with_item(|item| item.get(ENTITY_DATA).cloned());
        let Some(entity_data) = entity_data else {
            return InteractionResult::Pass;
        };

        let entity_type = entity_data.entity_type();
        let clicked_pos = context.hit_result.block_pos;
        let block_state = context.world.get_block_state(clicked_pos);

        if block_state.get_block() == &vanilla_blocks::SPAWNER {
            let mut spawn_data = simdnbt::owned::NbtCompound::new();
            let mut entity_compound = simdnbt::owned::NbtCompound::new();
            entity_compound.insert("id", entity_type.key.to_string());
            spawn_data.insert("entity", simdnbt::owned::NbtTag::Compound(entity_compound));
            let mut nbt = simdnbt::owned::NbtCompound::new();
            nbt.insert("SpawnData", simdnbt::owned::NbtTag::Compound(spawn_data));

            let spawner = BLOCK_ENTITIES.create_and_load_owned_or_raw(
                &vanilla_block_entity_types::MOB_SPAWNER,
                Arc::downgrade(context.world),
                clicked_pos,
                block_state,
                nbt,
            );
            context.world.set_block_entity(spawner);
            context.world.broadcast_block_entity_if_needed(clicked_pos);

            if !context.player.has_infinite_materials() {
                context.inv.with_item(|item| item.shrink(1));
            }
            return InteractionResult::Success;
        }

        let place_pos = if block_state.can_be_replaced(&context.build_place_context()) {
            clicked_pos
        } else {
            context.hit_result.direction.relative(clicked_pos)
        };

        let spawn_pos = DVec3::new(
            f64::from(place_pos.x()) + 0.5,
            f64::from(place_pos.y()),
            f64::from(place_pos.z()) + 0.5,
        );

        let Some(entity) = ENTITIES.create(
            entity_type,
            next_entity_id(),
            spawn_pos,
            Arc::downgrade(context.world),
        ) else {
            return InteractionResult::Pass;
        };

        if !entity_data.data().is_empty() {
            let mut bytes = Vec::new();
            entity_data.data().as_compound().write(&mut bytes);
            if let Ok(borrowed) = read_borrowed_compound(&mut Cursor::new(&bytes)) {
                entity.load_additional((&borrowed).into());
            }
        }

        if let Some(mob) = entity.as_mob() {
            let _ = mob.finalize_spawn(context.world, EntitySpawnReason::SpawnItemUse, None);
        }

        if let Err(error) = context.world.try_add_entity(entity) {
            log::debug!("failed to spawn entity from spawn egg: {error}");
            return InteractionResult::Fail;
        }

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }

    fn interact_living_entity(
        &self,
        stack: &mut ItemStack,
        player: &Player,
        target: &dyn LivingEntity,
        _hand: InteractionHand,
    ) -> InteractionResult {
        let Some(entity_data) = stack.get(ENTITY_DATA) else {
            return InteractionResult::Pass;
        };

        if entity_data.entity_type() != target.entity_type() {
            return InteractionResult::Pass;
        }

        let Some(world) = target.level() else {
            return InteractionResult::Pass;
        };

        let spawn_pos = target.position();
        let Some(entity) = ENTITIES.create(
            entity_data.entity_type(),
            next_entity_id(),
            spawn_pos,
            Arc::downgrade(&world),
        ) else {
            return InteractionResult::Pass;
        };

        if let Some(ageable) = entity.as_ageable_mob() {
            ageable.set_baby(true);
        }

        if let Some(mob) = entity.as_mob() {
            let _ = mob.finalize_spawn(&world, EntitySpawnReason::SpawnItemUse, None);
        }

        if world.try_add_entity(entity).is_ok() {
            if !player.has_infinite_materials() {
                stack.shrink(1);
            }
            return InteractionResult::Success;
        }

        InteractionResult::Pass
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_registry::blocks::properties::Direction;
    use steel_registry::item_stack::ItemStack;
    use steel_registry::items::item::BlockHitResult;
    use steel_registry::{init_vanilla_registry, REGISTRY, RegistryExt, vanilla_entities};
    use steel_utils::BlockPos;
    use steel_utils::locks::SyncMutex;
    use steel_utils::types::InteractionHand;

    use super::SpawnEggItem;
    use crate::behavior::init_behaviors;
    use crate::behavior::ItemBehavior;
    use crate::behavior::context::{InteractionResult, UseOnContext};
    use crate::entity::{init_entities, next_entity_id, AgeableMob};
    use crate::inventory::container::Container as _;
    use crate::player::player_inventory::PlayerInventory;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    #[test]
    fn spawn_egg_spawns_typed_mob_into_world_on_use_on() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let world = fresh_test_world("spawn_egg_spawns_typed_mob");
        let chunk_pos = steel_utils::ChunkPos::new(0, 0);
        insert_ready_full_chunk(&world, chunk_pos);

        let egg_item = REGISTRY
            .items
            .by_key(&steel_utils::Identifier::vanilla_static("cow_spawn_egg"))
            .expect("cow spawn egg should exist");
        let stack = ItemStack::new(egg_item);

        let inventory = Arc::new(SyncMutex::new(PlayerInventory::new()));
        inventory.lock().set_item(0, stack);

        let player = TestPlayerBuilder::new(world.clone(), "player", 100).build();
        let pos = BlockPos::new(1, 64, 1);

        let hit_result = BlockHitResult {
            location: DVec3::new(1.5, 65.0, 1.5),
            direction: Direction::Up,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        };

        let mut context = UseOnContext::new(
            &player,
            InteractionHand::MainHand,
            hit_result,
            &world,
            inventory.clone(),
        );

        let behavior = SpawnEggItem;
        let result = behavior.use_on(&mut context);

        assert_eq!(result, InteractionResult::Success);

        let entities = world.get_entities_in_aabb(&steel_utils::WorldAabb::new(
            1.0, 64.0, 1.0, 3.0, 67.0, 3.0,
        ));
        assert!(!entities.is_empty(), "cow mob should be spawned in world");
        let spawned = &entities[0];
        assert_eq!(spawned.entity_type(), &vanilla_entities::COW);
        assert!(spawned.as_mob().is_some());
    }

    #[test]
    fn spawn_egg_spawns_fallback_raw_entity_for_uncoded_mobs() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let world = fresh_test_world("spawn_egg_fallback_raw");
        let chunk_pos = steel_utils::ChunkPos::new(0, 0);
        insert_ready_full_chunk(&world, chunk_pos);

        let egg_item = REGISTRY
            .items
            .by_key(&steel_utils::Identifier::vanilla_static("creeper_spawn_egg"))
            .expect("creeper spawn egg should exist");
        let stack = ItemStack::new(egg_item);

        let inventory = Arc::new(SyncMutex::new(PlayerInventory::new()));
        inventory.lock().set_item(0, stack);

        let player = TestPlayerBuilder::new(world.clone(), "player", 101).build();
        let pos = BlockPos::new(2, 64, 2);

        let hit_result = BlockHitResult {
            location: DVec3::new(2.5, 65.0, 2.5),
            direction: Direction::Up,
            block_pos: pos,
            miss: false,
            inside: false,
            world_border_hit: false,
        };

        let mut context = UseOnContext::new(
            &player,
            InteractionHand::MainHand,
            hit_result,
            &world,
            inventory,
        );

        let behavior = SpawnEggItem;
        let result = behavior.use_on(&mut context);

        assert_eq!(result, InteractionResult::Success);

        let entities = world.get_entities_in_aabb(&steel_utils::WorldAabb::new(
            2.0, 64.0, 2.0, 4.0, 67.0, 4.0,
        ));
        assert!(!entities.is_empty(), "creeper entity should be spawned in world");
        let spawned = &entities[0];
        assert_eq!(spawned.entity_type(), &vanilla_entities::CREEPER);
    }

    #[test]
    fn spawn_egg_on_living_entity_spawns_baby() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let world = fresh_test_world("spawn_egg_living_baby");
        let chunk_pos = steel_utils::ChunkPos::new(0, 0);
        insert_ready_full_chunk(&world, chunk_pos);

        let cow = crate::entity::entities::CowEntity::new(
            &vanilla_entities::COW,
            next_entity_id(),
            DVec3::new(5.0, 64.0, 5.0),
            Arc::downgrade(&world),
        );
        let shared_cow: Arc<dyn crate::entity::Entity> = Arc::new(cow);
        world
            .try_add_entity(shared_cow.clone())
            .expect("cow should be added to world");

        let egg_item = REGISTRY
            .items
            .by_key(&steel_utils::Identifier::vanilla_static("cow_spawn_egg"))
            .expect("cow spawn egg should exist");
        let mut stack = ItemStack::new(egg_item);

        let player = TestPlayerBuilder::new(world.clone(), "player", 102).build();
        let behavior = SpawnEggItem;
        let result = behavior.interact_living_entity(
            &mut stack,
            &player,
            shared_cow.as_living_entity().unwrap(),
            InteractionHand::MainHand,
        );

        assert_eq!(result, InteractionResult::Success);

        let entities = world.get_entities_in_aabb(&steel_utils::WorldAabb::new(
            4.0, 63.0, 4.0, 6.0, 67.0, 6.0,
        ));
        assert_eq!(entities.len(), 2, "baby cow should be spawned beside adult");
        let baby = entities.iter().find(|e| e.id() != shared_cow.id()).unwrap();
        assert!(baby.as_ageable_mob().is_some_and(AgeableMob::is_baby));
    }
}
