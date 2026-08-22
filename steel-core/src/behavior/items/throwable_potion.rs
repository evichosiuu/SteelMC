use std::borrow::Cow;
use std::sync::Arc;

use glam::DVec3;
use steel_macros::item_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::item_stack::ItemStack;
use steel_registry::{sound_events, vanilla_entities};
use text_components::TextComponent;

use crate::behavior::context::{InteractionResult, UseItemContext};
use crate::behavior::ItemBehavior;
use crate::entity::entities::{ThrownLingeringPotionEntity, ThrownSplashPotionEntity};
use crate::entity::{
    Entity, Projectile, SharedEntity, ThrowableItemProjectile, next_entity_id,
};

use super::dynamic_name::potion_name;

/// Vanilla throwable potion shoot power.
const SHOOT_POWER: f32 = 0.5;

/// Splash-potion behavior providing Vanilla's potion-content-dependent name and throw use.
#[item_behavior]
pub struct SplashPotionItem;

impl ItemBehavior for SplashPotionItem {
    fn get_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        potion_name(stack)
    }

    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let player = context.player;
        let world = context.world;

        let pitch = 0.4 / (rand::random::<f32>() * 0.4 + 0.8);
        world.play_sound_at(
            &sound_events::ENTITY_SPLASH_POTION_THROW,
            SoundSource::Neutral,
            player.position(),
            0.5,
            pitch,
            None,
        );

        let thrown_item = context.inv.with_item(|item| item.clone());

        let player_pos = player.position();
        let spawn_pos = DVec3::new(player_pos.x, player.get_eye_y() - 0.1, player_pos.z);

        let potion = Arc::new(ThrownSplashPotionEntity::new(
            &vanilla_entities::SPLASH_POTION,
            next_entity_id(),
            spawn_pos,
            Arc::downgrade(world),
        ));
        if let Some(owner) = world.players.get_by_uuid(&player.gameprofile.id) {
            let owner: SharedEntity = owner;
            potion.set_owner_entity(Some(&owner));
        } else {
            potion.set_owner_uuid(Some(player.gameprofile.id));
        }
        potion.set_item_clamped(thrown_item);

        let (yaw, player_pitch) = player.rotation();
        potion.shoot_from_rotation(player, player_pitch, yaw, -20.0, SHOOT_POWER, 1.0);

        let entity: SharedEntity = potion;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("failed to spawn splash potion: {error}");
            return InteractionResult::Fail;
        }

        context.inv.with_item(|item| item.shrink(1));

        InteractionResult::Success
    }
}

/// Lingering-potion behavior providing Vanilla's potion-content-dependent name and throw use.
#[item_behavior]
pub struct LingeringPotionItem;

impl ItemBehavior for LingeringPotionItem {
    fn get_name<'a>(&self, stack: &'a ItemStack) -> Cow<'a, TextComponent> {
        potion_name(stack)
    }

    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let player = context.player;
        let world = context.world;

        let pitch = 0.4 / (rand::random::<f32>() * 0.4 + 0.8);
        world.play_sound_at(
            &sound_events::ENTITY_LINGERING_POTION_THROW,
            SoundSource::Neutral,
            player.position(),
            0.5,
            pitch,
            None,
        );

        let thrown_item = context.inv.with_item(|item| item.clone());

        let player_pos = player.position();
        let spawn_pos = DVec3::new(player_pos.x, player.get_eye_y() - 0.1, player_pos.z);

        let potion = Arc::new(ThrownLingeringPotionEntity::new(
            &vanilla_entities::LINGERING_POTION,
            next_entity_id(),
            spawn_pos,
            Arc::downgrade(world),
        ));
        if let Some(owner) = world.players.get_by_uuid(&player.gameprofile.id) {
            let owner: SharedEntity = owner;
            potion.set_owner_entity(Some(&owner));
        } else {
            potion.set_owner_uuid(Some(player.gameprofile.id));
        }
        potion.set_item_clamped(thrown_item);

        let (yaw, player_pitch) = player.rotation();
        potion.shoot_from_rotation(player, player_pitch, yaw, -20.0, SHOOT_POWER, 1.0);

        let entity: SharedEntity = potion;
        if let Err(error) = world.try_add_entity(entity) {
            log::debug!("failed to spawn lingering potion: {error}");
            return InteractionResult::Fail;
        }

        context.inv.with_item(|item| item.shrink(1));

        InteractionResult::Success
    }
}
