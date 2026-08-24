//! State-driven cauldron behavior needed by comparators.
//!
//! Cauldron item interactions and drip filling remain separate block mechanics;
//! this module implements the complete vanilla analog-output contract shared by
//! empty, water, and powder-snow cauldrons.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::Item;
use steel_registry::{
    blocks::{
        BlockRef,
        block_state_ext::BlockStateExt as _,
        properties::{BlockStateProperties, Direction, IntProperty},
    },
    level_events, sound_events, vanilla_blocks, vanilla_fluids, vanilla_game_events, vanilla_items,
};
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId};

use crate::{
    behavior::{
        BlockBehavior, BlockHitResult, BlockPlaceContext, InteractionResult, InventoryAccess,
        blocks::vegetation,
    },
    entity::ai::path::PathComputationType,
    entity::Entity,
    player::Player,
    world::{LevelReader, World, game_event::GameEventContext},
};

/// Vanilla empty cauldron behavior.
#[block_behavior]
pub struct CauldronBlock {
    block: BlockRef,
}

const LEVEL_CAULDRON: &IntProperty = &BlockStateProperties::LEVEL_CAULDRON;

impl CauldronBlock {
    /// Creates empty cauldron behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

fn give_or_swap_item(player: &Player, inv: &mut InventoryAccess, new_item: &'static Item) {
    if player.has_infinite_materials() {
        return;
    }
    inv.with_item(|item| {
        if item.count() == 1 {
            *item = ItemStack::new(new_item);
        } else {
            item.shrink(1);
        }
    });
}

impl BlockBehavior for CauldronBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn use_item_on(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let is_water_bucket = inv.with_item(|item| item.is(&vanilla_items::WATER_BUCKET));
        let is_lava_bucket = inv.with_item(|item| item.is(&vanilla_items::LAVA_BUCKET));
        let is_powder_snow_bucket =
            inv.with_item(|item| item.is(&vanilla_items::POWDER_SNOW_BUCKET));

        if is_water_bucket {
            let new_state = vanilla_blocks::WATER_CAULDRON
                .default_state()
                .set_value(LEVEL_CAULDRON, 3);
            world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
            world.play_block_sound(
                &sound_events::ITEM_BUCKET_EMPTY,
                pos,
                1.0,
                1.0,
                Some(player.id()),
            );
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(Some(player), Some(new_state)),
            );
            give_or_swap_item(player, inv, &vanilla_items::BUCKET);
            return InteractionResult::Success;
        }

        if is_lava_bucket {
            let new_state = vanilla_blocks::LAVA_CAULDRON.default_state();
            world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
            world.play_block_sound(
                &sound_events::ITEM_BUCKET_EMPTY_LAVA,
                pos,
                1.0,
                1.0,
                Some(player.id()),
            );
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(Some(player), Some(new_state)),
            );
            give_or_swap_item(player, inv, &vanilla_items::BUCKET);
            return InteractionResult::Success;
        }

        if is_powder_snow_bucket {
            let new_state = vanilla_blocks::POWDER_SNOW_CAULDRON
                .default_state()
                .set_value(LEVEL_CAULDRON, 3);
            world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
            world.play_block_sound(
                &sound_events::ITEM_BUCKET_EMPTY_POWDER_SNOW,
                pos,
                1.0,
                1.0,
                Some(player.id()),
            );
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(Some(player), Some(new_state)),
            );
            give_or_swap_item(player, inv, &vanilla_items::BUCKET);
            return InteractionResult::Success;
        }

        InteractionResult::Pass
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        0
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }

    fn tick(&self, _state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let Some(stalactite_pos) =
            vegetation::find_stalactite_tip_above_cauldron(world.as_ref(), pos)
        else {
            return;
        };
        let Some(fluid) = vegetation::get_cauldron_fill_fluid_type(world, stalactite_pos) else {
            return;
        };

        if fluid == &vanilla_fluids::WATER {
            let new_state = vanilla_blocks::WATER_CAULDRON
                .default_state()
                .set_value(LEVEL_CAULDRON, 1);
            world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(None, Some(new_state)),
            );
            world.level_event(level_events::SOUND_DRIP_WATER_INTO_CAULDRON, pos, 0, None);
        } else if fluid == &vanilla_fluids::LAVA {
            let new_state = vanilla_blocks::LAVA_CAULDRON.default_state();
            world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(None, Some(new_state)),
            );
            world.level_event(level_events::SOUND_DRIP_LAVA_INTO_CAULDRON, pos, 0, None);
        }
    }
}

