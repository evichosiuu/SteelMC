//! Bucket item behavior implementation.
//!
//! Handles water buckets, lava buckets, and empty buckets.
//!
//! Mirrors vanilla's `BucketItem(Fluid fluid)`: `fluid_block = None` = empty bucket,
//! `Some(block)` = filled bucket. Logic is dispatched in `use_item`.
//!
use crate::behavior::context::InteractionResult;
use crate::behavior::item_utils::create_filled_result;
use crate::behavior::{
    BLOCK_BEHAVIORS, BlockStateBehaviorExt, FLUID_BEHAVIORS, ItemBehavior, UseItemContext,
    pickup_waterlogged_block,
};
use crate::fluid::FluidStateExt;
use crate::world::RaytraceAction;
use steel_macros::item_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::Direction;
use steel_registry::fluid::FluidState;
use steel_registry::item_stack::ItemStack;
use steel_registry::level_events;
use steel_registry::sound_events;
use steel_registry::vanilla_blocks;
use steel_registry::vanilla_fluids;
use steel_registry::vanilla_entities;
use steel_registry::vanilla_game_events;
use steel_registry::vanilla_items;
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::context::UseOnContext;
use crate::entity::{
    ENTITIES, Entity, EntitySpawnReason, LivingEntity, RemovalReason, next_entity_id,
};
use crate::player::Player;
use crate::world::game_event::GameEventContext;

/// Handles all bucket variants (empty, water, lava).
#[item_behavior]
pub struct BucketItem {
    #[json_arg(vanilla_blocks, json = "content", optional = "empty")]
    fluid_block: Option<BlockRef>,
}

impl BucketItem {
    /// Creates a new bucket behavior. `None` = empty bucket, `Some(block)` = filled.
    #[must_use]
    pub const fn new(fluid_block: Option<BlockRef>) -> Self {
        Self { fluid_block }
    }
}

impl ItemBehavior for BucketItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        match self.fluid_block {
            None => use_empty_bucket(context),
            Some(fluid_block) => use_filled_bucket(fluid_block, context),
        }
    }

    fn interact_living_entity(
        &self,
        stack: &mut ItemStack,
        player: &Player,
        target: &dyn LivingEntity,
        hand: InteractionHand,
    ) -> InteractionResult {
        if self.fluid_block.is_some() {
            return InteractionResult::Pass;
        }

        let target_type = target.entity_type();

        if (target_type == &vanilla_entities::COW || target_type == &vanilla_entities::MOOSHROOM)
            && !target.is_baby()
        {
            if let Some(world) = target.level() {
                world.play_sound_at(
                    &sound_events::ENTITY_COW_MILK,
                    steel_protocol::packets::game::SoundSource::Neutral,
                    target.position(),
                    1.0,
                    1.0,
                    None,
                );
            }
            if !player.has_infinite_materials() {
                stack.shrink(1);
                let milk = ItemStack::new(&vanilla_items::MILK_BUCKET);
                if stack.is_empty() {
                    player.inventory.lock().set_item_in_hand(hand, milk);
                } else {
                    player.add_item_or_drop(milk);
                }
            }
            return InteractionResult::Success;
        }

        let (bucket_item, sound) = if target_type == &vanilla_entities::COD {
            (&vanilla_items::COD_BUCKET, &sound_events::ITEM_BUCKET_FILL_FISH)
        } else if target_type == &vanilla_entities::SALMON {
            (&vanilla_items::SALMON_BUCKET, &sound_events::ITEM_BUCKET_FILL_FISH)
        } else if target_type == &vanilla_entities::PUFFERFISH {
            (&vanilla_items::PUFFERFISH_BUCKET, &sound_events::ITEM_BUCKET_FILL_FISH)
        } else if target_type == &vanilla_entities::TROPICAL_FISH {
            (&vanilla_items::TROPICAL_FISH_BUCKET, &sound_events::ITEM_BUCKET_FILL_FISH)
        } else if target_type == &vanilla_entities::AXOLOTL {
            (&vanilla_items::AXOLOTL_BUCKET, &sound_events::ITEM_BUCKET_FILL_AXOLOTL)
        } else if target_type == &vanilla_entities::TADPOLE {
            (&vanilla_items::TADPOLE_BUCKET, &sound_events::ITEM_BUCKET_FILL_TADPOLE)
        } else {
            return InteractionResult::Pass;
        };

        if let Some(world) = target.level() {
            world.play_sound_at(
                sound,
                steel_protocol::packets::game::SoundSource::Neutral,
                target.position(),
                1.0,
                1.0,
                None,
            );
        }
        target.set_removed(RemovalReason::Discarded);

        if !player.has_infinite_materials() {
            stack.shrink(1);
            let filled = ItemStack::new(bucket_item);
            if stack.is_empty() {
                player.inventory.lock().set_item_in_hand(hand, filled);
            } else {
                player.add_item_or_drop(filled);
            }
        }

        InteractionResult::Success
    }
}

