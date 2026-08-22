//! Vanilla Endermite entity with AI goals and lifetime despawning behavior.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::EndermiteEntityData;
use steel_registry::{sound_events, vanilla_attributes};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob, RemovalReason,
};
use crate::physics::MoveResult;
use crate::world::World;

const DEFAULT_STEP_HEIGHT: f32 = 0.6;
/// Maximum lifespan for an Endermite (2 minutes / 2400 ticks) before it despawns.
const MAX_LIFETIME: i32 = 2400;

#[entity_behavior(class = "Endermite")]
/// Vanilla Endermite entity that attacks players and despawns after 2 minutes.
pub struct EndermiteEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<EndermiteEntityData>,
    life: SyncMutex<i32>,
    player_spawned: SyncMutex<bool>,
    health: SyncMutex<f32>,
    mob_flags: SyncMutex<i8>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EndermiteEntity`.
unsafe impl DowncastType for EndermiteEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/endermite");
}

impl EndermiteEntity {
    /// Creates a new Endermite at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs an Endermite from persisted base entity state.
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
        let mut entity_data = EndermiteEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(1, FloatGoal::new(&mob_base));
            goal_selector.add_goal(2, MeleeAttackGoal::new(1.0, false));
            goal_selector.add_goal(7, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(8, LookAtPlayerGoal::new(8.0));
            goal_selector.add_goal(9, RandomLookAroundGoal::new());

            let mut target_selector = mob_base.target_selector().lock();
            target_selector.add_goal(1, HurtByTargetGoal::new());
            target_selector.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_target, _world| true),
            );
        }

        let max_health = living_base
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MAX_HEALTH) as f32;

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            life: SyncMutex::new(0),
            player_spawned: SyncMutex::new(false),
            health: SyncMutex::new(max_health),
            mob_flags: SyncMutex::new(0),
        }
    }

    /// Returns whether this Endermite was spawned by an Ender Pearl thrown by a player.
    #[must_use]
    pub fn is_player_spawned(&self) -> bool {
        *self.player_spawned.lock()
    }

    /// Sets whether this Endermite was spawned by an Ender Pearl thrown by a player.
    pub fn set_player_spawned(&self, player_spawned: bool) {
        *self.player_spawned.lock() = player_spawned;
    }

    /// Returns the current lifetime ticks of this Endermite.
    #[must_use]
    pub fn life(&self) -> i32 {
        *self.life.lock()
    }

    /// Sets the lifetime ticks of this Endermite.
    pub fn set_life(&self, life: i32) {
        *self.life.lock() = life;
    }

    fn check_lifetime_despawn(&self) {
        let mut life = self.life.lock();
        *life += 1;
        if *life >= MAX_LIFETIME {
            self.set_removed(RemovalReason::Discarded);
        }
    }
}

impl Entity for EndermiteEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn max_up_step(&self) -> f32 {
        self.attributes()
            .lock()
            .get_value(vanilla_attributes::STEP_HEIGHT)
            .unwrap_or(f64::from(DEFAULT_STEP_HEIGHT)) as f32
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_ENDERMITE_STEP, 0.15, 1.0);
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("Lifetime", *self.life.lock());
        nbt.insert("PlayerSpawned", self.is_player_spawned());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(life) = nbt.int("Lifetime") {
            *self.life.lock() = life;
        }
        self.set_player_spawned(nbt.byte("PlayerSpawned").is_some_and(|b| b != 0));
    }
}

impl LivingEntity for EndermiteEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn get_health(&self) -> f32 {
        *self.health.lock()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        *self.health.lock() = clamped;
    }

    fn sound_volume(&self) -> f32 {
        0.4
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMITE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMITE_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.check_lifetime_despawn();
        self.default_ai_step()
    }
}

impl Mob for EndermiteEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMITE_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.mob_flags.lock()
    }

    fn set_mob_flags(&self, flags: i8) {
        *self.mob_flags.lock() = flags;
    }
}

impl PathfinderMob for EndermiteEntity {}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    use crate::behavior::init_behaviors;
    use crate::entity::{Entity, LivingEntity};
    use crate::world::World;

    use super::EndermiteEntity;

    #[test]
    fn endermite_despawns_after_max_lifetime() {
        init_vanilla_registry();
        init_behaviors();

        let endermite = EndermiteEntity::new(
            &vanilla_entities::ENDERMITE,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        endermite.set_life(2399);
        assert!(!endermite.is_removed());

        endermite.ai_step();
        assert!(endermite.is_removed(), "Endermite should be discarded when lifetime reaches 2400");
    }

    #[test]
    fn endermite_player_spawned_flag() {
        init_vanilla_registry();
        init_behaviors();

        let endermite = EndermiteEntity::new(
            &vanilla_entities::ENDERMITE,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );

        assert!(!endermite.is_player_spawned());
        endermite.set_player_spawned(true);
        assert!(endermite.is_player_spawned());
    }
}
