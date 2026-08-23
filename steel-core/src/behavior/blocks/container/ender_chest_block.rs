//! Ender Chest block behavior implementation.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty, Direction, EnumProperty};
use steel_registry::{sound_events, vanilla_block_entity_types};
use steel_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::EnderChestBlockEntity;
use crate::block_entity::BLOCK_ENTITIES;
use crate::inventory::lock::ContainerRef;
use crate::inventory::menu::kinds::chest;
use crate::player::Player;
use crate::world::World;

const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Behavior for Ender Chest blocks.
#[block_behavior]
pub struct EnderChestBlock {
    block: BlockRef,
}

impl EnderChestBlock {
    /// Creates a new ender chest block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for EnderChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let facing = context.horizontal_direction().opposite();
        let is_waterlogged = context.is_water_source();

        Some(
            self.block
                .default_state()
                .set_value(FACING, facing)
                .set_value(WATERLOGGED, is_waterlogged),
        )
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let block_entity = if let Some(be) = world.get_block_entity(pos) {
            be
        } else {
            let be = BLOCK_ENTITIES.create_or_raw(
                &vanilla_block_entity_types::ENDER_CHEST,
                Arc::downgrade(world),
                pos,
                state,
            );
            world.set_block_entity(Arc::clone(&be));
            be
        };

        let ender_chest_be = block_entity
            .downcast_ref::<EnderChestBlockEntity>()
            .expect("Ender chest block entity must be EnderChestBlockEntity");

        let inventory = player.inventory.clone();
        let ender_chest_ref = ContainerRef::owned_by_block_entity(
            player.ender_chest_inventory.clone(),
            ender_chest_be.base_arc(),
        );

        player.open_menu(
            TextComponent::translated(translations::CONTAINER_ENDERCHEST.msg()),
            move |context| chest(inventory, context.container_id, ender_chest_ref, 3),
        );

        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::ENDER_CHEST,
            level,
            pos,
            state,
        ))
    }

    fn trigger_event(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        param_a: i32,
        param_b: i32,
    ) -> bool {
        if param_a == 1 {
            let sound = if param_b > 0 {
                &sound_events::BLOCK_ENDER_CHEST_OPEN
            } else {
                &sound_events::BLOCK_ENDER_CHEST_CLOSE
            };
            let pitch = 0.9 + rand::random::<f32>() * 0.1;
            world.play_sound(sound, SoundSource::Blocks, pos, 0.5, pitch, None);
            true
        } else {
            false
        }
    }
}
