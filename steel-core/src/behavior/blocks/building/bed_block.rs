use std::sync::Arc;

use crate::{
    behavior::{
        BlockBehavior, BlockHitResult, BlockPlaceContext, BlockStateBehaviorExt as _,
        EntityFallDamage, EntityFallOnContext, EntityLandingContext, InteractionResult,
        InventoryAccess, PlacementSource,
    },
    entity::{Entity, ai::path::PathComputationType, damage::DamageSource, dismount_helper},
    player::Player,
    world::{ScheduledTickAccess, World, explosion::ExplosionBlockInteraction},
};
use glam::DVec3;
use steel_macros::block_behavior;
use steel_registry::blocks::properties::{BedPart, BoolProperty, EnumProperty};
use steel_registry::blocks::{
    BlockRef, block_state_ext::BlockStateExt, properties::BlockStateProperties,
};
use steel_registry::{vanilla_blocks, vanilla_damage_types};
use steel_utils::{BlockPos, BlockStateId, Direction, types::UpdateFlags};
use text_components::TextComponent;
use text_components::translation::TranslatedMessage;

const BED_BOUNCE_SCALE: f64 = 0.660_000_026_226_043_7;
const BED_PART: &EnumProperty<BedPart> = &BlockStateProperties::BED_PART;
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const OCCUPIED: &BoolProperty = &BlockStateProperties::OCCUPIED;
/// Behavior for beds
///
/// TODO: Mirror vanilla `BedBlock.kickVillagerOutOfBed` once villager sleeping
/// entities exist.
#[block_behavior]
pub struct BedBlock {
    block: BlockRef,
}

