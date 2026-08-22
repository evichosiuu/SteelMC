//! Ender Dragon Fight manager.

use std::sync::{Arc, Weak};

use glam::DVec3;
use steel_protocol::packets::game::{BossBarColor, BossBarOverlay, CBossEvent};
use steel_registry::{vanilla_blocks, vanilla_entities};
use steel_utils::{BlockPos, WorldAabb, locks::SyncMutex, types::UpdateFlags};
use text_components::TextComponent;
use uuid::Uuid;

use crate::entity::damage::DamageSource;
use crate::entity::entities::{EndCrystalEntity, EnderDragonEntity, ExperienceOrbEntity};
use crate::entity::Entity;
use crate::player::Player;
use crate::world::World;

/// State of the Ender Dragon Fight in the End dimension.
#[derive(Debug)]
pub struct EnderDragonFight {
    world: Weak<World>,
    boss_bar_uuid: Uuid,
    state: SyncMutex<DragonFightState>,
}

#[derive(Debug)]
struct DragonFightState {
    dragon_killed: bool,
    previously_killed: bool,
    is_respawning: bool,
    gateway_count: usize,
    exit_portal_location: BlockPos,
    _dragon_uuid: Option<Uuid>,
}

impl EnderDragonFight {
    /// Creates a new dragon fight manager for an End world.
    #[must_use]
    pub fn new(world: Weak<World>) -> Self {
        Self {
            world,
            boss_bar_uuid: Uuid::new_v4(),
            state: SyncMutex::new(DragonFightState {
                dragon_killed: false,
                previously_killed: false,
                is_respawning: false,
                gateway_count: 0,
                exit_portal_location: BlockPos::new(0, 45, 0),
                _dragon_uuid: None,
            }),
        }
    }

    /// Adds a player to the dragon fight boss bar.
    pub fn add_player(&self, player: &Player) {
        let state = self.state.lock();
        if !state.dragon_killed {
            let packet = CBossEvent::add(
                self.boss_bar_uuid,
                TextComponent::from("Ender Dragon"),
                1.0,
                BossBarColor::Pink,
                BossBarOverlay::Progress,
                0,
            );
            player.send_packet(packet);
        }
    }

    /// Removes a player from the dragon fight boss bar.
    pub fn remove_player(&self, player: &Player) {
        let packet = CBossEvent::remove(self.boss_bar_uuid);
        player.send_packet(packet);
    }

    /// Updates the dragon's health on the boss bar across all End players.
    pub fn update_dragon_health(&self, health: f32) {
        let health_ratio = (health / EnderDragonEntity::MAX_HEALTH).clamp(0.0, 1.0);
        let packet = CBossEvent::update_health(self.boss_bar_uuid, health_ratio);
        if let Some(world) = self.world.upgrade() {
            world.players.iter_players(|_uuid, player| {
                player.send_packet(packet.clone());
                true
            });
        }
    }

    /// Called when an End Crystal is destroyed.
    pub fn on_crystal_destroyed(&self, crystal: &EndCrystalEntity, source: &DamageSource) {
        if let Some(world) = self.world.upgrade() {
            let search_box = crystal.bounding_box().inflate(32.0);
            for entity in world.get_entities_in_aabb_matching(&search_box, |e| {
                e.entity_type() == &vanilla_entities::ENDER_DRAGON
            }) {
                if let Some(dragon) = entity.as_living_entity() {
                    dragon.hurt(&world, source, 10.0);
                }
            }
        }
    }

    /// Called when the Ender Dragon is killed.
    pub fn on_dragon_killed(&self, dragon: &EnderDragonEntity) {
        let mut state = self.state.lock();
        state.dragon_killed = true;

        if let Some(world) = self.world.upgrade() {
            let remove_packet = CBossEvent::remove(self.boss_bar_uuid);
            world.players.iter_players(|_uuid, player| {
                player.send_packet(remove_packet.clone());
                true
            });

            let xp_amount = if !state.previously_killed { 12000 } else { 500 };
            let orb_count = 10;
            let xp_per_orb = xp_amount / orb_count;
            for _ in 0..orb_count {
                let orb = Arc::new(ExperienceOrbEntity::new(
                    &vanilla_entities::EXPERIENCE_ORB,
                    crate::entity::next_entity_id(),
                    dragon.position(),
                    Arc::downgrade(&world),
                ));
                orb.set_value(xp_per_orb);
                let _ = world.try_add_entity(orb);
            }

            self.generate_exit_portal_blocks(&world, &state, true);
            self.spawn_next_gateway_internal(&world, &mut state);
            state.previously_killed = true;
        }
    }

