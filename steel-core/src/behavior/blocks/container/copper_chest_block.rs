//! Copper Chest and Weathering Copper Chest block behavior implementations.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, ChestType, Direction, EnumProperty,
};
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BLOCK_ENTITIES;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::{chest, double_chest};
use crate::player::Player;
use crate::world::{LevelReader, World};

const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const CHEST_TYPE: &EnumProperty<ChestType> = &BlockStateProperties::CHEST_TYPE;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Behavior for unwaxed/waxed copper chest blocks.
#[block_behavior]
pub struct CopperChestBlock {
    block: BlockRef,
}

impl CopperChestBlock {
    /// Creates a new copper chest block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for CopperChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let facing = context.horizontal_direction().opposite();
        let mut chest_type = ChestType::Single;

        if !context.is_secondary_use_active() {
            let world = context.world;
            let pos = context.place_pos();

            let right_pos = pos.relative(facing.rotate_y_clockwise());
            let right_state = world.get_block_state(right_pos);
            if right_state.get_block() == self.block
                && right_state.get_value(FACING) == facing
                && right_state.get_value(CHEST_TYPE) == ChestType::Single
            {
                chest_type = ChestType::Left;
            } else {
                let left_pos = pos.relative(facing.rotate_y_counter_clockwise());
                let left_state = world.get_block_state(left_pos);
                if left_state.get_block() == self.block
                    && left_state.get_value(FACING) == facing
                    && left_state.get_value(CHEST_TYPE) == ChestType::Single
                {
                    chest_type = ChestType::Right;
                }
            }
        }

        let is_waterlogged = context.is_water_source();

        Some(
            self.block
                .default_state()
                .set_value(FACING, facing)
                .set_value(CHEST_TYPE, chest_type)
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
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };

        let Some(container_ref) = ContainerRef::from_block_entity(block_entity) else {
            return InteractionResult::Pass;
        };

        let facing = state.get_value(FACING);
        let chest_type = state.get_value(CHEST_TYPE);

        let inventory = player.inventory.clone();

        if chest_type == ChestType::Single {
            player.open_menu(
                TextComponent::translated(translations::CONTAINER_CHEST.msg()),
                move |context| chest(inventory, context.container_id, container_ref, 3),
            );
        } else if let Some(connected_dir) = match chest_type {
            ChestType::Single => None,
            ChestType::Left => Some(facing.rotate_y_clockwise()),
            ChestType::Right => Some(facing.rotate_y_counter_clockwise()),
        } {
            let other_pos = pos.relative(connected_dir);
            let other_state = world.get_block_state(other_pos);

            if other_state.get_block() == self.block && other_state.get_value(FACING) == facing {
                let other_entity = world.get_block_entity(other_pos);
                let other_ref = other_entity.and_then(ContainerRef::from_block_entity);

                if let Some(other_ref) = other_ref {
                    let (first, second) = if chest_type == ChestType::Left {
                        (container_ref, other_ref)
                    } else {
                        (other_ref, container_ref)
                    };

                    player.open_menu(
                        TextComponent::translated(translations::CONTAINER_CHEST_DOUBLE.msg()),
                        move |context| double_chest(inventory, context.container_id, first, second),
                    );
                } else {
                    player.open_menu(
                        TextComponent::translated(translations::CONTAINER_CHEST.msg()),
                        move |context| chest(inventory, context.container_id, container_ref, 3),
                    );
                }
            } else {
                player.open_menu(
                    TextComponent::translated(translations::CONTAINER_CHEST.msg()),
                    move |context| chest(inventory, context.container_id, container_ref, 3),
                );
            }
        }

        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::CHEST,
            level,
            pos,
            state,
        ))
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

/// Behavior for weathering copper chest blocks.
#[block_behavior]
pub struct WeatheringCopperChestBlock {
    block: BlockRef,
}

impl WeatheringCopperChestBlock {
    /// Creates a new weathering copper chest block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for WeatheringCopperChestBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let facing = context.horizontal_direction().opposite();
        let is_waterlogged = context.is_water_source();

        Some(
            self.block
                .default_state()
                .set_value(FACING, facing)
                .set_value(CHEST_TYPE, ChestType::Single)
                .set_value(WATERLOGGED, is_waterlogged),
        )
    }

    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };

        let Some(container_ref) = ContainerRef::from_block_entity(block_entity) else {
            return InteractionResult::Pass;
        };

        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_CHEST.msg()),
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
            &vanilla_block_entity_types::CHEST,
            level,
            pos,
            state,
        ))
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
