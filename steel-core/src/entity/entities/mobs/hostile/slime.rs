//! Vanilla Slime entity implementation.

use std::sync::Weak;
use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::SlimeEntityData;
use steel_registry::sound_events;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob,
};
use crate::physics::MoveResult;
use crate::world::World;

#[entity_behavior(class = "Slime")]
pub struct SlimeEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<SlimeEntityData>,
}

unsafe impl DowncastType for SlimeEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/slime");
}

impl SlimeEntity {
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

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

        let mut entity_data = SlimeEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }
}

impl Entity for SlimeEntity {
    fn base(&self) -> &EntityBase { &self.base }
    fn entity_type(&self) -> EntityTypeRef { self.entity_type }
    fn synced_data(&self) -> Option<&dyn EntitySyncedData> { Some(&self.entity_data) }
    fn base_tick(&self) { Mob::base_tick_mob(self); }
    fn sound_source(&self) -> SoundSource { SoundSource::Hostile }
    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}
    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }
    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for SlimeEntity {
    fn living_base(&self) -> &LivingEntityBase { &self.living_base }
    fn get_health(&self) -> f32 { *self.entity_data.lock().living_entity().health.get() }
    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data.lock().living_entity_mut().health.set(clamped);
    }
    fn sound_volume(&self) -> f32 { 0.4 }
    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> { Some(&sound_events::ENTITY_SLIME_HURT) }
    fn death_sound(&self) -> Option<SoundEventRef> { Some(&sound_events::ENTITY_SLIME_DEATH) }
    fn server_ai_step(&self) { Mob::mob_server_ai_step(self); }
    fn ai_step(&self) -> Option<MoveResult> { self.default_ai_step() }
}

impl Mob for SlimeEntity {
    fn mob_base(&self) -> &MobBase { &self.mob_base }
    fn tick_goal_selectors(&self) { PathfinderMob::tick_pathfinder_goal_selectors(self); }
    fn tick_path_navigation(&self) { PathfinderMob::tick_pathfinder_path_navigation(self); }
    fn ambient_sound(&self) -> Option<SoundEventRef> { None }
    fn mob_flags(&self) -> i8 { *self.entity_data.lock().mob().mob_flags.get() }
    fn set_mob_flags(&self, flags: i8) { self.entity_data.lock().mob_mut().mob_flags.set(flags); }
}

impl PathfinderMob for SlimeEntity {}
