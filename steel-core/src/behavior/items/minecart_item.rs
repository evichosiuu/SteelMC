//! Minecart item behavior (`MinecartItem`).
//!
//! Spawns minecart entities when used on rails.
//! Mirrors vanilla `MinecartItem`.

use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::BlockStateProperties;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_block_tags::BlockTag;

use crate::behavior::context::{InteractionResult, UseOnContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::{ENTITIES, next_entity_id};

/// Behavior for minecart items.
#[item_behavior(class = "MinecartItem")]
pub struct MinecartItem {
    #[json_arg(vanilla_entities, json = "type")]
    entity_type: EntityTypeRef,
}

impl MinecartItem {
    /// Creates a minecart item behavior for the given entity type.
    #[must_use]
    pub const fn new(entity_type: EntityTypeRef) -> Self {
        Self { entity_type }
    }
}

impl ItemBehavior for MinecartItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let clicked_pos = context.hit_result.block_pos;
        let block_state = context.world.get_block_state(clicked_pos);

        if !block_state.get_block().has_tag(&BlockTag::RAILS) {
            return InteractionResult::Pass;
        }

        let is_slope = block_state
            .try_get_value(&BlockStateProperties::RAIL_SHAPE)
            .is_some_and(|shape| shape.is_slope());

        let y_offset = if is_slope { 0.5 } else { 0.0 };
        let spawn_pos = DVec3::new(
            f64::from(clicked_pos.x()) + 0.5,
            f64::from(clicked_pos.y()) + 0.0625 + y_offset,
            f64::from(clicked_pos.z()) + 0.5,
        );

        let Some(entity) = ENTITIES.create(
            self.entity_type,
            next_entity_id(),
            spawn_pos,
            Arc::downgrade(context.world),
        ) else {
            return InteractionResult::Pass;
        };

        if let Some(custom_name) = context
            .inv
            .with_item(|item| item.custom_name().map(std::borrow::Cow::into_owned))
        {
            entity.set_custom_name(Some(custom_name));
        }

        if let Err(error) = context.world.try_add_entity(entity) {
            log::debug!("failed to spawn minecart entity from item: {error}");
            return InteractionResult::Fail;
        }

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_registry::blocks::block_state_ext::BlockStateExt as _;
    use steel_registry::blocks::properties::{BlockStateProperties, Direction, RailShape};
    use steel_registry::data_components::vanilla_components::CUSTOM_NAME;
    use steel_registry::item_stack::ItemStack;
    use steel_registry::items::item::BlockHitResult;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities, vanilla_items};
    use steel_utils::locks::SyncMutex;
    use steel_utils::types::{InteractionHand, UpdateFlags};
    use steel_utils::{BlockPos, ChunkPos, WorldAabb};
    use text_components::TextComponent;

    use super::MinecartItem;
    use crate::behavior::ItemBehavior as _;
    use crate::behavior::context::{InteractionResult, UseOnContext};
    use crate::behavior::init_behaviors;
    use crate::entity::init_entities;
    use crate::inventory::container::Container as _;
    use crate::player::player_inventory::PlayerInventory;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    #[test]
    fn minecart_item_use_on_rail_spawns_minecart_and_shrinks_stack() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let world = fresh_test_world("minecart_item_use_on_rail");
        let chunk_pos = ChunkPos::new(0, 0);
        insert_ready_full_chunk(&world, chunk_pos);

        let rail_pos = BlockPos::new(1, 64, 1);
        world.set_block(
            rail_pos.below(),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        );
        world.set_block(
            rail_pos,
            vanilla_blocks::RAIL.default_state(),
            UpdateFlags::UPDATE_NONE,
        );

        let mut stack = ItemStack::new(&vanilla_items::MINECART);
        stack.set(CUSTOM_NAME, TextComponent::plain("Speedy Minecart"));

        let inventory = Arc::new(SyncMutex::new(PlayerInventory::new()));
        inventory.lock().set_item(0, stack);

        let player = TestPlayerBuilder::new(world.clone(), "player", 200).build();

        let hit_result = BlockHitResult {
            location: DVec3::new(1.5, 64.1, 1.5),
            direction: Direction::Up,
            block_pos: rail_pos,
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

        let behavior = MinecartItem::new(&vanilla_entities::MINECART);
        let result = behavior.use_on(&mut context);

        assert_eq!(result, InteractionResult::Success);

        // Check stack shrunk
        let count = inventory.lock().get_item(0).count();
        assert_eq!(count, 0);

        // Check entity spawned at flat rail height
        let entities = world.get_entities_in_aabb(&WorldAabb::new(1.0, 64.0, 1.0, 2.0, 65.0, 2.0));
        assert_eq!(entities.len(), 1);
        let spawned = &entities[0];
        assert_eq!(spawned.entity_type(), &vanilla_entities::MINECART);
        let pos = spawned.position();
        assert!((pos.x - 1.5).abs() < f64::EPSILON);
        assert!((pos.y - 64.0625).abs() < f64::EPSILON);
        assert!((pos.z - 1.5).abs() < f64::EPSILON);
        assert_eq!(
            spawned.custom_name().map(|n| n.to_string()),
            Some("Speedy Minecart".to_owned())
        );
    }

    #[test]
    fn minecart_item_use_on_sloped_rail_includes_slope_offset() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let world = fresh_test_world("minecart_item_sloped_rail");
        let chunk_pos = ChunkPos::new(0, 0);
        insert_ready_full_chunk(&world, chunk_pos);

        let rail_pos = BlockPos::new(1, 64, 1);
        world.set_block(
            rail_pos.below(),
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        );
        let sloped_state = vanilla_blocks::RAIL
            .default_state()
            .set_value(&BlockStateProperties::RAIL_SHAPE, RailShape::AscendingEast);
        world.set_block(rail_pos, sloped_state, UpdateFlags::UPDATE_NONE);

        let stack = ItemStack::new(&vanilla_items::CHEST_MINECART);
        let inventory = Arc::new(SyncMutex::new(PlayerInventory::new()));
        inventory.lock().set_item(0, stack);

        let player = TestPlayerBuilder::new(world.clone(), "player", 201).build();

        let hit_result = BlockHitResult {
            location: DVec3::new(1.5, 64.1, 1.5),
            direction: Direction::Up,
            block_pos: rail_pos,
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

        let behavior = MinecartItem::new(&vanilla_entities::CHEST_MINECART);
        let result = behavior.use_on(&mut context);

        assert_eq!(result, InteractionResult::Success);

        let entities = world.get_entities_in_aabb(&WorldAabb::new(1.0, 64.0, 1.0, 2.0, 66.0, 2.0));
        assert_eq!(entities.len(), 1);
        let spawned = &entities[0];
        assert_eq!(spawned.entity_type(), &vanilla_entities::CHEST_MINECART);
        let pos = spawned.position();
        assert!((pos.y - 64.5625).abs() < f64::EPSILON);
    }

    #[test]
    fn minecart_item_use_on_non_rail_returns_pass() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let world = fresh_test_world("minecart_item_non_rail");
        let chunk_pos = ChunkPos::new(0, 0);
        insert_ready_full_chunk(&world, chunk_pos);

        let pos = BlockPos::new(1, 64, 1);
        world.set_block(
            pos,
            vanilla_blocks::STONE.default_state(),
            UpdateFlags::UPDATE_NONE,
        );

        let stack = ItemStack::new(&vanilla_items::TNT_MINECART);
        let inventory = Arc::new(SyncMutex::new(PlayerInventory::new()));
        inventory.lock().set_item(0, stack);

        let player = TestPlayerBuilder::new(world.clone(), "player", 202).build();

        let hit_result = BlockHitResult {
            location: DVec3::new(1.5, 64.5, 1.5),
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

        let behavior = MinecartItem::new(&vanilla_entities::TNT_MINECART);
        let result = behavior.use_on(&mut context);

        assert_eq!(result, InteractionResult::Pass);
        assert_eq!(inventory.lock().get_item(0).count(), 1);
    }
}