/// Vanilla layered water and powder-snow cauldron behavior.
#[block_behavior]
pub struct LayeredCauldronBlock {
    block: BlockRef,
}

impl LayeredCauldronBlock {
    /// Creates layered cauldron behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for LayeredCauldronBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let level = state.get_value(LEVEL_CAULDRON);
        let is_water = self.block == &vanilla_blocks::WATER_CAULDRON;
        let is_powder_snow = self.block == &vanilla_blocks::POWDER_SNOW_CAULDRON;

        let is_bucket = inv.with_item(|item| item.is(&vanilla_items::BUCKET));
        let is_glass_bottle = inv.with_item(|item| item.is(&vanilla_items::GLASS_BOTTLE));

        if is_bucket && level == 3 {
            let (new_bucket, sound) = if is_water {
                (&vanilla_items::WATER_BUCKET, &sound_events::ITEM_BUCKET_FILL)
            } else if is_powder_snow {
                (
                    &vanilla_items::POWDER_SNOW_BUCKET,
                    &sound_events::ITEM_BUCKET_FILL_POWDER_SNOW,
                )
            } else {
                return InteractionResult::Pass;
            };

            let empty_state = vanilla_blocks::CAULDRON.default_state();
            world.set_block(pos, empty_state, UpdateFlags::UPDATE_ALL);
            world.play_block_sound(sound, pos, 1.0, 1.0, Some(player.id()));
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(Some(player), Some(empty_state)),
            );
            give_or_swap_item(player, inv, new_bucket);
            return InteractionResult::Success;
        }

        if is_water && is_glass_bottle && level > 0 {
            let next_level = level - 1;
            let new_state = if next_level == 0 {
                vanilla_blocks::CAULDRON.default_state()
            } else {
                state.set_value(LEVEL_CAULDRON, next_level)
            };

            world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
            world.play_block_sound(
                &sound_events::ITEM_BOTTLE_FILL,
                pos,
                1.0,
                1.0,
                Some(player.id()),
            );
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(Some(player), Some(new_state)),
            );
            give_or_swap_item(player, inv, &vanilla_items::POTION);
            return InteractionResult::Success;
        }

        InteractionResult::Pass
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        i32::from(state.get_value(LEVEL_CAULDRON))
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let Some(stalactite_pos) =
            vegetation::find_stalactite_tip_above_cauldron(world.as_ref(), pos)
        else {
            return;
        };
        let Some(fluid) = vegetation::get_cauldron_fill_fluid_type(world, stalactite_pos) else {
            return;
        };

        if fluid == &vanilla_fluids::WATER {
            let level = state.get_value(LEVEL_CAULDRON);
            if level < LEVEL_CAULDRON.max {
                let new_state = state.set_value(LEVEL_CAULDRON, level + 1);
                world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
                world.game_event(
                    &vanilla_game_events::BLOCK_CHANGE,
                    pos,
                    &GameEventContext::new(None, Some(new_state)),
                );
                world.level_event(level_events::SOUND_DRIP_WATER_INTO_CAULDRON, pos, 0, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;
    use crate::{
        behavior::{BLOCK_BEHAVIORS, init_behaviors},
        test_support::TestLevel,
    };

    #[test]
    fn registered_cauldron_behaviors_expose_vanilla_fill_levels() {
        init_vanilla_registry();
        init_behaviors();
        let level = TestLevel::default();
        let pos = BlockPos::ZERO;

        let empty = vanilla_blocks::CAULDRON.default_state();
        let empty_behavior = BLOCK_BEHAVIORS.get_behavior(empty.get_block());
        assert!(empty_behavior.has_analog_output_signal(empty));
        assert_eq!(
            empty_behavior.get_analog_output_signal(empty, &level, pos, Direction::North),
            0,
        );

        for level_value in 1..=3 {
            let state = vanilla_blocks::WATER_CAULDRON
                .default_state()
                .set_value(LEVEL_CAULDRON, level_value);
            let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());
            assert_eq!(
                behavior.get_analog_output_signal(state, &level, pos, Direction::North),
                i32::from(level_value),
            );
        }
    }
}