fn filled_bucket_success_stack(context: &UseItemContext) -> ItemStack {
    if context.player.has_infinite_materials() {
        context
            .inv
            .with_item(|item| item.copy_with_count(item.count()))
    } else {
        ItemStack::new(&vanilla_items::BUCKET)
    }
}

fn use_empty_bucket(context: &mut UseItemContext) -> InteractionResult {
    let (start, end) = context.player.get_ray_endpoints();

    // Raytrace: stop on source fluids
    let (hit_block, _) = context.world.raytrace(start, end, |pos, world| {
        let state = world.get_block_state(pos);
        let block = state.get_block();

        if block == &vanilla_blocks::AIR {
            return RaytraceAction::Pass;
        }

        let fluid_state = state.get_fluid_state();
        if fluid_state.is_source() {
            return RaytraceAction::ImmediateHit;
        }
        // Vanilla parity: ClipContext.Fluid.SOURCE_ONLY — flowing fluid is transparent.
        if !fluid_state.is_empty() {
            return RaytraceAction::Pass;
        }

        RaytraceAction::CheckShape
    });

    // Vanilla returns PASS when raytrace misses (allows other handlers to try)
    let Some(hit_pos) = hit_block else {
        return InteractionResult::Pass;
    };

    let hit_state = context.world.get_block_state(hit_pos);

    if hit_state.get_block() == &vanilla_blocks::POWDER_SNOW {
        context.world.set_block(
            hit_pos,
            vanilla_blocks::AIR.default_state(),
            UpdateFlags::UPDATE_ALL_IMMEDIATE,
        );
        context.world.play_block_sound(
            &sound_events::ITEM_BUCKET_FILL_POWDER_SNOW,
            hit_pos,
            1.0,
            1.0,
            Some(context.player.id()),
        );
        create_filled_result(context, ItemStack::new(&vanilla_items::POWDER_SNOW_BUCKET), true);
        return InteractionResult::Success;
    }

    let block_behavior = BLOCK_BEHAVIORS.get_behavior(hit_state.get_block());

    if let Some(result) =
        block_behavior.pickup_block(context.world, hit_pos, hit_state, Some(context.player))
    {
        // Apply sound
        if let Some(sound) = result.sound {
            context
                .world
                .play_block_sound(sound, hit_pos, 1.0, 1.0, None);
        }

        // Give filled bucket
        create_filled_result(context, result.filled_bucket, true);
        context.world.game_event(
            &vanilla_game_events::FLUID_PICKUP,
            hit_pos,
            &GameEventContext::new(Some(context.player), None),
        );

        return InteractionResult::Success;
    }

    // TODO: Remove fallback once all waterloggable blocks implement pickup_block.
    if let Some(result) = pickup_waterlogged_block(
        block_behavior,
        context.world,
        hit_pos,
        hit_state,
        Some(context.player),
    ) {
        if let Some(sound) = result.sound {
            context
                .world
                .play_block_sound(sound, hit_pos, 1.0, 1.0, None);
        }

        create_filled_result(context, result.filled_bucket, true);
        context.world.game_event(
            &vanilla_game_events::FLUID_PICKUP,
            hit_pos,
            &GameEventContext::new(Some(context.player), None),
        );

        return InteractionResult::Success;
    }

    // Nothing was picked up — no fluid source block and no waterlogged block found.
    // Vanilla returns FAIL here so the client knows no item change occurred.
    InteractionResult::Fail
}

