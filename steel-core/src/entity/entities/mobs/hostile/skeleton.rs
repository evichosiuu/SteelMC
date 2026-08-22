//! Vanilla Skeleton entity with AI goals and behavior.

use std::sync::Arc;
use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::SkeletonEntityData;
use steel_registry::{sound_events, vanilla_attributes, vanilla_enchantments, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::types::Difficulty;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    FleeSunGoal, FloatGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SpawnGroupData,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::physics::MoveResult;
use crate::world::World;

const DEFAULT_STEP_HEIGHT: f32 = 0.6;

#[entity_behavior(class = "Skeleton")]
/// Vanilla skeleton entity with AI goals and daylight burning behavior.
pub struct SkeletonEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<SkeletonEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `SkeletonEntity`.
unsafe impl DowncastType for SkeletonEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/skeleton");
}

impl SkeletonEntity {
    /// Creates a new skeleton at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs a skeleton from persisted base entity state.
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
            goal_selector.add_goal(2, FleeSunGoal::new(1.0));
            goal_selector.add_goal(4, MeleeAttackGoal::new(1.2, false));
            goal_selector.add_goal(5, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(6, LookAtPlayerGoal::new(8.0));
            goal_selector.add_goal(6, RandomLookAroundGoal::new());

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

        let mut entity_data = SkeletonEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);
        entity_data
            .living_entity_mut()
            .health
            .set(max_health);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
        }
    }

    fn check_sun_burn(&self) {
        let Some(world) = self.level() else {
            return;
        };

        if !world.is_bright_outside() {
            return;
        }

        let pos = self.block_position();
        if !world.can_see_sky(pos) {
            return;
        }

        if self.is_on_fire() {
            return;
        }

        self.set_remaining_fire_ticks(160);
    }
}

impl Entity for SkeletonEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
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
        self.play_sound(&sound_events::ENTITY_SKELETON_STEP, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for SkeletonEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn sound_volume(&self) -> f32 {
        0.4
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SKELETON_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SKELETON_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.check_sun_burn();
        self.default_ai_step()
    }
}

impl Mob for SkeletonEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let result = self.finalize_spawn_mob_base(world, spawn_reason, group_data);

        let mut bow = ItemStack::new(&vanilla_items::BOW);
        let difficulty = world.difficulty();

        // Vanilla equips weapon and rolls for enchanted weapon based on difficulty.
        let enchant_chance = match difficulty {
            Difficulty::Peaceful => 0.0,
            Difficulty::Easy => 0.05,
            Difficulty::Normal => 0.15,
            Difficulty::Hard => 0.25,
        };

        if enchant_chance > 0.0 && rand::random::<f32>() < enchant_chance {
            let possible_enchantments = [
                (&vanilla_enchantments::POWER.key, 1..=3),
                (&vanilla_enchantments::PUNCH.key, 1..=2),
                (&vanilla_enchantments::FLAME.key, 1..=1),
                (&vanilla_enchantments::UNBREAKING.key, 1..=3),
            ];
            let idx = rand::random_range(0..possible_enchantments.len());
            let (enchantment_key, ref level_range) = possible_enchantments[idx];
            let level = rand::random_range(level_range.clone());
            bow.upgrade_enchantment(enchantment_key.clone(), level);
        }

        self.living_base()
            .equipment()
            .lock()
            .set(EquipmentSlot::MainHand, bow);

        result
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_SKELETON_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for SkeletonEntity {}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::entity_data::EntityData;
    use steel_registry::vanilla_entities;

    use crate::entity::Entity;

    use super::SkeletonEntity;

    #[test]
    fn skeleton_on_fire_syncs_dirty_entity_data_with_on_fire_flag() {
        let skeleton = SkeletonEntity::new(&vanilla_entities::SKELETON, 1, DVec3::ZERO, Weak::new());
        assert!(!skeleton.is_on_fire());

        skeleton.set_remaining_fire_ticks(160);
        assert!(skeleton.is_on_fire());

        let dirty = skeleton
            .pack_dirty_entity_data()
            .expect("expected dirty entity data when skeleton set on fire");
        let flags_entry = dirty.iter().find(|val| val.index == 0).expect("expected metadata index 0");
        if let EntityData::Byte(b) = flags_entry.value {
            assert_ne!(b & 1, 0, "expected ON_FIRE bit set in metadata index 0");
        } else {
            panic!("expected Byte metadata value for index 0");
        }
    }
}