    /// Checks if 4 End Crystals are placed on the exit portal edges to trigger a dragon respawn.
    pub fn try_respawn_dragon(&self, world: &Arc<World>) -> bool {
        let mut state = self.state.lock();
        if !state.dragon_killed || state.is_respawning {
            return false;
        }

        let center = state.exit_portal_location;
        let portal_crystal_offsets = [
            BlockPos::new(center.x(), center.y(), center.z() + 2),
            BlockPos::new(center.x(), center.y(), center.z() - 2),
            BlockPos::new(center.x() + 2, center.y(), center.z()),
            BlockPos::new(center.x() - 2, center.y(), center.z()),
        ];

        let mut crystals_count = 0;
        for pos in portal_crystal_offsets {
            let search_box = WorldAabb::new(
                f64::from(pos.x()),
                f64::from(pos.y()),
                f64::from(pos.z()),
                f64::from(pos.x() + 1),
                f64::from(pos.y() + 2),
                f64::from(pos.z() + 1),
            );
            let has_crystal = !world
                .get_entities_in_aabb_matching(&search_box, |e| {
                    e.entity_type() == &vanilla_entities::END_CRYSTAL
                })
                .is_empty();
            if has_crystal {
                crystals_count += 1;
            }
        }

        if crystals_count == 4 {
            state.dragon_killed = false;
            state.is_respawning = false;

            let dragon = Arc::new(EnderDragonEntity::new(
                &vanilla_entities::ENDER_DRAGON,
                crate::entity::next_entity_id(),
                DVec3::new(0.0, 128.0, 0.0),
                Arc::downgrade(world),
            ));
            let _ = world.try_add_entity(dragon);
            return true;
        }

        false
    }

    /// Generates exit portal blocks at the center.
    fn generate_exit_portal_blocks(&self, world: &Arc<World>, state: &DragonFightState, active: bool) {
        let center = state.exit_portal_location;

        for dx in -1..=1 {
            for dz in -1..=1 {
                let portal_pos = BlockPos::new(center.x() + dx, center.y(), center.z() + dz);
                let block = if active {
                    vanilla_blocks::END_PORTAL.default_state()
                } else {
                    vanilla_blocks::AIR.default_state()
                };
                let _ = world.set_block(portal_pos, block, UpdateFlags::UPDATE_ALL);
            }
        }

        if active && !state.previously_killed {
            let egg_pos = BlockPos::new(center.x(), center.y() + 1, center.z());
            let _ = world.set_block(
                egg_pos,
                vanilla_blocks::DRAGON_EGG.default_state(),
                UpdateFlags::UPDATE_ALL,
            );
        }
    }

    /// Spawns the next End Gateway in the ring.
    fn spawn_next_gateway_internal(&self, world: &Arc<World>, state: &mut DragonFightState) {
        if state.gateway_count >= 20 {
            return;
        }

        let angle = state.gateway_count as f64 * (std::f64::consts::TAU / 20.0);
        let x = (angle.cos() * 96.0).round() as i32;
        let z = (angle.sin() * 96.0).round() as i32;
        let gateway_pos = BlockPos::new(x, 75, z);

        if world.create_end_gateway_portal(gateway_pos, BlockPos::ZERO, true) {
            state.gateway_count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::fresh_test_world;

    #[test]
    fn dragon_fight_creation() {
        let fight = EnderDragonFight::new(Weak::new());
        assert!(!fight.state.lock().dragon_killed);
    }

    #[test]
    fn try_respawn_dragon_returns_false_when_crystals_missing() {
        let world = fresh_test_world("dragon_fight_respawn");
        let fight = EnderDragonFight::new(Arc::downgrade(&world));
        fight.state.lock().dragon_killed = true;

        assert!(!fight.try_respawn_dragon(&world));
    }
}