impl BedBlock {
    /// Creates a bed block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    #[must_use]
    fn fall_context(context: EntityFallOnContext<'_>) -> EntityFallOnContext<'_> {
        context.with_fall_distance(context.fall_distance * 0.5)
    }

    #[must_use]
    fn velocity_after_fall(context: EntityLandingContext) -> DVec3 {
        if context.velocity.y >= 0.0 {
            return context.velocity;
        }

        let entity_factor = if context.is_living_entity { 1.0 } else { 0.8 };
        DVec3::new(
            context.velocity.x,
            -context.velocity.y * BED_BOUNCE_SCALE * entity_factor,
            context.velocity.z,
        )
    }

    fn head_state_and_pos(
        &self,
        world: &Arc<World>,
        state: BlockStateId,
        pos: BlockPos,
    ) -> Option<(BlockStateId, BlockPos)> {
        if state.get_value(BED_PART) == BedPart::Head {
            return Some((state, pos));
        }

        let head_pos = state.get_value(FACING).relative(pos);
        let head_state = world.get_block_state(head_pos);
        (head_state.get_block() == self.block).then_some((head_state, head_pos))
    }

    const fn neighbor_direction(part: &BedPart, facing: Direction) -> Direction {
        match part {
            BedPart::Foot => facing,
            BedPart::Head => facing.opposite(),
        }
    }

    pub(crate) fn find_standup_position(
        world: &Arc<World>,
        entity: &dyn Entity,
        forward_dir: Direction,
        block_pos: BlockPos,
    ) -> Option<DVec3> {
        Self::find_standup_position_with_yaw(
            world,
            entity,
            forward_dir,
            block_pos,
            entity.rotation().0,
        )
    }

    pub(crate) fn find_standup_position_with_yaw(
        world: &Arc<World>,
        entity: &dyn Entity,
        forward_dir: Direction,
        block_pos: BlockPos,
        yaw: f32,
    ) -> Option<DVec3> {
        let right = forward_dir.rotate_y_clockwise();
        let side = if right.is_facing_yaw(yaw) {
            right.opposite()
        } else {
            right
        };

        if world.get_block_state(block_pos.below()).is_bed() {
            Self::find_bunk_bed_standup_position(world, entity, forward_dir, side, block_pos)
        } else {
            let offsets = Self::bed_standup_offsets(forward_dir, side);

            if let Some(safe_pos) =
                Self::find_standup_position_at_offset(world, entity, block_pos, &offsets, true)
            {
                return Some(safe_pos);
            }

            Self::find_standup_position_at_offset(world, entity, block_pos, &offsets, false)
        }
    }

    fn find_bunk_bed_standup_position(
        world: &Arc<World>,
        entity: &dyn Entity,
        forward_dir: Direction,
        side_dir: Direction,
        block_pos: BlockPos,
    ) -> Option<DVec3> {
        let offsets = Self::bed_surround_standup_offsets(forward_dir, side_dir);
        let below = block_pos.below();
        let above_offsets = Self::bed_above_standup_offsets(forward_dir);

        for check_dangerous in [true, false] {
            for (pos, offsets) in [
                (block_pos, offsets.as_slice()),
                (below, offsets.as_slice()),
                (block_pos, above_offsets.as_slice()),
            ] {
                if let Some(pos) = Self::find_standup_position_at_offset(
                    world,
                    entity,
                    pos,
                    offsets,
                    check_dangerous,
                ) {
                    return Some(pos);
                }
            }
        }

        None
    }

    fn find_standup_position_at_offset(
        world: &Arc<World>,
        entity: &dyn Entity,
        pos: BlockPos,
        offsets: &[(i32, i32)],
        check_dangerous: bool,
    ) -> Option<DVec3> {
        for &(off_x, off_z) in offsets {
            let offset_pos = BlockPos::new(pos.x() + off_x, pos.y(), pos.z() + off_z);
            if let Some(position) = dismount_helper::find_safe_dismount_location(
                world,
                entity,
                offset_pos,
                check_dangerous,
            ) {
                return Some(position);
            }
        }

        None
    }

    #[must_use]
    const fn bed_standup_offsets(forward: Direction, side: Direction) -> [(i32, i32); 12] {
        let surround = Self::bed_surround_standup_offsets(forward, side);
        let above = Self::bed_above_standup_offsets(forward);

        [
            surround[0],
            surround[1],
            surround[2],
            surround[3],
            surround[4],
            surround[5],
            surround[6],
            surround[7],
            surround[8],
            surround[9],
            above[0],
            above[1],
        ]
    }

    #[must_use]
    const fn bed_surround_standup_offsets(forward: Direction, side: Direction) -> [(i32, i32); 10] {
        let (fx, fz) = forward.offset_xz();
        let (sx, sz) = side.offset_xz();

        [
            (sx, sz),
            (sx - fx, sz - fz),
            (sx - fx * 2, sz - fz * 2),
            (-fx * 2, -fz * 2),
            (-sx - fx * 2, -sz - fz * 2),
            (-sx - fx, -sz - fz),
            (-sx, -sz),
            (-sx + fx, -sz + fz),
            (fx, fz),
            (sx + fx, sz + fz),
        ]
    }

    #[must_use]
    const fn bed_above_standup_offsets(forward: Direction) -> [(i32, i32); 2] {
        let (fx, fz) = forward.offset_xz();
        [(0, 0), (-fx, -fz)]
    }
}

impl BlockBehavior for BedBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let facing = context.horizontal_direction();
        let head_pos = facing.relative(context.place_pos());
        let head_state = context.world.get_block_state(head_pos);
        if !head_state.can_be_replaced(context)
            || !context.world.is_block_within_world_border(head_pos)
        {
            return None;
        }

