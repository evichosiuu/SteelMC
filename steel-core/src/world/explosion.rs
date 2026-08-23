//! Vanilla explosion system.

use std::sync::Arc;

use glam::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};
use steel_protocol::packets::game::{CExplode, CSound, SoundSource};
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::{
    particle_type::ParticleData, sound_event::SoundEventRef, sound_events, vanilla_blocks,
    vanilla_damage_types, vanilla_particle_types,
};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, ChunkPos, Downcast, WorldAabb};

use crate::behavior::FLUID_BEHAVIORS;
use crate::entity::damage::DamageSource;
use crate::entity::entities::PrimedTntEntity;
use crate::entity::{Entity, LivingEntity, SharedEntity};
use crate::world::raycast::{ClipBlockShape, ClipFluid};
use crate::world::World;

/// Controls block destruction behavior for explosions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplosionBlockInteraction {
    /// Keep blocks intact (no block destruction).
    Keep,
    /// Destroy blocks with 100% item drop chance.
    Destroy,
    /// Destroy blocks with decay drop probability (`1.0 / radius`).
    DestroyWithDecay,
    /// Trigger block interactions / redstone on destruction.
    TriggerBlocks,
}

/// Computes the vanilla explosion exposure fraction for an entity (0.0 to 1.0).
#[must_use]
pub fn get_exposure(center: DVec3, entity: &dyn Entity, world: &World) -> f32 {
    let bbox = entity.bounding_box();
    let min_x = bbox.min_x();
    let min_y = bbox.min_y();
    let min_z = bbox.min_z();
    let max_x = bbox.max_x();
    let max_y = bbox.max_y();
    let max_z = bbox.max_z();

    let dx = max_x - min_x;
    let dy = max_y - min_y;
    let dz = max_z - min_z;

    if dx <= 0.0 || dy <= 0.0 || dz <= 0.0 {
        return 0.0;
    }

    let mut unblocked = 0;
    let mut total = 0;

    let step_x = (1.0 / (dx * 2.0 + 1.0)).min(1.0);
    let step_y = (1.0 / (dy * 2.0 + 1.0)).min(1.0);
    let step_z = (1.0 / (dz * 2.0 + 1.0)).min(1.0);

    let x_offset = (1.0 - (dx * 2.0 + 1.0).floor() * step_x) * 0.5;
    let z_offset = (1.0 - (dz * 2.0 + 1.0).floor() * step_z) * 0.5;

    let mut x = x_offset;
    while x <= 1.0 {
        let mut y = 0.0;
        while y <= 1.0 {
            let mut z = z_offset;
            while z <= 1.0 {
                let target = DVec3::new(min_x + x * dx, min_y + y * dy, min_z + z * dz);
                if world
                    .clip(center, target, ClipBlockShape::Collider, ClipFluid::None)
                    .is_miss()
                {
                    unblocked += 1;
                }
                total += 1;

                z += step_z;
            }
            y += step_y;
        }
        x += step_x;
    }

    if total == 0 {
        0.0
    } else {
        unblocked as f32 / total as f32
    }
}

/// Context for performing an explosion in the world.
pub struct Explosion<'a> {
    /// World where the explosion takes place.
    pub world: &'a Arc<World>,
    /// Optional source entity causing the explosion.
    pub source: Option<SharedEntity>,
    /// Optional custom damage source for the explosion.
    pub damage_source: Option<DamageSource>,
    /// Center coordinates of the explosion.
    pub center: DVec3,
    /// Explosion power radius.
    pub radius: f32,
    /// Whether fire is placed in destroyed blocks.
    pub create_fire: bool,
    /// Mode of block destruction.
    pub block_interaction: ExplosionBlockInteraction,
    /// Small particle effect for the explosion.
    pub small_particle: ParticleData,
    /// Large particle effect for the explosion.
    pub large_particle: ParticleData,
    /// Sound event played for the explosion.
    pub sound: SoundEventRef,
}

impl<'a> Explosion<'a> {
    /// Creates a new explosion builder with vanilla defaults.
    #[must_use]
    pub fn new(world: &'a Arc<World>, center: DVec3, radius: f32) -> Self {
        Self {
            world,
            source: None,
            damage_source: None,
            center,
            radius,
            create_fire: false,
            block_interaction: ExplosionBlockInteraction::DestroyWithDecay,
            small_particle: ParticleData::simple(&vanilla_particle_types::EXPLOSION),
            large_particle: ParticleData::simple(&vanilla_particle_types::EXPLOSION_EMITTER),
            sound: &sound_events::ENTITY_GENERIC_EXPLODE,
        }
    }

    /// Sets the source entity responsible for the explosion.
    #[must_use]
    pub fn with_source(mut self, source: SharedEntity) -> Self {
        self.source = Some(source);
        self
    }

    /// Sets an explicit damage source.
    #[must_use]
    pub fn with_damage_source(mut self, damage_source: DamageSource) -> Self {
        self.damage_source = Some(damage_source);
        self
    }

