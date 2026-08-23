//! Chest and Trapped Chest block behavior implementation.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, ChestType, Direction, EnumProperty,
};
use steel_registry::{sound_events, vanilla_block_entity_types};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, translations};
use text_components::TextComponent;

use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::{InventoryAccess, PlacementSource};
use crate::block_entity::BLOCK_ENTITIES;
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::{chest, double_chest};
use crate::player::Player;
use crate::world::{LevelReader, ScheduledTickAccess, World};

const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const CHEST_TYPE: &EnumProperty<ChestType> = &BlockStateProperties::CHEST_TYPE;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Behavior for chest blocks.
#[block_behavior]
pub struct ChestBlock {
    block: BlockRef,
}

impl ChestBlock {
    /// Creates a new chest block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Gets neighbor offset for double chest based on facing and chest type.
    #[must_use]
    pub fn get_connected_direction(facing: Direction, chest_type: ChestType) -> Option<Direction> {
        match chest_type {
            ChestType::Single => None,
            ChestType::Left => Some(facing.rotate_y_clockwise()),
            ChestType::Right => Some(facing.rotate_y_counter_clockwise()),
        }
    }
}

impl BlockBehavior for ChestBlock {
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

    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source: &PlacementSource<'_>,
    ) {
        let facing = state.get_value(FACING);
        let chest_type = state.get_value(CHEST_TYPE);

        if chest_type != ChestType::Single {
            if let Some(connected_dir) = Self::get_connected_direction(facing, chest_type.clone())
            {
                let other_pos = pos.relative(connected_dir);
                let other_state = world.get_block_state(other_pos);

                if other_state.get_block() == self.block
                    && other_state.get_value(FACING) == facing
                    && other_state.get_value(CHEST_TYPE) == ChestType::Single
                {
                    let expected_other_type = if chest_type == ChestType::Left {
                        ChestType::Right
                    } else {
                        ChestType::Left
                    };
                    let new_other_state =
                        other_state.set_value(CHEST_TYPE, expected_other_type);
                    world.set_block(other_pos, new_other_state, UpdateFlags::UPDATE_ALL);
                }
            }
        }
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        _world: &dyn ScheduledTickAccess,
        _pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        let facing = state.get_value(FACING);
        let chest_type = state.get_value(CHEST_TYPE);

        if chest_type != ChestType::Single {
            if let Some(connected_dir) = Self::get_connected_direction(facing, chest_type.clone())
            {
                if direction == connected_dir {
                    let expected_other_type = if chest_type == ChestType::Left {
                        ChestType::Right
                    } else {
                        ChestType::Left
                    };
                    if neighbor_state.get_block() != self.block
                        || neighbor_state.get_value(FACING) != facing
                        || neighbor_state.get_value(CHEST_TYPE) != expected_other_type
                    {
                        return state.set_value(CHEST_TYPE, ChestType::Single);
                    }
                }
            }
        }

        state
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
        } else if let Some(connected_dir) =
            Self::get_connected_direction(facing, chest_type.clone())
        {
            let other_pos = pos.relative(connected_dir);
            let other_state = world.get_block_state(other_pos);

            let expected_other_type = if chest_type == ChestType::Left {
                ChestType::Right
            } else {
                ChestType::Left
            };

            if other_state.get_block() == self.block
                && other_state.get_value(FACING) == facing
                && other_state.get_value(CHEST_TYPE) == expected_other_type
            {
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
                        move |context| {
                            double_chest(inventory, context.container_id, first, second)
                        },
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
                &sound_events::BLOCK_CHEST_OPEN
            } else {
                &sound_events::BLOCK_CHEST_CLOSE
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

/// Behavior for trapped chest blocks.
#[block_behavior]
pub struct TrappedChestBlock {
    block: BlockRef,
}

impl TrappedChestBlock {
    /// Creates a new trapped chest block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for TrappedChestBlock {
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

    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source: &PlacementSource<'_>,
    ) {
        let facing = state.get_value(FACING);
        let chest_type = state.get_value(CHEST_TYPE);

        if chest_type != ChestType::Single {
            if let Some(connected_dir) =
                ChestBlock::get_connected_direction(facing, chest_type.clone())
            {
                let other_pos = pos.relative(connected_dir);
                let other_state = world.get_block_state(other_pos);

                if other_state.get_block() == self.block
                    && other_state.get_value(FACING) == facing
                    && other_state.get_value(CHEST_TYPE) == ChestType::Single
                {
                    let expected_other_type = if chest_type == ChestType::Left {
                        ChestType::Right
                    } else {
                        ChestType::Left
                    };
                    let new_other_state =
                        other_state.set_value(CHEST_TYPE, expected_other_type);
                    world.set_block(other_pos, new_other_state, UpdateFlags::UPDATE_ALL);
                }
            }
        }
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        _world: &dyn ScheduledTickAccess,
        _pos: BlockPos,
        direction: Direction,
        _neighbor_pos: BlockPos,
        neighbor_state: BlockStateId,
    ) -> BlockStateId {
        let facing = state.get_value(FACING);
        let chest_type = state.get_value(CHEST_TYPE);

        if chest_type != ChestType::Single {
            if let Some(connected_dir) =
                ChestBlock::get_connected_direction(facing, chest_type.clone())
            {
                if direction == connected_dir {
                    let expected_other_type = if chest_type == ChestType::Left {
                        ChestType::Right
                    } else {
                        ChestType::Left
                    };
                    if neighbor_state.get_block() != self.block
                        || neighbor_state.get_value(FACING) != facing
                        || neighbor_state.get_value(CHEST_TYPE) != expected_other_type
                    {
                        return state.set_value(CHEST_TYPE, ChestType::Single);
                    }
                }
            }
        }

        state
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
        } else if let Some(connected_dir) =
            ChestBlock::get_connected_direction(facing, chest_type.clone())
        {
            let other_pos = pos.relative(connected_dir);
            let other_state = world.get_block_state(other_pos);

            let expected_other_type = if chest_type == ChestType::Left {
                ChestType::Right
            } else {
                ChestType::Left
            };

            if other_state.get_block() == self.block
                && other_state.get_value(FACING) == facing
                && other_state.get_value(CHEST_TYPE) == expected_other_type
            {
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
                        move |context| {
                            double_chest(inventory, context.container_id, first, second)
                        },
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
            &vanilla_block_entity_types::TRAPPED_CHEST,
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
                &sound_events::BLOCK_CHEST_OPEN
            } else {
                &sound_events::BLOCK_CHEST_CLOSE
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

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use steel_registry::item_stack::ItemStack;
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::types::{InteractionHand, UpdateFlags};
    use steel_utils::{BlockPos, ChunkPos};

    use super::*;
    use crate::behavior::{PlacementOrientation, init_behaviors};
    use crate::block_entity::{BlockEntity, entities::ChestBlockEntity, init_block_entities};
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    use crate::test_support::TestPlayerBuilder;

    fn test_player(world: Arc<World>) -> Arc<Player> {
        let player = TestPlayerBuilder::new(world, "TestPlayer", 1).build();
        player.set_client_loaded(true);
        player
    }

    #[test]
    fn chest_opening_and_closing_runs_block_events() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("chest_open_close_test");
        let pos = BlockPos::new(1, 64, 1);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let chest_state = vanilla_blocks::CHEST.default_state();
        assert!(world.set_block(pos, chest_state, UpdateFlags::UPDATE_ALL));

        let block_entity = world
            .get_block_entity(pos)
            .expect("Chest should have a block entity");
        let container_ref = ContainerRef::from_block_entity(Arc::clone(&block_entity))
            .expect("Chest should expose container_ref");

        let player = test_player(Arc::clone(&world));

        // Open chest
        container_ref.start_open(&player);
        world.run_block_events();

        // Close chest
        container_ref.stop_open(&player);
        world.run_block_events();
    }

    #[test]
    fn double_chest_opening_and_closing_runs_block_events() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("double_chest_open_close_test");
        let pos_left = BlockPos::new(2, 64, 2);
        let pos_right = BlockPos::new(3, 64, 2);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos_left));

        let facing = Direction::South;
        let left_state = vanilla_blocks::CHEST
            .default_state()
            .set_value(FACING, facing)
            .set_value(CHEST_TYPE, ChestType::Left);
        let right_state = vanilla_blocks::CHEST
            .default_state()
            .set_value(FACING, facing)
            .set_value(CHEST_TYPE, ChestType::Right);

        assert!(world.set_block(pos_left, left_state, UpdateFlags::UPDATE_ALL));
        assert!(world.set_block(pos_right, right_state, UpdateFlags::UPDATE_ALL));

        let left_entity = world.get_block_entity(pos_left).unwrap();
        let left_ref = ContainerRef::from_block_entity(left_entity).unwrap();

        let player = test_player(Arc::clone(&world));

        left_ref.start_open(&player);
        world.run_block_events();

        left_ref.stop_open(&player);
        world.run_block_events();
    }

    #[test]
    fn barrel_opening_and_closing_updates_open_property() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("barrel_open_close_test");
        let pos = BlockPos::new(5, 64, 5);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let barrel_state = vanilla_blocks::BARREL
            .default_state()
            .set_value(&BlockStateProperties::OPEN, false);
        assert!(world.set_block(pos, barrel_state, UpdateFlags::UPDATE_ALL));

        let block_entity = world.get_block_entity(pos).unwrap();
        let container_ref = ContainerRef::from_block_entity(block_entity).unwrap();
        let player = test_player(Arc::clone(&world));

        container_ref.start_open(&player);
        assert!(
            world.get_block_state(pos).get_value(&BlockStateProperties::OPEN)
        );

        container_ref.stop_open(&player);
        assert!(
            !world.get_block_state(pos).get_value(&BlockStateProperties::OPEN)
        );
    }

    #[test]
    fn ender_chest_and_shulker_box_open_close_run_block_events() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("ender_shulker_open_close_test");
        let ec_pos = BlockPos::new(10, 64, 10);
        let shulker_pos = BlockPos::new(12, 64, 10);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(ec_pos));

        let ec_state = vanilla_blocks::ENDER_CHEST.default_state();
        let shulker_state = vanilla_blocks::SHULKER_BOX.default_state();

        assert!(world.set_block(ec_pos, ec_state, UpdateFlags::UPDATE_ALL));
        assert!(world.set_block(shulker_pos, shulker_state, UpdateFlags::UPDATE_ALL));

        let ec_be = world.get_block_entity(ec_pos).unwrap();
        let shulker_be = world.get_block_entity(shulker_pos).unwrap();
        let shulker_ref = ContainerRef::from_block_entity(shulker_be).unwrap();

        let player = test_player(Arc::clone(&world));

        ec_be.start_open(&player);
        world.run_block_events();

        ec_be.stop_open(&player);
        world.run_block_events();

        shulker_ref.start_open(&player);
        world.run_block_events();

        shulker_ref.stop_open(&player);
        world.run_block_events();
    }

    #[test]
    fn chest_block_entity_creation_and_container() {
        init_vanilla_registry();
        init_block_entities();

        let pos = BlockPos::new(0, 64, 0);
        let state = vanilla_blocks::CHEST.default_state();
        let entity = ChestBlockEntity::new(Weak::new(), pos, state);

        let container_ref = entity
            .container_ref()
            .expect("ChestBlockEntity should provide container_ref");
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        let container = guard
            .get(container_ref.container_id())
            .expect("Container should be locked");

        assert_eq!(container.get_container_size(), 27);
    }

    #[test]
    fn double_chest_placement_and_shape_update() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();

        let world = fresh_test_world("double_chest_test");
        let pos_a = BlockPos::new(10, 64, 10);
        let pos_b = BlockPos::new(11, 64, 10); // East of pos_a
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos_a));

        let facing_south = vanilla_blocks::CHEST
            .default_state()
            .set_value(FACING, Direction::South)
            .set_value(CHEST_TYPE, ChestType::Single);

        assert!(world.set_block(pos_a, facing_south, UpdateFlags::UPDATE_ALL));
        assert_eq!(
            world.get_block_state(pos_a).get_value(CHEST_TYPE),
            ChestType::Single
        );

        let behavior = ChestBlock::new(&vanilla_blocks::CHEST);
        let mut item = ItemStack::empty();
        let source = PlacementSource::direct(
            None,
            InteractionHand::MainHand,
            &mut item,
            PlacementOrientation::Directional {
                direction: Direction::South,
            },
            false,
        );

        let b_state = facing_south.set_value(CHEST_TYPE, ChestType::Left);
        assert!(world.set_block(pos_b, b_state, UpdateFlags::UPDATE_ALL));
        behavior.set_placed_by(b_state, &world, pos_b, &source);

        assert_eq!(
            world.get_block_state(pos_b).get_value(CHEST_TYPE),
            ChestType::Left
        );
        assert_eq!(
            world.get_block_state(pos_a).get_value(CHEST_TYPE),
            ChestType::Right
        );

        assert!(world.set_block(pos_b, vanilla_blocks::AIR.default_state(), UpdateFlags::UPDATE_ALL));

        let updated_a_state = behavior.update_shape(
            world.get_block_state(pos_a),
            &world,
            pos_a,
            Direction::East,
            pos_b,
            vanilla_blocks::AIR.default_state(),
        );
        assert_eq!(updated_a_state.get_value(CHEST_TYPE), ChestType::Single);
    }
}