        Some(self.block.default_state().set_value(FACING, facing))
    }

    fn fall_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        context: EntityFallOnContext<'_>,
    ) -> Option<EntityFallDamage> {
        self.default_fall_on(state, world, pos, Self::fall_context(context))
    }

    fn update_entity_movement_after_fall_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        context: EntityLandingContext,
    ) -> DVec3 {
        if context.suppresses_bounce {
            return self.default_update_entity_movement_after_fall_on(state, world, pos, context);
        }

        Self::velocity_after_fall(context)
    }

    fn is_pathfindable(
        &self,
        _state: BlockStateId,
        _computation_type: PathComputationType,
    ) -> bool {
        false
    }

    fn is_bed(&self) -> bool {
        true
    }

    fn player_will_destroy(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
    ) -> BlockStateId {
        if !player.has_infinite_materials() || state.get_value(BED_PART) != BedPart::Foot {
            return state;
        }

        let facing = state.get_value(FACING);
        let head_pos = Self::neighbor_direction(&BedPart::Foot, facing).relative(pos);
        let head_state = world.get_block_state(head_pos);
        if head_state.get_block() != self.block || head_state.get_value(BED_PART) != BedPart::Head {
            return state;
        }

        world.set_block(
            head_pos,
            vanilla_blocks::AIR.default_state(),
            UpdateFlags::UPDATE_ALL | UpdateFlags::UPDATE_SUPPRESS_DROPS,
        );
        world.destroy_block_effect(head_pos, u32::from(head_state.0), Some(player.id()));
        state
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
        let part = state.get_value(BED_PART);
        let facing = state.get_value(FACING);
        if direction != Self::neighbor_direction(&part, facing) {
            return state;
        }

        if neighbor_state.get_block() == self.block && neighbor_state.get_value(BED_PART) != part {
            return state.set_value(OCCUPIED, neighbor_state.get_value(OCCUPIED));
        }

        vanilla_blocks::AIR.default_state()
    }

    fn set_placed_by(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source: &PlacementSource<'_>,
    ) {
        let facing = state.get_value(FACING);
        let head_pos = facing.relative(pos);
        let head_state = state.set_value(BED_PART, BedPart::Head);

        world.set_block(head_pos, head_state, UpdateFlags::UPDATE_ALL);
        world.update_neighbors_at(pos, &vanilla_blocks::AIR);
        world.update_neighbor_shapes_at(state, pos, UpdateFlags::UPDATE_ALL, World::UPDATE_LIMIT);
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
        let Some((head_state, head_pos)) = self.head_state_and_pos(world, state, pos) else {
            return InteractionResult::Consume;
        };

        if world.dimension_type.bed_rule.explodes {
            let error_key = world
                .dimension_type
                .bed_rule
                .error_message_key
                .unwrap_or("block.minecraft.bed.no_sleep");
            player.send_overlay_message(&TextComponent::translated(TranslatedMessage {
                key: error_key.into(),
                fallback: None,
                args: None,
            }));

            let facing = head_state.get_value(FACING);
            let foot_pos = facing.opposite().relative(head_pos);

            world.set_block(
                head_pos,
                vanilla_blocks::AIR.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
            let foot_state = world.get_block_state(foot_pos);
            if foot_state.get_block() == self.block {
                world.set_block(
                    foot_pos,
                    vanilla_blocks::AIR.default_state(),
                    UpdateFlags::UPDATE_ALL,
                );
            }

            let center = head_pos.0.as_dvec3() + DVec3::splat(0.5);
            let damage_source = DamageSource::environment(&vanilla_damage_types::BAD_RESPAWN_POINT)
                .with_source_position(center);

            world.explode(
                None,
                Some(damage_source),
                center,
                5.0,
                true,
                ExplosionBlockInteraction::Destroy,
            );

            return InteractionResult::SuccessServer;
        }

        if head_state.get_value(OCCUPIED) {
            // TODO: Mirror vanilla `kickVillagerOutOfBed`: find a sleeping
            // villager in this bed AABB and call `stopSleeping` once villager
            // sleeping exists.
            player.send_overlay_message(&TextComponent::translated(TranslatedMessage {
                key: "block.minecraft.bed.occupied".into(),
                fallback: None,
                args: None,
            }));
            return InteractionResult::SuccessServer;
        }

        if let Err(problem) = player.start_sleep_in_bed(head_pos)
            && let Some(message) = problem.message()
        {
            player.send_overlay_message(message);
        }

        InteractionResult::SuccessServer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use steel_registry::{sound_events, vanilla_entities};

    use crate::behavior::EntityFallOnFacts;

    fn landing(
        velocity: DVec3,
        is_living_entity: bool,
        suppresses_bounce: bool,
    ) -> EntityLandingContext {
        EntityLandingContext::new(velocity, is_living_entity, suppresses_bounce)
    }

    #[test]
    fn bed_halves_fall_distance_before_default_damage() {
        let context = BedBlock::fall_context(EntityFallOnContext::new(
            12.0,
            false,
            EntityFallOnFacts::new(
                &vanilla_entities::PLAYER,
                true,
                0.6,
                1.8,
                (
                    &sound_events::ENTITY_PLAYER_SMALL_FALL,
                    &sound_events::ENTITY_PLAYER_BIG_FALL,
                ),
            ),
            None,
        ));

        assert!((context.fall_distance - 6.0).abs() < f64::EPSILON);
        assert!(!context.suppresses_bounce);
        assert!(context.entity.is_player());
    }

    #[test]
    fn living_entities_bounce_with_bed_factor() {
        let velocity =
            BedBlock::velocity_after_fall(landing(DVec3::new(1.0, -3.0, -2.0), true, false));

        assert!((velocity.y - 1.980_000_078_678_131).abs() < f64::EPSILON);
        assert!((velocity.x - 1.0).abs() < f64::EPSILON);
        assert!((velocity.z + 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn non_living_entities_bounce_with_vanilla_reduction() {
        let velocity =
            BedBlock::velocity_after_fall(landing(DVec3::new(1.0, -3.0, -2.0), false, false));

        assert!((velocity.y - 1.584_000_062_942_505).abs() < f64::EPSILON);
    }

    #[test]
    fn upward_velocity_is_not_changed_by_bounce_logic() {
        let velocity =
            BedBlock::velocity_after_fall(landing(DVec3::new(1.0, 0.5, -2.0), true, false));

        assert_eq!(velocity, DVec3::new(1.0, 0.5, -2.0));
    }

    #[test]
    fn bed_explodes_in_nether_dimension() {
        use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_dimension_types};
        use steel_utils::ChunkPos;
        use crate::behavior::BLOCK_BEHAVIORS;
        use crate::test_support::{
            TestPlayerBuilder, fresh_test_world_in_dimension, insert_ready_full_chunk,
        };

        init_vanilla_registry();
        crate::behavior::init_behaviors();
        let world = fresh_test_world_in_dimension(
            "bed_explosion_test",
            &vanilla_dimension_types::THE_NETHER,
        );
        insert_ready_full_chunk(&world, ChunkPos::new(0, 0));

        let foot_pos = BlockPos::new(0, 64, 0);
        let head_pos = BlockPos::new(0, 64, 1);

        let foot_state = vanilla_blocks::RED_BED
            .default_state()
            .set_value(BED_PART, BedPart::Foot)
            .set_value(FACING, Direction::South);
        let head_state = vanilla_blocks::RED_BED
            .default_state()
            .set_value(BED_PART, BedPart::Head)
            .set_value(FACING, Direction::South);

        world.set_block(foot_pos, foot_state, UpdateFlags::UPDATE_ALL);
        world.set_block(head_pos, head_state, UpdateFlags::UPDATE_ALL);

        let shared_player = TestPlayerBuilder::new(Arc::clone(&world), "exploder", 1).build();
        let player = shared_player.as_player().unwrap();

        let hit_result = BlockHitResult {
            location: DVec3::ZERO,
            direction: Direction::Up,
            block_pos: foot_pos,
            inside: false,
            miss: false,
            world_border_hit: false,
        };
        let mut inv_access =
            InventoryAccess::new(Arc::clone(&player.inventory), steel_utils::types::InteractionHand::MainHand);

        let bed_behavior = BLOCK_BEHAVIORS.get_behavior(&vanilla_blocks::RED_BED);
        let res = bed_behavior.use_without_item(
            foot_state,
            &world,
            foot_pos,
            player,
            &hit_result,
            &mut inv_access,
        );

        assert_eq!(res, InteractionResult::SuccessServer);
        assert!(world.get_block_state(foot_pos).is_air());
        assert!(world.get_block_state(head_pos).is_air());
    }
}
