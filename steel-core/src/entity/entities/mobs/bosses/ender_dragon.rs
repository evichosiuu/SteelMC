//! Vanilla Ender Dragon entity (`EnderDragon`).

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::EnderDragonEntityData;
use steel_registry::{sound_events, vanilla_damage_type_tags, vanilla_entities};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, Downcast, DowncastType, DowncastTypeKey};

use crate::entity::damage::DamageSource;
use crate::entity::entities::EndCrystalEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob, RemovalReason, SharedEntity,
};
use crate::physics::MoveResult;
use crate::world::World;

/// Vanilla dragon phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum DragonPhase {
    /// Circling around portal and spikes.
    HoldingPattern = 0,
    /// Flying towards player to strafe.
    StrafePlayer = 1,
    /// Approaching exit portal to land.
    LandingApproach = 2,
    /// Landing onto bedrock exit portal.
    Landing = 3,
    /// Sitting on portal and attacking close players.
    SittingAttacking = 4,
    /// Sitting on portal scanning for players.
    SittingScanning = 5,
    /// Sitting on portal breathing dragon breath.
    SittingFlaming = 6,
    /// Launching upwards from portal after sitting.
    Takeoff = 7,
    /// Charging directly at a target player.
    ChargingPlayer = 8,
    /// Dying sequence (floating, explosions, XP drop).
    Dying = 9,
    /// Hovering in air.
    Hover = 10,
}

struct DragonState {
    health: f32,
    phase: DragonPhase,
    ticks_in_phase: i32,
    dying_ticks: i32,
    nearest_crystal: Option<Weak<dyn Entity>>,
}