    /// Sets whether the explosion places fire in destroyed blocks.
    #[must_use]
    pub fn with_fire(mut self, create_fire: bool) -> Self {
        self.create_fire = create_fire;
        self
    }

    /// Sets the block destruction mode.
    #[must_use]
    pub fn with_block_interaction(mut self, interaction: ExplosionBlockInteraction) -> Self {
        self.block_interaction = interaction;
        self
    }

    /// Executes the explosion according to vanilla rules.
    pub fn explode(&self) {
        let max_distance = self.radius * 2.0;
        let max_dist_f64 = f64::from(max_distance);

        // Phase 1: Raytraced block calculations
        let mut to_explode = FxHashSet::default();
        if self.block_interaction != ExplosionBlockInteraction::Keep {
            for x in 0..16 {
                for y in 0..16 {
                    for z in 0..16 {
                        if x == 0 || x == 15 || y == 0 || y == 15 || z == 0 || z == 15 {
                            let dir = DVec3::new(
                                x as f64 / 15.0 * 2.0 - 1.0,
                                y as f64 / 15.0 * 2.0 - 1.0,
                                z as f64 / 15.0 * 2.0 - 1.0,
                            );
                            let len = dir.length();
                            if len == 0.0 {
                                continue;
                            }
                            let step_dir = dir / len * 0.3;
                            let mut ray_pos = self.center;
                            let mut ray_strength =
                                self.radius * (0.7 + rand::random::<f32>() * 0.6);

                            while ray_strength > 0.0 {
                                let block_pos = BlockPos::containing(
                                    ray_pos.x, ray_pos.y, ray_pos.z,
                                );
                                if self.world.is_in_valid_bounds(block_pos) {
                                    let block_state = self.world.get_block_state(block_pos);
                                    let fluid_state = block_state.get_fluid_state();

                                    if !block_state.is_air() || !fluid_state.is_empty() {
                                        let block_resistance =
                                            block_state.get_block().config.explosion_resistance;
                                        let fluid_resistance = FLUID_BEHAVIORS
                                            .get_behavior(fluid_state.fluid_id)
                                            .explosion_resistance();
                                        let resistance = block_resistance.max(fluid_resistance);
                                        ray_strength -= (resistance + 0.3) * 0.3;
                                    } else {
                                        ray_strength -= 0.09;
                                    }

                                    if ray_strength > 0.0 && !block_state.is_air() {
                                        to_explode.insert(block_pos);
                                    }
                                }
                                ray_pos += step_dir;
                            }
                        }
                    }
                }
            }
        }

        // Phase 2: Entity damage and knockback
        let search_box = WorldAabb::new(
            self.center.x - max_dist_f64,
            self.center.y - max_dist_f64,
            self.center.z - max_dist_f64,
            self.center.x + max_dist_f64,
            self.center.y + max_dist_f64,
            self.center.z + max_dist_f64,
        );
        let entities = self.world.get_entities_in_aabb(&search_box);

        let default_damage_source = if let Some(source) = &self.source {
            if let Some(player) = source.as_player() {
                DamageSource::environment(&vanilla_damage_types::PLAYER_EXPLOSION)
                    .with_causing_entity(player.id())
                    .with_source_position(self.center)
            } else if let Some(tnt) = source.downcast_ref::<PrimedTntEntity>() {
                if let Some(owner_id) = tnt.owner_id() {
                    let owner_is_player = self
                        .world
                        .get_entity_by_id(owner_id)
                        .map_or(false, |e| e.as_player().is_some());
                    let damage_type = if owner_is_player {
                        &vanilla_damage_types::PLAYER_EXPLOSION
                    } else {
                        &vanilla_damage_types::EXPLOSION
                    };
                    DamageSource::environment(damage_type)
                        .with_causing_entity(owner_id)
                        .with_direct_entity(source.id())
                        .with_source_position(self.center)
                } else {
                    DamageSource::environment(&vanilla_damage_types::EXPLOSION)
                        .with_direct_entity(source.id())
                        .with_source_position(self.center)
                }
            } else {
                DamageSource::environment(&vanilla_damage_types::EXPLOSION)
                    .with_causing_entity(source.id())
                    .with_direct_entity(source.id())
                    .with_source_position(self.center)
            }
        } else {
            DamageSource::environment(&vanilla_damage_types::EXPLOSION)
                .with_source_position(self.center)
        };

        let resolved_damage_source = self
            .damage_source
            .clone()
            .unwrap_or(default_damage_source);

        let mut player_knockbacks: FxHashMap<i32, DVec3> = FxHashMap::default();

        for entity in &entities {
            if entity.is_spectator() {
                continue;
            }

            let eye_pos =
                entity.position() + DVec3::new(0.0, f64::from(entity.get_eye_height()), 0.0);
            let dist = eye_pos.distance(self.center);
            if dist > max_dist_f64 {
                continue;
            }

            let q = dist / max_dist_f64;
            if q > 1.0 {
                continue;
            }

            let exposure = get_exposure(self.center, entity.as_ref(), self.world);
            let impact = (1.0 - q) * f64::from(exposure);

            // Apply damage to living entities
            if let Some(living) = entity.as_living_entity() {
                let damage =
                    ((impact * impact + impact) / 2.0 * 7.0 * max_dist_f64 + 1.0) as f32;
                living.hurt_server(self.world, &resolved_damage_source, damage);
            }

            // Knockback / Impulse calculation
            let blast_res = entity
                .as_living_entity()
                .map_or(0.0, LivingEntity::knockback_resistance);
            let knockback_power = impact * (1.0 - blast_res);
            let diff = (eye_pos - self.center).normalize_or_zero();
            let impulse = diff * knockback_power;

            entity.set_velocity(entity.velocity() + impulse);
            entity.mark_velocity_sync();

            if let Some(player) = entity.as_player() {
                player_knockbacks.insert(player.id(), impulse);
            }

            // Shorten fuse on Primed TNT entities caught in the explosion
            if let Some(tnt) = entity.downcast_ref::<PrimedTntEntity>() {
                let short_fuse = rand::random_range(10..=30);
                if tnt.fuse() > short_fuse {
                    tnt.set_fuse(short_fuse);
                }
            }
        }

        // Phase 3: TNT block chain reactions & block destruction
        let mut tnt_to_spawn = Vec::new();

        for &block_pos in &to_explode {
            let state = self.world.get_block_state(block_pos);
            if state.get_block() == &vanilla_blocks::TNT {
                tnt_to_spawn.push(block_pos);
            }
        }

        // Remove TNT blocks from `to_explode` set
        for block_pos in &tnt_to_spawn {
            to_explode.remove(block_pos);
            self.world.set_block(
                *block_pos,
                vanilla_blocks::AIR.default_state(),
                UpdateFlags::UPDATE_ALL,
            );

            let center = block_pos.0.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
            let short_fuse = rand::random_range(10..=30);
            let igniter_id = self.source.as_ref().and_then(|source| {
                if let Some(tnt) = source.downcast_ref::<PrimedTntEntity>() {
                    tnt.owner_id()
                } else {
                    Some(source.id())
                }
            });
            let _ = PrimedTntEntity::spawn(self.world, center, short_fuse, igniter_id);
        }

        // Destroy non-TNT blocks
        let drop_chance = if self.block_interaction == ExplosionBlockInteraction::DestroyWithDecay
        {
            1.0 / self.radius
        } else {
            1.0
        };

        for &block_pos in &to_explode {
            let state = self.world.get_block_state(block_pos);
            if state.is_air() {
                continue;
            }

            self.world.set_block(
                block_pos,
                vanilla_blocks::AIR.default_state(),
                UpdateFlags::UPDATE_ALL,
            );

            // Item drop check
            if rand::random::<f32>() < drop_chance {
                // Block drops
            }
        }

        // Phase 4: Place fire
        if self.create_fire {
            for &block_pos in &to_explode {
                if rand::random_range(0..3) == 0
                    && self.world.get_block_state(block_pos).is_air()
                    && self.world.get_block_state(block_pos.below()).is_solid()
                {
                    self.world.set_block(
                        block_pos,
                        vanilla_blocks::FIRE.default_state(),
                        UpdateFlags::UPDATE_ALL,
                    );
                }
            }
        }

        // Phase 5: Network packets and sound
        let chunk_pos = ChunkPos::from_entity_pos(self.center);
        let sound_packet = CSound::new(
            self.sound,
            SoundSource::Blocks,
            self.center,
            4.0,
            (1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.2) * 0.7,
            rand::random(),
        );
        self.world.broadcast_to_nearby(chunk_pos, sound_packet, None);

        // Send CExplode to tracking players with their individual knockback
        self.world.players.iter_players(|_uuid, player| {
            let knockback = player_knockbacks.get(&player.id()).copied();
            let explode_packet = CExplode::new(
                self.center,
                knockback,
                self.large_particle.clone(),
                self.sound,
            );
            player.send_packet(explode_packet);
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::init_vanilla_registry;
    use steel_registry::vanilla_entities;
    use std::sync::Weak;
    use crate::entity::entities::PigEntity;

    #[test]
    fn get_exposure_returns_full_exposure_in_air() {
        init_vanilla_registry();
        let pig = PigEntity::new(&vanilla_entities::PIG, 1, DVec3::new(0.0, 64.0, 0.0), Weak::new());
        let world = crate::test_support::fresh_test_world("explosion_exposure");

        let exposure = get_exposure(DVec3::new(2.0, 64.0, 0.0), &pig, &world);
        assert!((exposure - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn explosion_triggers_tnt_block_chain_reaction() {
        init_vanilla_registry();
        let world = crate::test_support::fresh_test_world("explosion_tnt_chain");
        let tnt_pos = BlockPos::new(0, 64, 0);
        world.set_block(tnt_pos, vanilla_blocks::TNT.default_state(), UpdateFlags::UPDATE_ALL);

        let explosion = Explosion::new(&world, DVec3::new(0.5, 64.5, 0.5), 4.0);
        explosion.explode();

        assert!(world.get_block_state(tnt_pos).is_air());
    }
}
