//! Boat item behavior (`BoatItem`).
//!
//! Spawns boat entities when used on water or ground.
//! Mirrors vanilla `BoatItem`.

use std::ops::{Add, Mul};
use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::items::item::BlockHitResult;

use crate::behavior::context::{InteractionResult, UseItemContext, UseOnContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::{ENTITIES, Entity, next_entity_id};
use crate::player::Player;
use crate::world::{ClipBlockShape, ClipFluid, World};

/// Behavior for boat items.
#[item_behavior(class = "BoatItem")]
pub struct BoatItem {
    #[json_arg(vanilla_entities, json = "type")]
    entity_type: EntityTypeRef,
}

impl BoatItem {
    /// Creates a boat item behavior for the given entity type.
    #[must_use]
    pub const fn new(entity_type: EntityTypeRef) -> Self {
        Self { entity_type }
    }
}

impl ItemBehavior for BoatItem {
    fn use_on(&self, _context: &mut UseOnContext) -> InteractionResult {
        InteractionResult::Pass
    }

    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let hit_result =
            get_player_pov_hit_result(context.world, context.player, ClipFluid::Any);

        if hit_result.miss {
            return InteractionResult::Pass;
        }

        let spawn_pos = hit_result.location;

        let Some(entity) = ENTITIES.create(
            self.entity_type,
            next_entity_id(),
            spawn_pos,
            Arc::downgrade(context.world),
        ) else {
            return InteractionResult::Pass;
        };

        let player_yaw = context.player.rotation().0;
        entity.set_rotation((player_yaw, 0.0));
        entity.base().set_old_rotation_to_current();

        if let Some(custom_name) = context
            .inv
            .with_item(|item| item.custom_name().map(std::borrow::Cow::into_owned))
        {
            entity.set_custom_name(Some(custom_name));
        }

        if let Err(error) = context.world.try_add_entity(entity) {
            log::debug!("failed to spawn boat entity from item: {error}");
            return InteractionResult::Fail;
        }

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}

fn get_player_pov_hit_result(
    world: &Arc<World>,
    player: &Player,
    fluid: ClipFluid,
) -> BlockHitResult {
    let from = player.position().with_y(player.get_eye_y());
    let to = from.add(
        player
            .calculate_view_vector(player.rotation().1, player.rotation().0)
            .mul(player.block_interaction_range()),
    );
    let c_r = world.clip(from, to, ClipBlockShape::Outline, fluid);
    BlockHitResult {
        location: c_r.location,
        direction: c_r.direction,
        block_pos: c_r.block_pos,
        miss: c_r.miss,
        inside: c_r.inside,
        world_border_hit: c_r.world_border_hit,
    }
}