// TODO: Refactor into smaller helpers once all bucket types are implemented
#[expect(
    clippy::too_many_lines,
    reason = "mirrors vanilla's emptyContents flow; splitting would obscure the sequential placement logic"
)]
fn use_filled_bucket(fluid_block: BlockRef, context: &mut UseItemContext) -> InteractionResult {
    // Raytrace to find target block
    let (start, end) = context.player.get_ray_endpoints();
    let (ray_block, ray_dir) = context.world.raytrace(start, end, |pos, world| {
        let state = world.get_block_state(pos);
        let block = state.get_block();
        // Filled buckets use ClipContext.Fluid.NONE: ignore fluid shapes, but
        // still test the block shape of waterlogged/container blocks.
        if block == &vanilla_blocks::AIR {
            return RaytraceAction::Pass;
        }
        RaytraceAction::CheckShape
    });

    // Vanilla returns PASS when raytrace misses (allows other handlers to try)
    let (Some(clicked_pos), Some(direction)) = (ray_block, ray_dir) else {
        return InteractionResult::Pass;
    };

    // If the block is out of bounds, return fail
    if !context.world.is_in_valid_bounds(clicked_pos) {
        return InteractionResult::Fail;
    }

    let clicked_state = context.world.get_block_state(clicked_pos);
    let is_sneaking = context.player.is_crouching();

    // Define fluid placement logic as a closure to reuse for primary/secondary targets.
    // `check_sneak`: true for primary attempt, false for secondary (vanilla parity:
    // recursive emptyContents passes hitResult=null for fallback, bypassing sneak check).
    let try_place_fluid = |pos: BlockPos, check_sneak: bool| -> bool {
        if !context.world.is_in_valid_bounds(pos) {
            return false;
        }

        let state = context.world.get_block_state(pos);
        let fluid_state = state.get_fluid_state();

        // Vanilla parity (bl4): when sneaking, only air allows placement at this position.
        // Non-air blocks redirect to the neighbor — handled by the secondary call.
        // The secondary call bypasses this check (hitResult == null in vanilla).
        if check_sneak && is_sneaking && !state.get_block().config.is_air {
            return false;
        }

        let is_water_bucket = fluid_block == &vanilla_blocks::WATER;
        let behavior = BLOCK_BEHAVIORS.get_behavior(state.get_block());
        let is_liquid_container = state.is_liquid_container();
        let can_place_liquid = is_water_bucket
            && is_liquid_container
            && behavior.can_place_liquid_with_player(
                state,
                FluidState::source(&vanilla_fluids::WATER).fluid_id,
                Some(context.player),
            );
        let can_replace = state.can_be_replaced_by_fluid(fluid_block);

        // Vanilla parity: block must be replaceable or liquid-container-admissible for placement.
        if !can_replace && !can_place_liquid {
            return false;
        }

        // Vanilla parity: in worlds where water evaporates (e.g. the Nether),
        // water buckets fizz out without placing any fluid.
        // TODO: Per-position environment attributes (vanilla uses EnvironmentAttributes.WATER_EVAPORATES per-pos)
        if is_water_bucket && context.world.dimension_type.water_evaporates {
            context
                .world
                .level_event(level_events::PARTICLES_WATER_EVAPORATING, pos, 0, None);
            return true;
        }

        // 1. Try LiquidBlockContainer handling (only if Water bucket).
        if is_water_bucket && is_liquid_container {
            let source_water = FluidState::source(&vanilla_fluids::WATER);
            behavior.place_liquid(context.world, pos, state, source_water);
            play_empty_sound_and_event(context, pos, true);
            return true;
        }

        // 2. Try Standard Placement (Replaceable block)
        if can_replace {
            // If same fluid already exists and is source, just consume bucket (parity)
            let is_same_fluid = if is_water_bucket {
                fluid_state.is_water()
            } else {
                fluid_state.is_lava()
            };

            if is_same_fluid && fluid_state.is_source() {
                play_empty_sound_and_event(context, pos, is_water_bucket);
                return true;
            }

            // Vanilla parity: destroy non-liquid replaceable blocks first so they
            // drop their items (e.g. tall grass, flowers, snow layers).
            if !state.get_block().config.liquid && !state.get_block().config.is_air {
                context.player.get_world().destroy_block(pos, true);
            }

            // Place fluid block
            let fluid_state_to_place = fluid_block.default_state();
            if context
                .world
                .set_block(pos, fluid_state_to_place, UpdateFlags::UPDATE_ALL_IMMEDIATE)
            {
                let fluid_ref = if is_water_bucket {
                    &vanilla_fluids::WATER
                } else {
                    &vanilla_fluids::LAVA
                };
                let tick_delay = FLUID_BEHAVIORS
                    .get_behavior(fluid_ref)
                    .tick_delay(context.world);
                context
                    .world
                    .schedule_fluid_tick_default(pos, fluid_ref, tick_delay);

                play_empty_sound_and_event(context, pos, is_water_bucket);

                return true;
            }
        }
        false
    };

    // Vanilla parity (BucketItem.java): position selection mirrors
    // `instanceof LiquidBlockContainer && content == Fluids.WATER ? pos : directionOffsetPos`.
    // If primary fails, secondary retries at the offset pos without sneak check,
    // matching vanilla's recursive `emptyContents(hitResult=null)` fallback.
    let is_water_bucket = fluid_block == &vanilla_blocks::WATER;
    let primary_pos =
        filled_bucket_primary_pos(clicked_state, clicked_pos, direction, is_water_bucket);

    // Attempt Primary (with sneak check)
    if try_place_fluid(primary_pos, true) {
        let result_stack = filled_bucket_success_stack(context);
        create_filled_result(context, result_stack, true);
        return InteractionResult::Success;
    }

    // Attempt Secondary (Fallback — no sneak check, matching vanilla hitResult=null).
    // Vanilla's emptyContents always recurses with hitResult=null at the offset position
    // when the primary attempt fails, regardless of bucket type.
    let secondary_pos = direction.relative(clicked_pos);
    if try_place_fluid(secondary_pos, false) {
        let result_stack = filled_bucket_success_stack(context);
        create_filled_result(context, result_stack, true);
        return InteractionResult::Success;
    }

    InteractionResult::Fail
}

