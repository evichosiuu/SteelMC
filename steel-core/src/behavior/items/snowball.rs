//! Snowball item behavior (`SnowballItem`).
//!
//! Throwing a snowball spawns a [`SnowballEntity`] from the player's eye,
//! shot along their look direction, and consumes one snowball.
//! Mirrors vanilla `SnowballItem.use`.

use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::{sound_events, vanilla_entities};

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::item::ItemBehavior;
use crate::entity::entities::SnowballEntity;
use crate::entity::{Entity, Projectile, SharedEntity, ThrowableItemProjectile, next_entity_id};

/// Vanilla `SnowballItem.PROJECTILE_SHOOT_POWER`.
const SHOOT_POWER: f32 = 1.5;

/// Behavior for the snowball item.
#[item_behavior(class = "SnowballItem")]
pub struct SnowballItem;

impl ItemBehavior for SnowballItem {
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let player = context.player;
        let world = context.world;

        let pitch = 0.4 / (rand::random::<f32>() * 0.4 + 0.8);
        world.play_sound_at(
            &sound_events::ENTITY_SNOWBALL_THROW,
            SoundSource::Neutral,
            player.position(),
            0.5,
            pitch,
            None,
        );

        let thrown_item = context.inv.with_item(|item| item.clone());

        // Vanilla `ThrowableItemProjectile` spawns at the shooter's eye minus 0.1.
        let player_pos = player.position();
        let spawn_pos = DVec3::new(player_pos.x, player.get_eye_y() - 0.1, player_pos.z);

        let snowball = Arc::new(SnowballEntity::new(
            &vanilla_entities::SNOWBALL,
            next_entity_id(),
            spawn_pos,
            Arc::downgrade(world),
        ));
        if let Some(owner) = world.players.get_by_uuid(&player.gameprofile.id) {
            let owner: SharedEntity = owner;
            snowball.set_owner_entity(Some(&owner));
        } else {
            snowball.set_owner_uuid(Some(player.gameprofile.id));
        }
        snowball.set_item_clamped(thrown_item);

        let (yaw, player_pitch) = player.rotation();
        snowball.shoot_from_rotation(player, player_pitch, yaw, 0.0, SHOOT_POWER, 1.0);

        let entity: SharedEntity = snowball;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("failed to spawn snowball: {error}");
            return InteractionResult::Fail;
        }

        context.inv.with_item(|item| item.shrink(1));

        InteractionResult::Success
    }
}
