//! Comparator-visible composter state.
//!
//! Composting interactions and hopper automation are independent mechanics;
//! comparators read the block-state fill level directly in vanilla.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::Item;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    blocks::{
        BlockRef,
        block_state_ext::BlockStateExt as _,
        properties::{BlockStateProperties, Direction, IntProperty},
    },
    sound_events, vanilla_game_events, vanilla_items,
};
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId};

use crate::{
    behavior::{
        BlockBehavior, BlockHitResult, BlockPlaceContext, InteractionResult, InventoryAccess,
    },
    entity::ai::path::PathComputationType,
    entity::Entity,
    player::Player,
    world::{LevelReader, World, game_event::GameEventContext},
};

/// Vanilla composter behavior required by analog signal readers.
#[block_behavior]
pub struct ComposterBlock {
    block: BlockRef,
}

const LEVEL_COMPOSTER: &IntProperty = &BlockStateProperties::LEVEL_COMPOSTER;

impl ComposterBlock {
    /// Creates composter behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn get_compost_chance(item: &'static Item) -> Option<f32> {
        if item.has_tag(&ItemTag::SAPLINGS)
            || item.has_tag(&ItemTag::LEAVES)
            || item.has_tag(&ItemTag::SMALL_FLOWERS)
            || item == &*vanilla_items::WHEAT_SEEDS
            || item == &*vanilla_items::BEETROOT_SEEDS
            || item == &*vanilla_items::PUMPKIN_SEEDS
            || item == &*vanilla_items::MELON_SEEDS
            || item == &*vanilla_items::TORCHFLOWER_SEEDS
            || item == &*vanilla_items::PITCHER_POD
            || item == &*vanilla_items::DRIED_KELP
            || item == &*vanilla_items::GLOW_LICHEN
            || item == &*vanilla_items::SHORT_GRASS
            || item == &*vanilla_items::FERN
            || item == &*vanilla_items::MOSS_CARPET
        {
            return Some(0.30);
        }

        if item == &*vanilla_items::DRIED_KELP_BLOCK
            || item == &*vanilla_items::TALL_GRASS
            || item == &*vanilla_items::LARGE_FERN
            || item == &*vanilla_items::CACTUS
            || item == &*vanilla_items::SUGAR_CANE
            || item == &*vanilla_items::VINE
            || item == &*vanilla_items::GLOW_BERRIES
            || item == &*vanilla_items::SWEET_BERRIES
            || item == &*vanilla_items::MELON_SLICE
        {
            return Some(0.50);
        }

        if item == &*vanilla_items::APPLE
            || item == &*vanilla_items::BEETROOT
            || item == &*vanilla_items::CARROT
            || item == &*vanilla_items::POTATO
            || item == &*vanilla_items::WHEAT
            || item == &*vanilla_items::BROWN_MUSHROOM
            || item == &*vanilla_items::RED_MUSHROOM
            || item == &*vanilla_items::NETHER_WART
            || item == &*vanilla_items::CRIMSON_FUNGUS
            || item == &*vanilla_items::WARPED_FUNGUS
            || item == &*vanilla_items::PUMPKIN
            || item == &*vanilla_items::CARVED_PUMPKIN
            || item == &*vanilla_items::MELON
            || item == &*vanilla_items::MOSS_BLOCK
        {
            return Some(0.65);
        }

        if item == &*vanilla_items::BAKED_POTATO
            || item == &*vanilla_items::BREAD
            || item == &*vanilla_items::COOKIE
            || item == &*vanilla_items::HAY_BLOCK
        {
            return Some(0.85);
        }

        if item == &*vanilla_items::PUMPKIN_PIE || item == &*vanilla_items::CAKE {
            return Some(1.00);
        }

        None
    }
}

impl BlockBehavior for ComposterBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
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
        i32::from(state.get_value(LEVEL_COMPOSTER))
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }

    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let level = state.get_value(LEVEL_COMPOSTER);
        if level >= 7 {
            return self.use_without_item(state, world, pos, player, hit_result, inv);
        }

        let compost_chance = inv.with_item(|item| {
            if item.is_empty() {
                None
            } else {
                Self::get_compost_chance(item.item())
            }
        });

        let Some(chance) = compost_chance else {
            return InteractionResult::Pass;
        };

        if !player.has_infinite_materials() {
            inv.with_item(|item| {
                item.shrink(1);
            });
        }

        let success = rand::random::<f32>() < chance;
        if success {
            let next_level = level + 1;
            let new_state = state.set_value(LEVEL_COMPOSTER, next_level);
            world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
            world.game_event(
                &vanilla_game_events::BLOCK_CHANGE,
                pos,
                &GameEventContext::new(Some(player), Some(new_state)),
            );

            let sound = if next_level == 7 {
                &sound_events::BLOCK_COMPOSTER_READY
            } else {
                &sound_events::BLOCK_COMPOSTER_FILL_SUCCESS
            };
            world.play_block_sound(sound, pos, 1.0, 1.0, Some(player.id()));
        } else {
            world.play_block_sound(
                &sound_events::BLOCK_COMPOSTER_FILL,
                pos,
                1.0,
                1.0,
                Some(player.id()),
            );
        }

        InteractionResult::Success
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
        let level = state.get_value(LEVEL_COMPOSTER);
        if level < 7 {
            return InteractionResult::Pass;
        }

        let new_state = state.set_value(LEVEL_COMPOSTER, 0);
        world.set_block(pos, new_state, UpdateFlags::UPDATE_ALL);
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(player), Some(new_state)),
        );
        world.play_block_sound(
            &sound_events::BLOCK_COMPOSTER_EMPTY,
            pos,
            1.0,
            1.0,
            Some(player.id()),
        );

        world.pop_resource(pos, ItemStack::new(&vanilla_items::BONE_MEAL));
        InteractionResult::Success
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
    fn registered_composter_outputs_its_full_state_level() {
        init_vanilla_registry();
        init_behaviors();
        let state = vanilla_blocks::COMPOSTER
            .default_state()
            .set_value(LEVEL_COMPOSTER, LEVEL_COMPOSTER.max);
        let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());

        assert!(behavior.has_analog_output_signal(state));
        assert_eq!(
            behavior.get_analog_output_signal(
                state,
                &TestLevel::default(),
                BlockPos::ZERO,
                Direction::North,
            ),
            i32::from(LEVEL_COMPOSTER.max),
        );
    }
}