fn play_empty_sound_and_event(context: &UseItemContext, pos: BlockPos, is_water_bucket: bool) {
    let sound_event = if is_water_bucket {
        &sound_events::ITEM_BUCKET_EMPTY
    } else {
        &sound_events::ITEM_BUCKET_EMPTY_LAVA
    };
    context
        .world
        .play_block_sound(sound_event, pos, 1.0, 1.0, None);
    context.world.game_event(
        &vanilla_game_events::FLUID_PLACE,
        pos,
        &GameEventContext::new(Some(context.player), None),
    );
}

fn filled_bucket_primary_pos(
    clicked_state: BlockStateId,
    clicked_pos: BlockPos,
    direction: Direction,
    is_water_bucket: bool,
) -> BlockPos {
    if is_water_bucket && clicked_state.is_liquid_container() {
        clicked_pos
    } else {
        direction.relative(clicked_pos)
    }
}

/// Behavior for solid bucket items like powder snow bucket.
#[item_behavior]
pub struct SolidBucketItem;

impl ItemBehavior for SolidBucketItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let clicked_pos = context.hit_result.block_pos;
        let clicked_state = context.world.get_block_state(clicked_pos);
        let place_pos = if clicked_state.can_be_replaced_by_fluid(&vanilla_blocks::POWDER_SNOW) {
            clicked_pos
        } else {
            context.hit_result.direction.relative(clicked_pos)
        };

        if !context.world.is_in_valid_bounds(place_pos) {
            return InteractionResult::Fail;
        }

        let place_state = context.world.get_block_state(place_pos);
        if !place_state.can_be_replaced_by_fluid(&vanilla_blocks::POWDER_SNOW) {
            return InteractionResult::Fail;
        }

        context.world.set_block(
            place_pos,
            vanilla_blocks::POWDER_SNOW.default_state(),
            UpdateFlags::UPDATE_ALL_IMMEDIATE,
        );
        context.world.play_block_sound(
            &sound_events::ITEM_BUCKET_EMPTY_POWDER_SNOW,
            place_pos,
            1.0,
            1.0,
            Some(context.player.id()),
        );

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| {
                *item = ItemStack::new(&vanilla_items::BUCKET);
            });
        }

        InteractionResult::Success
    }
}

/// Behavior for mob buckets (cod, salmon, axolotl, tadpole, etc.).
#[item_behavior]
pub struct MobBucketItem;

