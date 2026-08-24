use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::{
    BlockRef,
    block_state_ext::BlockStateExt,
    properties::{BlockStateProperties, BoolProperty, Direction, IntProperty},
    shapes::VoxelShape,
};
use steel_registry::vanilla_blocks;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockLocalAabb, BlockPos, BlockStateId};

use crate::behavior::{
    BlockBehavior, BlockCollisionContext, BlockPlaceContext,
    block::schedule_water_tick_if_waterlogged,
};
use crate::world::{LevelReader, ScheduledTickAccess, World};

const SHAPE_STABLE_BOXES: &[BlockLocalAabb] = &[
    BlockLocalAabb::new(0.0, 0.875, 0.0, 1.0, 1.0, 1.0),
    BlockLocalAabb::new(0.0, 0.0, 0.0, 0.125, 1.0, 0.125),
    BlockLocalAabb::new(0.875, 0.0, 0.0, 1.0, 1.0, 0.125),
    BlockLocalAabb::new(0.0, 0.0, 0.875, 0.125, 1.0, 1.0),
    BlockLocalAabb::new(0.875, 0.0, 0.875, 1.0, 1.0, 1.0),
];
const SHAPE_UNSTABLE_BOTTOM_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, 0.0, 0.0, 1.0, 0.125, 1.0)];
const SHAPE_BELOW_BLOCK_BOXES: &[BlockLocalAabb] =
    &[BlockLocalAabb::new(0.0, -1.0, 0.0, 1.0, 0.0, 1.0)];

const SHAPE_STABLE: VoxelShape = VoxelShape::from_boxes(SHAPE_STABLE_BOXES);
const SHAPE_UNSTABLE_BOTTOM: VoxelShape = VoxelShape::from_boxes(SHAPE_UNSTABLE_BOTTOM_BOXES);
const SHAPE_BELOW_BLOCK: VoxelShape = VoxelShape::from_boxes(SHAPE_BELOW_BLOCK_BOXES);

/// Vanilla scaffolding collision-shape behavior.
///
/// TODO: Add vanilla placement, stability distance updates, falling conversion, and waterlogging.
#[block_behavior]
pub struct ScaffoldingBlock {
    block: BlockRef,
}

const BOTTOM: &BoolProperty = &BlockStateProperties::BOTTOM;
const STABILITY_DISTANCE: &IntProperty = &BlockStateProperties::STABILITY_DISTANCE;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

impl ScaffoldingBlock {
    /// Creates a scaffolding block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn calculate_distance(world: &dyn LevelReader, pos: BlockPos) -> u8 {
        let below_pos = pos.below();
        let below_state = world.get_block_state(below_pos);

        if below_state.get_block() == &vanilla_blocks::SCAFFOLDING {
            return below_state.get_value(STABILITY_DISTANCE);
        }

        if world.is_face_sturdy(below_state, below_pos, Direction::Up) {
            return 0;
        }

        let mut min_neighbor_dist = 7u8;
        for dir in Direction::HORIZONTAL {
            let neighbor_pos = pos.relative(dir);
            let neighbor_state = world.get_block_state(neighbor_pos);
            if neighbor_state.get_block() == &vanilla_blocks::SCAFFOLDING {
                let dist = neighbor_state.get_value(STABILITY_DISTANCE);
                if dist < min_neighbor_dist {
                    min_neighbor_dist = dist;
                }
            }
        }

        (min_neighbor_dist + 1).min(7)
    }
}

