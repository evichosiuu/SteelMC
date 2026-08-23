//! Ender eye item behavior implementation.

use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::REGISTRY;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{BlockStateProperties, Direction};
use steel_registry::{level_events, sound_events, vanilla_blocks, vanilla_entities};
use steel_utils::{BlockPos, Identifier, types::UpdateFlags};

use crate::behavior::ItemBehavior;
use crate::behavior::block::push_entities_up;
use crate::behavior::context::{InteractionResult, UseItemContext, UseOnContext};
use crate::entity::entities::EyeOfEnderEntity;
use crate::entity::{Entity, SharedEntity, next_entity_id};
use crate::world::{LevelReader, World};
use crate::worldgen::generator::ChunkGenerator;

const END_PORTAL_PATTERN_DISTANCE: i32 = 5;
const END_PORTAL_PATTERN: [[char; 5]; 5] = [
    ['?', 'v', 'v', 'v', '?'],
    ['>', '?', '?', '?', '<'],
    ['>', '?', '?', '?', '<'],
    ['>', '?', '?', '?', '<'],
    ['?', '^', '^', '^', '?'],
];
const PATTERN_DIRECTIONS: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

/// Behavior for the ender eye item.
///
/// When used on an end portal frame without an eye, places the eye
/// and checks for portal completion. When used in air/general use, throws an
/// Eye of Ender that flies toward the nearest stronghold.
#[item_behavior(class = "EnderEyeItem")]
pub struct EnderEyeItem;

impl ItemBehavior for EnderEyeItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let clicked_pos = context.hit_result.block_pos;
        let clicked_state = context.world.get_block_state(clicked_pos);

        let Some(clicked_block) = REGISTRY.blocks.by_state_id(clicked_state) else {
            return InteractionResult::Pass;
        };

        if clicked_block.key != vanilla_blocks::END_PORTAL_FRAME.key {
            return InteractionResult::Pass;
        }

        let has_eye: bool = clicked_state.get_value(&BlockStateProperties::EYE);
        if has_eye {
            return InteractionResult::Pass;
        }

        let new_state = clicked_state.set_value(&BlockStateProperties::EYE, true);
        let new_state = push_entities_up(clicked_state, new_state, context.world, clicked_pos);

        if !context
            .world
            .set_block(clicked_pos, new_state, UpdateFlags::UPDATE_CLIENTS)
        {
            return InteractionResult::Pass;
        }
        context
            .world
            .update_neighbor_for_output_signal(clicked_pos, &vanilla_blocks::END_PORTAL_FRAME);

        // Play the end portal frame fill sound effect (no exclusion, all players hear it)
        context
            .world
            .level_event(level_events::END_PORTAL_FRAME_FILL, clicked_pos, 0, None);

        context.inv.with_item(|item| item.shrink(1));

        if let Some(portal_origin) = find_completed_end_portal_origin(context.world, clicked_pos) {
            spawn_end_portal(context.world, portal_origin);
        }

        InteractionResult::Success
    }

    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let player = context.player;
        let world = context.world;

        let player_pos = player.position();
        let block_origin = BlockPos::from(player_pos);

        let target_pos = if let Some(structure_generator) = world
            .chunk_map
            .world_gen_context
            .generator
            .structure_generator()
            && let Some(plan) = structure_generator
                .locate_plan_for_structures(&[Identifier::vanilla_static("stronghold")])
            && let candidates = plan.ring_candidates(block_origin)
            && let Some(first_candidate) = candidates.first()
        {
            first_candidate.locate_pos
        } else {
            let (yaw, pitch) = player.rotation();
            let yaw_rad = (yaw + 90.0).to_radians();
            let pitch_rad = (-pitch).to_radians();
            let dx = f64::from(pitch_rad.cos() * yaw_rad.cos());
            let dz = f64::from(pitch_rad.cos() * yaw_rad.sin());
            let dy = f64::from(pitch_rad.sin());
            BlockPos::from(player_pos + DVec3::new(dx * 12.0, dy * 12.0, dz * 12.0))
        };

        let pitch = 1.0 / (rand::random::<f32>() * 0.4 + 1.2);
        world.play_sound_at(
            &sound_events::ENTITY_ENDER_EYE_LAUNCH,
            SoundSource::Neutral,
            player_pos,
            0.5,
            pitch,
            None,
        );

        let thrown_item = context.inv.with_item(|item| item.copy_with_count(1));

        let spawn_pos = DVec3::new(player_pos.x, player.get_eye_y() - 0.1, player_pos.z);
        let eye = Arc::new(EyeOfEnderEntity::new(
            &vanilla_entities::EYE_OF_ENDER,
            next_entity_id(),
            spawn_pos,
            Arc::downgrade(world),
        ));

        eye.set_item(thrown_item);
        eye.signal_to(target_pos);

        let entity: SharedEntity = eye;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("failed to spawn eye of ender: {error}");
            return InteractionResult::Fail;
        }

        if !player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}