impl ItemBehavior for MobBucketItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let clicked_pos = context.hit_result.block_pos;
        let clicked_state = context.world.get_block_state(clicked_pos);
        let place_pos = if clicked_state.can_be_replaced_by_fluid(&vanilla_blocks::WATER) {
            clicked_pos
        } else {
            context.hit_result.direction.relative(clicked_pos)
        };

        if !context.world.is_in_valid_bounds(place_pos) {
            return InteractionResult::Fail;
        }

        let item_key = context.inv.with_item(|item| item.item().key.clone());

        let (entity_type, sound) = if item_key == vanilla_items::COD_BUCKET.key {
            (&vanilla_entities::COD, &sound_events::ITEM_BUCKET_EMPTY_FISH)
        } else if item_key == vanilla_items::SALMON_BUCKET.key {
            (&vanilla_entities::SALMON, &sound_events::ITEM_BUCKET_EMPTY_FISH)
        } else if item_key == vanilla_items::PUFFERFISH_BUCKET.key {
            (&vanilla_entities::PUFFERFISH, &sound_events::ITEM_BUCKET_EMPTY_FISH)
        } else if item_key == vanilla_items::TROPICAL_FISH_BUCKET.key {
            (&vanilla_entities::TROPICAL_FISH, &sound_events::ITEM_BUCKET_EMPTY_FISH)
        } else if item_key == vanilla_items::AXOLOTL_BUCKET.key {
            (&vanilla_entities::AXOLOTL, &sound_events::ITEM_BUCKET_EMPTY_AXOLOTL)
        } else if item_key == vanilla_items::TADPOLE_BUCKET.key {
            (&vanilla_entities::TADPOLE, &sound_events::ITEM_BUCKET_EMPTY_TADPOLE)
        } else {
            (&vanilla_entities::COD, &sound_events::ITEM_BUCKET_EMPTY_FISH)
        };

        let state = context.world.get_block_state(place_pos);
        if state.can_be_replaced_by_fluid(&vanilla_blocks::WATER) {
            context.world.set_block(
                place_pos,
                vanilla_blocks::WATER.default_state(),
                UpdateFlags::UPDATE_ALL_IMMEDIATE,
            );
        }

        let spawn_pos = glam::DVec3::new(
            f64::from(place_pos.x()) + 0.5,
            f64::from(place_pos.y()) + 0.1,
            f64::from(place_pos.z()) + 0.5,
        );
        if let Some(mob) = ENTITIES.create(
            entity_type,
            next_entity_id(),
            spawn_pos,
            std::sync::Arc::downgrade(context.world),
        ) {
            if let Some(mob_base) = mob.as_mob() {
                mob_base.finalize_spawn(context.world, EntitySpawnReason::Bucket, None);
            }
            let _ = context.world.try_add_entity(mob);
        }

        context.world.play_block_sound(
            sound,
            place_pos,
            1.0,
            1.0,
            Some(context.player.id()),
        );

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| {
                *item = ItemStack::new(&vanilla_items::BUCKET);
            });
        }

        InteractionResult::Success
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities, vanilla_items};
    use steel_utils::types::InteractionHand;
    use crate::behavior::init_behaviors;
    use crate::entity::init_entities;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    use super::*;

    #[test]
    fn filled_water_bucket_targets_non_waterlogged_liquid_container_in_place() {
        init_vanilla_registry();
        init_behaviors();

        let kelp = vanilla_blocks::KELP.default_state();

        assert_eq!(
            filled_bucket_primary_pos(kelp, BlockPos::ZERO, Direction::North, true),
            BlockPos::ZERO
        );
        assert_eq!(
            filled_bucket_primary_pos(kelp, BlockPos::ZERO, Direction::North, false),
            Direction::North.relative(BlockPos::ZERO)
        );
    }

    #[test]
    fn empty_bucket_milks_cow_giving_milk_bucket() {
        init_vanilla_registry();
        init_behaviors();
        init_entities();

        let world = fresh_test_world("empty_bucket_milks_cow");
        insert_ready_full_chunk(&world, steel_utils::ChunkPos::new(0, 0));

        let cow = crate::entity::entities::CowEntity::new(
            &vanilla_entities::COW,
            next_entity_id(),
            DVec3::new(1.0, 64.0, 1.0),
            Arc::downgrade(&world),
        );

        let mut stack = ItemStack::new(&vanilla_items::BUCKET);
        let player = TestPlayerBuilder::new(world.clone(), "player", 1).build();

        let behavior = BucketItem::new(None);
        let result = behavior.interact_living_entity(
            &mut stack,
            &player,
            &cow,
            InteractionHand::MainHand,
        );

        assert_eq!(result, InteractionResult::Success);
        let inv = player.inventory.lock();
        let in_hand = inv.get_item_in_hand(InteractionHand::MainHand);
        assert!(in_hand.is(&vanilla_items::MILK_BUCKET));
    }
}