/// Vanilla Ender Dragon boss entity.
#[entity_behavior(class = "EnderDragon")]
pub struct EnderDragonEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    mob_flags: SyncMutex<i8>,
    entity_data: SyncMutex<EnderDragonEntityData>,
    dragon_state: SyncMutex<DragonState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EnderDragonEntity`.
unsafe impl DowncastType for EnderDragonEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/ender_dragon");
}

impl EnderDragonEntity {
    /// Maximum health of the Ender Dragon.
    pub const MAX_HEALTH: f32 = 200.0;

    /// Creates a new Ender Dragon entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Creates an Ender Dragon entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            mob_flags: SyncMutex::new(0),
            entity_data: SyncMutex::new(EnderDragonEntityData::new()),
            dragon_state: SyncMutex::new(DragonState {
                health: Self::MAX_HEALTH,
                phase: DragonPhase::HoldingPattern,
                ticks_in_phase: 0,
                dying_ticks: 0,
                nearest_crystal: None,
            }),
        }
    }

    /// Returns the current dragon phase.
    #[must_use]
    pub fn phase(&self) -> DragonPhase {
        self.dragon_state.lock().phase
    }

    /// Sets the dragon phase.
    pub fn set_phase(&self, phase: DragonPhase) {
        {
            let mut state = self.dragon_state.lock();
            if state.phase == phase {
                return;
            }
            state.phase = phase;
            state.ticks_in_phase = 0;
        }
        self.entity_data.lock().phase.set(phase as i32);
    }

    /// Ticks crystal healing logic.
    fn check_crystal_healing(&self, world: &World) {
        if self.get_health() >= Self::MAX_HEALTH {
            return;
        }

        let pos = self.position();
        let search_box = self.bounding_box().inflate(32.0);
        let crystals = world.get_entities_in_aabb_matching(&search_box, |entity| {
            entity.downcast_ref::<EndCrystalEntity>().is_some()
        });

        let mut closest_crystal: Option<SharedEntity> = None;
        let mut closest_dist_sq = f64::MAX;

        for crystal_entity in crystals {
            let dist_sq = pos.distance_squared(crystal_entity.position());
            if dist_sq < closest_dist_sq {
                closest_dist_sq = dist_sq;
                closest_crystal = Some(crystal_entity);
            }
        }

        if let Some(crystal_entity) = closest_crystal
            && let Some(crystal) = crystal_entity.downcast_ref::<EndCrystalEntity>()
        {
            crystal.set_beam_target(Some(BlockPos::containing(pos.x, pos.y, pos.z)));
            self.set_health((self.get_health() + 1.0).min(Self::MAX_HEALTH));
            self.dragon_state.lock().nearest_crystal = Some(Arc::downgrade(&crystal_entity));
        }
    }

    fn tick_phase_ai(&self, world: &Arc<World>) {
        let (phase, ticks) = {
            let mut state = self.dragon_state.lock();
            state.ticks_in_phase += 1;
            (state.phase, state.ticks_in_phase)
        };

        match phase {
            DragonPhase::HoldingPattern => {
                let angle = (ticks as f64) * 0.05;
                let target = DVec3::new(angle.cos() * 40.0, 75.0, angle.sin() * 40.0);
                let dir = (target - self.position()).normalize_or_zero();
                self.set_velocity(dir * 0.5);

                if ticks > 200 {
                    let roll = rand::random::<u8>() % 3;
                    match roll {
                        0 => self.set_phase(DragonPhase::LandingApproach),
                        1 => self.set_phase(DragonPhase::StrafePlayer),
                        _ => self.set_phase(DragonPhase::ChargingPlayer),
                    }
                }
            }
            DragonPhase::StrafePlayer => {
                let dragon_pos = self.position();
                world.players.iter_players(|_uuid, player| {
                    let target_pos = player.position();
                    let fireball = Arc::new(crate::entity::entities::DragonFireballEntity::new(
                        &vanilla_entities::DRAGON_FIREBALL,
                        crate::entity::next_entity_id(),
                        dragon_pos,
                        Arc::downgrade(world),
                    ));
                    fireball.set_velocity((target_pos - dragon_pos).normalize_or_zero() * 1.0);
                    let _ = world.try_add_entity(fireball);
                    false
                });
                self.set_phase(DragonPhase::HoldingPattern);
            }
            DragonPhase::ChargingPlayer => {
                let target = DVec3::new(0.0, 50.0, 0.0);
                let dir = (target - self.position()).normalize_or_zero();
                self.set_velocity(dir * 0.8);
                if ticks > 100 || self.position().distance(target) < 5.0 {
                    self.set_phase(DragonPhase::HoldingPattern);
                }
            }
            DragonPhase::LandingApproach => {
                let target = DVec3::new(0.0, 48.0, 0.0);
                let dist = self.position().distance(target);
                if dist < 3.0 {
                    self.set_phase(DragonPhase::Landing);
                } else {
                    let dir = (target - self.position()).normalize_or_zero();
                    self.set_velocity(dir * 0.4);
                }
            }
            DragonPhase::Landing => {
                let target = DVec3::new(0.0, 45.0, 0.0);
                if self.position().distance(target) < 1.0 {
                    self.set_phase(DragonPhase::SittingScanning);
                } else {
                    let dir = (target - self.position()).normalize_or_zero();
                    self.set_velocity(dir * 0.2);
                }
            }
            DragonPhase::SittingScanning | DragonPhase::SittingAttacking | DragonPhase::SittingFlaming => {
                self.set_velocity(DVec3::ZERO);
                if ticks > 100 {
                    self.set_phase(DragonPhase::Takeoff);
                }
            }
            DragonPhase::Takeoff => {
                self.set_velocity(DVec3::new(0.0, 0.5, 0.0));
                if ticks > 40 {
                    self.set_phase(DragonPhase::HoldingPattern);
                }
            }
            DragonPhase::Dying => {
                self.set_velocity(DVec3::new(0.0, 0.1, 0.0));
                let dying_ticks = {
                    let mut state = self.dragon_state.lock();
                    state.dying_ticks += 1;
                    state.dying_ticks
                };
                if dying_ticks >= 200 {
                    world.play_sound_at(
                        &sound_events::ENTITY_ENDER_DRAGON_DEATH,
                        SoundSource::Hostile,
                        self.position(),
                        5.0,
                        1.0,
                        None,
                    );
                    if let Some(fight) = world.ender_dragon_fight() {
                        fight.on_dragon_killed(self);
                    }
                    self.set_removed(RemovalReason::Killed);
                }
            }
            _ => {}
        }
    }
}

impl Entity for EnderDragonEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("DragonPhase", self.phase() as i32);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(phase_id) = nbt.int("DragonPhase") {
            let phase = match phase_id {
                0 => DragonPhase::HoldingPattern,
                1 => DragonPhase::StrafePlayer,
                2 => DragonPhase::LandingApproach,
                3 => DragonPhase::Landing,
                4 => DragonPhase::SittingAttacking,
                5 => DragonPhase::SittingScanning,
                6 => DragonPhase::SittingFlaming,
                7 => DragonPhase::Takeoff,
                8 => DragonPhase::ChargingPlayer,
                9 => DragonPhase::Dying,
                _ => DragonPhase::HoldingPattern,
            };
            self.set_phase(phase);
        }
    }

    fn hurt(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        let phase = self.phase();
        if phase == DragonPhase::Dying {
            return false;
        }

        let is_projectile = source.is(&vanilla_damage_type_tags::DamageTypeTag::IS_PROJECTILE)
            || (source.direct_entity_id.is_some() && source.causing_entity_id != source.direct_entity_id);

        if (phase == DragonPhase::Landing
            || phase == DragonPhase::SittingScanning
            || phase == DragonPhase::SittingFlaming
            || phase == DragonPhase::SittingAttacking)
            && is_projectile
        {
            return false;
        }

        let new_health = (self.get_health() - amount).max(0.0);
        self.set_health(new_health);

        world.play_sound_at(
            &sound_events::ENTITY_ENDER_DRAGON_HURT,
            SoundSource::Hostile,
            self.position(),
            5.0,
            1.0,
            None,
        );

        if let Some(fight) = world.ender_dragon_fight() {
            fight.update_dragon_health(new_health);
        }

        if new_health <= 0.0 {
            self.set_phase(DragonPhase::Dying);
        }

        true
    }
}

impl LivingEntity for EnderDragonEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn get_health(&self) -> f32 {
        self.dragon_state.lock().health
    }

    fn set_health(&self, health: f32) {
        let max_health = Self::MAX_HEALTH;
        let clamped = health.clamp(0.0, max_health);
        self.dragon_state.lock().health = clamped;
    }

    fn sound_volume(&self) -> f32 {
        5.0
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDER_DRAGON_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDER_DRAGON_DEATH)
    }

    fn server_ai_step(&self) {
        if let Some(world) = self.level() {
            self.check_crystal_healing(&world);
            self.tick_phase_ai(&world);
        }
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.default_ai_step()
    }
}

impl Mob for EnderDragonEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn mob_flags(&self) -> i8 {
        *self.mob_flags.lock()
    }

    fn set_mob_flags(&self, flags: i8) {
        *self.mob_flags.lock() = flags;
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDER_DRAGON_AMBIENT)
    }
}

impl PathfinderMob for EnderDragonEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    #[test]
    fn ender_dragon_initial_phase() {
        init_vanilla_registry();
        let dragon = EnderDragonEntity::new(
            &vanilla_entities::ENDER_DRAGON,
            1,
            DVec3::ZERO,
            Weak::new(),
        );
        assert_eq!(dragon.phase(), DragonPhase::HoldingPattern);
        assert_eq!(dragon.get_health(), EnderDragonEntity::MAX_HEALTH);
    }
}