fn find_completed_end_portal_origin(
    level: &impl LevelReader,
    clicked_pos: BlockPos,
) -> Option<BlockPos> {
    for z in clicked_pos.z()..clicked_pos.z() + END_PORTAL_PATTERN_DISTANCE {
        for y in clicked_pos.y()..clicked_pos.y() + END_PORTAL_PATTERN_DISTANCE {
            for x in clicked_pos.x()..clicked_pos.x() + END_PORTAL_PATTERN_DISTANCE {
                let front_top_left = BlockPos::new(x, y, z);
                for forwards in PATTERN_DIRECTIONS {
                    for up in PATTERN_DIRECTIONS {
                        if up == forwards || up == forwards.opposite() {
                            continue;
                        }
                        if end_portal_pattern_matches(level, front_top_left, forwards, up) {
                            return Some(front_top_left.offset(-3, 0, -3));
                        }
                    }
                }
            }
        }
    }

    None
}

fn end_portal_pattern_matches(
    level: &impl LevelReader,
    front_top_left: BlockPos,
    forwards: Direction,
    up: Direction,
) -> bool {
    let forwards_vector = forwards.offset_vec();
    let up_vector = up.offset_vec();
    let right_vector = forwards_vector.cross(up_vector);

    for right in 0..5 {
        for down in 0..5 {
            let pattern_pos = BlockPos(front_top_left.0 + up_vector * -down + right_vector * right);
            if !end_portal_pattern_entry_matches(
                level,
                pattern_pos,
                END_PORTAL_PATTERN[down as usize][right as usize],
            ) {
                return false;
            }
        }
    }

    true
}

fn end_portal_pattern_entry_matches(
    level: &impl LevelReader,
    pos: BlockPos,
    pattern_entry: char,
) -> bool {
    match pattern_entry {
        '?' => true,
        '^' => end_portal_frame_matches(level, pos, Direction::South),
        '>' => end_portal_frame_matches(level, pos, Direction::West),
        'v' => end_portal_frame_matches(level, pos, Direction::North),
        '<' => end_portal_frame_matches(level, pos, Direction::East),
        _ => false,
    }
}

fn end_portal_frame_matches(level: &impl LevelReader, pos: BlockPos, facing: Direction) -> bool {
    let state = level.get_block_state(pos);
    state.get_block() == &vanilla_blocks::END_PORTAL_FRAME
        && state.get_value(&BlockStateProperties::EYE)
        && state.get_value(&BlockStateProperties::HORIZONTAL_FACING) == facing
}

