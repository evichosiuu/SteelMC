//! Vanilla Hoglin entity implementation.

use std::sync::{Arc, Weak};
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::HoglinEntityData;
use steel_registry::{sound_events, vanilla_entities};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, Downcast, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::mobs::hostile::ZoglinEntity;
use crate::entity::{
    ENTITIES, Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase,
    Mob, MobBase, PathfinderMob, RemovalReason, next_entity_id,
};
use crate::physics::MoveResult;
use crate::world::World;

#[entity_behavior(class = "Hoglin")]
/// Vanilla Hoglin entity.
pub struct HoglinEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<HoglinEntityData>,
    time_in_overworld: SyncMutex<i32>,
}

unsafe impl DowncastType for HoglinEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/hoglin");
}

impl HoglinEntity {
    /// Creates a new entity instance at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs an entity instance from saved NBT data.
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

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(2, MeleeAttackGoal::new(1.0, false));
            goal_selector.add_goal(7, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(8, LookAtPlayerGoal::new(8.0));
            goal_selector.add_goal(8, RandomLookAroundGoal::new());

            let mut target_selector = mob_base.target_selector().lock();
            target_selector.add_goal(1, HurtByTargetGoal::new());
            target_selector.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |target, _world| {
                    target.can_be_seen_as_enemy()
                }),
            );
        }

        let mut entity_data = HoglinEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            time_in_overworld: SyncMutex::new(0),
        }
    }

    /// Returns time spent in overworld before zombifying.
    #[must_use]
    pub fn time_in_overworld(&self) -> i32 {
        *self.time_in_overworld.lock()
    }

    /// Sets time spent in overworld before zombifying.
    pub fn set_time_in_overworld(&self, time: i32) {
        *self.time_in_overworld.lock() = time;
    }

    /// Returns whether this hoglin is immune to zombification.
    #[must_use]
    pub fn is_immune_to_zombification(&self) -> bool {
        *self
            .entity_data
            .lock()
            .hoglin()
            .immune_to_zombification
            .get()
    }

    /// Sets whether this hoglin is immune to zombification.
    pub fn set_immune_to_zombification(&self, immune: bool) {
        self.entity_data
            .lock()
            .hoglin_mut()
            .immune_to_zombification
            .set(immune);
    }

    /// Returns whether this hoglin is a baby.
    #[must_use]
    pub fn is_baby(&self) -> bool {
        *self.entity_data.lock().ageable_mob().baby.get()
    }

    /// Sets whether this hoglin is a baby.
    pub fn set_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }

    fn tick_zombification(&self) {
        if !LivingEntity::is_alive(self) || self.is_removed() {
            return;
        }

        if self.is_immune_to_zombification() {
            self.set_time_in_overworld(0);
            return;
        }

        let Some(world) = self.level() else {
            return;
        };

        if world.dimension_type.piglins_zombify {
            let time = self.time_in_overworld() + 1;
            self.set_time_in_overworld(time);
            if time > 300 {
                self.zombify(&world);
            }
        } else {
            let time = self.time_in_overworld();
            if time > 0 {
                self.set_time_in_overworld(time - 1);
            }
        }
    }

    fn zombify(&self, world: &Arc<World>) {
        let pos = self.position();
        let world_weak = Arc::downgrade(world);

        let Some(zoglin) = ENTITIES.create(
            &vanilla_entities::ZOGLIN,
            next_entity_id(),
            pos,
            world_weak,
        ) else {
            return;
        };

        zoglin.set_rotation(self.rotation());
        zoglin.set_velocity(self.velocity());

        if let Some(custom_name) = self.custom_name() {
            zoglin.set_custom_name(Some(custom_name));
        }
        zoglin.set_custom_name_visible(self.is_custom_name_visible());

        if self.is_persistence_required() {
            if let Some(mob) = zoglin.as_mob() {
                mob.set_persistence_required();
            }
        }

        if self.is_baby() {
            if let Some(zoglin_entity) = zoglin.as_ref().downcast_ref::<ZoglinEntity>() {
                zoglin_entity.set_baby(true);
            }
        }

        self.play_sound(&sound_events::ENTITY_HOGLIN_CONVERTED_TO_ZOMBIFIED, 1.0, 1.0);
        self.set_removed(RemovalReason::Killed);

        if let Err(error) = world.try_add_entity(zoglin) {
            log::debug!("failed to spawn zoglin: {error}");
        }
    }
}

impl Entity for HoglinEntity {
    fn base(&self) -> &EntityBase { &self.base }
    fn entity_type(&self) -> EntityTypeRef { self.entity_type }
    fn synced_data(&self) -> Option<&dyn EntitySyncedData> { Some(&self.entity_data) }
    fn base_tick(&self) {
        Mob::base_tick_mob(self);
        self.tick_zombification();
    }
    fn sound_source(&self) -> SoundSource { SoundSource::Hostile }
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("TimeInOverworld", self.time_in_overworld());
        if self.is_immune_to_zombification() {
            nbt.insert("IsImmuneToZombification", true);
        }
        if self.is_baby() {
            nbt.insert("IsBaby", true);
        }
    }
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(time) = nbt.int("TimeInOverworld") {
            self.set_time_in_overworld(time);
        }
        let immune = nbt
            .byte("IsImmuneToZombification")
            .map_or(false, |b| b != 0);
        self.set_immune_to_zombification(immune);
        let baby = nbt
            .byte("IsBaby")
            .map_or(false, |b| b != 0);
        self.set_baby(baby);
    }
}

impl LivingEntity for HoglinEntity {
    fn living_base(&self) -> &LivingEntityBase { &self.living_base }
    fn get_health(&self) -> f32 { *self.entity_data.lock().living_entity().health.get() }
    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data.lock().living_entity_mut().health.set(clamped);
    }
    fn sound_volume(&self) -> f32 { 0.4 }
    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> { Some(&sound_events::ENTITY_HOGLIN_HURT) }
    fn death_sound(&self) -> Option<SoundEventRef> { Some(&sound_events::ENTITY_HOGLIN_DEATH) }
    fn server_ai_step(&self) { Mob::mob_server_ai_step(self); }
    fn ai_step(&self) -> Option<MoveResult> { self.default_ai_step() }
}

impl Mob for HoglinEntity {
    fn mob_base(&self) -> &MobBase { &self.mob_base }
    fn tick_goal_selectors(&self) { PathfinderMob::tick_pathfinder_goal_selectors(self); }
    fn tick_path_navigation(&self) { PathfinderMob::tick_pathfinder_path_navigation(self); }
    fn ambient_sound(&self) -> Option<SoundEventRef> { Some(&sound_events::ENTITY_HOGLIN_AMBIENT) }
    fn mob_flags(&self) -> i8 { *self.entity_data.lock().mob().mob_flags.get() }
    fn set_mob_flags(&self, flags: i8) { self.entity_data.lock().mob_mut().mob_flags.set(flags); }
}

impl PathfinderMob for HoglinEntity {}