impl BlockBehavior for ScaffoldingBlock {
    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        schedule_water_tick_if_waterlogged(state, world, pos);
        world.schedule_block_tick_default(pos, self.block, 1);
        state
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        world.schedule_block_tick_default(pos, self.block, 1);
    }

    fn handle_neighbor_changed(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        world.schedule_block_tick_default(pos, self.block, 1);
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        let current_dist = state.get_value(STABILITY_DISTANCE);
        let target_dist = Self::calculate_distance(world.as_ref(), pos);
        let below_state = world.get_block_state(pos.below());
        let bottom = below_state.get_block() != &vanilla_blocks::SCAFFOLDING
            && target_dist != 0;

        if target_dist == 7 {
            world.destroy_block(pos, true);
        } else if current_dist != target_dist || state.get_value(BOTTOM) != bottom {
            world.set_block(
                pos,
                state
                    .set_value(STABILITY_DISTANCE, target_dist)
                    .set_value(BOTTOM, bottom),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let pos = context.place_pos();
        let distance = Self::calculate_distance(context.world, pos);
        let waterlogged = context.is_water_source();
        let below_state = context.world.get_block_state(pos.below());
        let bottom = below_state.get_block() != &vanilla_blocks::SCAFFOLDING
            && distance != 0;

        Some(
            self.block
                .default_state()
                .set_value(STABILITY_DISTANCE, distance)
                .set_value(BOTTOM, bottom)
                .set_value(WATERLOGGED, waterlogged),
        )
    }

    fn get_collision_shape(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        pos: BlockPos,
        context: BlockCollisionContext,
    ) -> VoxelShape {
        if context.is_placement() {
            return VoxelShape::EMPTY;
        }

        if context.is_above(VoxelShape::FULL_BLOCK, pos, true) && !context.is_descending() {
            return SHAPE_STABLE;
        }

        let distance = state.get_value(STABILITY_DISTANCE);
        let bottom = state.get_value(BOTTOM);
        if distance != 0 && bottom && context.is_above(SHAPE_BELOW_BLOCK, pos, true) {
            SHAPE_UNSTABLE_BOTTOM
        } else {
            VoxelShape::EMPTY
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_fluids};

    use crate::test_support::TestLevel;

    const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

    fn scaffolding_state(distance: u8, bottom: bool) -> BlockStateId {
        vanilla_blocks::SCAFFOLDING
            .default_state()
            .set_value(STABILITY_DISTANCE, distance)
            .set_value(BOTTOM, bottom)
    }

    fn collision_shape(state: BlockStateId, context: BlockCollisionContext) -> VoxelShape {
        let behavior = ScaffoldingBlock::new(&vanilla_blocks::SCAFFOLDING);
        let level = TestLevel::default().with_min_y(0);
        behavior.get_collision_shape(state, &level, BlockPos::new(0, 64, 0), context)
    }

    #[test]
    fn placement_context_has_no_scaffolding_collision() {
        init_vanilla_registry();

        let shape = collision_shape(
            scaffolding_state(0, false),
            BlockCollisionContext::pre_move(65.0, false),
        );

        assert_eq!(shape, VoxelShape::EMPTY);
    }

    #[test]
    fn entity_above_scaffolding_collides_with_stable_shape() {
        init_vanilla_registry();

        let shape = collision_shape(
            scaffolding_state(0, false),
            BlockCollisionContext::entity(65.0, false),
        );

        assert_eq!(shape, SHAPE_STABLE);
    }

    #[test]
    fn descending_entity_only_collides_with_unstable_bottom_shape() {
        init_vanilla_registry();

        let shape = collision_shape(
            scaffolding_state(1, true),
            BlockCollisionContext::entity(64.5, true),
        );

        assert_eq!(shape, SHAPE_UNSTABLE_BOTTOM);
    }

    #[test]
    fn non_bottom_descending_scaffolding_has_empty_collision() {
        init_vanilla_registry();

        let shape = collision_shape(
            scaffolding_state(1, false),
            BlockCollisionContext::entity(64.5, true),
        );

        assert_eq!(shape, VoxelShape::EMPTY);
    }

    #[test]
    fn shape_update_schedules_stability_and_water_ticks() {
        init_vanilla_registry();
        let behavior = ScaffoldingBlock::new(&vanilla_blocks::SCAFFOLDING);
        let state = vanilla_blocks::SCAFFOLDING
            .default_state()
            .set_value(WATERLOGGED, true);
        let pos = BlockPos::new(0, 64, 0);
        let level = TestLevel::default();

        assert_eq!(
            behavior.update_shape(
                state,
                &level,
                pos,
                Direction::North,
                pos.north(),
                vanilla_blocks::AIR.default_state(),
            ),
            state
        );
        assert_eq!(
            level
                .scheduled_block_ticks
                .borrow()
                .iter()
                .map(|tick| (tick.pos, tick.block, tick.delay))
                .collect::<Vec<_>>(),
            vec![(pos, &vanilla_blocks::SCAFFOLDING, 1)]
        );
        assert_eq!(
            level
                .scheduled_fluid_ticks
                .borrow()
                .iter()
                .map(|tick| (tick.pos, tick.fluid, tick.delay))
                .collect::<Vec<_>>(),
            vec![(pos, &vanilla_fluids::WATER, 5)]
        );
    }
}