fn spawn_end_portal(world: &Arc<World>, portal_origin: BlockPos) {
    let portal_state = vanilla_blocks::END_PORTAL.default_state();
    for x_offset in 0..3 {
        for z_offset in 0..3 {
            let portal_pos = portal_origin.offset(x_offset, 0, z_offset);
            let _ = world.destroy_block(portal_pos, true);
            let _ = world.set_block(portal_pos, portal_state, UpdateFlags::UPDATE_CLIENTS);
        }
    }

    world.global_level_event(
        level_events::SOUND_END_PORTAL_SPAWN,
        portal_origin.offset(1, 0, 1),
        0,
    );
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use steel_registry::blocks::block_state_ext::BlockStateExt;
    use steel_registry::blocks::properties::{BlockStateProperties, Direction};
    use steel_registry::item_stack::ItemStack;
    use steel_registry::{init_vanilla_registry, vanilla_blocks, vanilla_entities, vanilla_items};
    use steel_utils::types::InteractionHand;
    use steel_utils::{BlockPos, BlockStateId};

    use crate::behavior::ItemBehavior;
    use crate::behavior::context::{InteractionResult, UseItemContext};
    use crate::entity::Entity;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    use super::{EnderEyeItem, find_completed_end_portal_origin};

    fn eye_frame(facing: Direction) -> BlockStateId {
        vanilla_blocks::END_PORTAL_FRAME
            .default_state()
            .set_value(&BlockStateProperties::HORIZONTAL_FACING, facing)
            .set_value(&BlockStateProperties::EYE, true)
    }

    fn place_inward_frame_ring(level: &crate::test_support::TestLevel, origin: BlockPos) {
        for offset in 0..3 {
            level.set_test_block(origin.offset(offset, 0, -1), eye_frame(Direction::South));
            level.set_test_block(origin.offset(offset, 0, 3), eye_frame(Direction::North));
            level.set_test_block(origin.offset(-1, 0, offset), eye_frame(Direction::East));
            level.set_test_block(origin.offset(3, 0, offset), eye_frame(Direction::West));
        }
    }

    #[test]
    fn end_portal_pattern_matches_player_built_inward_layout() {
        init_vanilla_registry();

        let level = crate::test_support::TestLevel::default();
        let origin = BlockPos::new(4, 64, 9);
        place_inward_frame_ring(&level, origin);

        assert_eq!(
            find_completed_end_portal_origin(&level, origin.offset(1, 0, -1)),
            Some(origin)
        );
        assert_eq!(
            find_completed_end_portal_origin(&level, origin.offset(-1, 0, 2)),
            Some(origin)
        );
        assert_eq!(
            find_completed_end_portal_origin(&level, origin.offset(2, 0, 3)),
            Some(origin)
        );
        assert_eq!(
            find_completed_end_portal_origin(&level, origin.offset(3, 0, 0)),
            Some(origin)
        );
    }

    #[test]
    fn end_portal_pattern_rejects_wrong_side_facing() {
        init_vanilla_registry();

        let level = crate::test_support::TestLevel::default();
        let origin = BlockPos::new(4, 64, 9);
        place_inward_frame_ring(&level, origin);
        level.set_test_block(origin.offset(-1, 0, 1), eye_frame(Direction::West));

        assert_eq!(find_completed_end_portal_origin(&level, origin), None);
    }

    #[test]
    fn end_portal_pattern_uses_vanilla_front_top_left_offset() {
        init_vanilla_registry();

        let level = crate::test_support::TestLevel::default();
        let origin = BlockPos::new(4, 64, 9);
        place_inward_frame_ring(&level, origin);
        for offset in 0..3 {
            level.set_test_block(origin.offset(offset, 0, -1), eye_frame(Direction::North));
            level.set_test_block(origin.offset(offset, 0, 3), eye_frame(Direction::South));
        }

        assert_eq!(
            find_completed_end_portal_origin(&level, origin.offset(1, 0, -1)),
            Some(origin.offset(0, 0, -4))
        );
    }

    #[test]
    fn use_item_throws_ender_eye_and_spawns_entity() {
        init_vanilla_registry();

        let world = fresh_test_world("use_item_throws_ender_eye");
        insert_ready_full_chunk(&world, steel_utils::ChunkPos::new(0, 0));

        let player = TestPlayerBuilder::new(Arc::clone(&world), "Thrower", 1).build();
        player
            .inventory
            .lock()
            .set_selected_item(ItemStack::with_count(&vanilla_items::ENDER_EYE, 16));

        let mut context = UseItemContext::new(&player, InteractionHand::MainHand, &world, player.inventory.clone());

        let behavior = EnderEyeItem;
        let result = behavior.use_item(&mut context);

        assert_eq!(result, InteractionResult::Success);

        let remaining = player.inventory.lock().get_item_in_hand(InteractionHand::MainHand).count();
        assert_eq!(remaining, 15);

        let eye_entity = world
            .get_entities_in_aabb(&player.bounding_box().inflate(10.0))
            .into_iter()
            .find(|e| e.entity_type() == &vanilla_entities::EYE_OF_ENDER);

        assert!(eye_entity.is_some(), "Eye of Ender entity should be spawned in the world");
    }
}
