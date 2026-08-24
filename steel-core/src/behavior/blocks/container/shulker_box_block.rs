//! Shulker Box block behavior implementation.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::properties::{BlockStateProperties, Direction, EnumProperty};
use steel_registry::{sound_events, vanilla_block_entity_types};
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BLOCK_ENTITIES;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::chest;
use crate::player::Player;
use crate::world::{LevelReader, World};

const FACING: &EnumProperty<Direction> = &BlockStateProperties::FACING;

/// Behavior for Shulker Box blocks.
#[block_behavior]
pub struct ShulkerBoxBlock {
    block: BlockRef,
}

impl ShulkerBoxBlock {
    /// Creates a new shulker box block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn can_open(state: BlockStateId, world: &World, pos: BlockPos) -> bool {
        let facing = state.get_value(FACING);
        let adjacent_pos = pos.relative(facing);
        let adjacent_state = world.get_block_state(adjacent_pos);

        !adjacent_state.is_solid()
    }
}

impl BlockBehavior for ShulkerBoxBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let facing = context.clicked_face();
        Some(self.block.default_state().set_value(FACING, facing))
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
        if !Self::can_open(state, world.as_ref(), pos) {
            return InteractionResult::Pass;
        }

        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };

        let Some(container_ref) = ContainerRef::from_block_entity(block_entity) else {
            return InteractionResult::Pass;
        };

        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_SHULKER_BOX.msg()),
            move |context| chest(inventory, context.container_id, container_ref, 3),
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
            &vanilla_block_entity_types::SHULKER_BOX,
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
                &sound_events::BLOCK_SHULKER_BOX_OPEN
            } else {
                &sound_events::BLOCK_SHULKER_BOX_CLOSE
            };
            let pitch = 0.9 + rand::random::<f32>() * 0.1;
            world.play_sound(sound, SoundSource::Blocks, pos, 0.5, pitch, None);
            true
        } else {
            false
        }
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, |container| {
                calculate_redstone_signal_from_container(container)
            })
    }
}
