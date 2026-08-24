//! Vanilla Ghast and HappyGhast entity implementations.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::{GhastEntityData, HappyGhastEntityData};
use steel_registry::{sound_events, vanilla_attributes};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity,
    LivingEntityBase, Mob, MobBase, PathfinderMob, SharedEntity,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

#[entity_behavior(class = "Ghast")]
/// Vanilla Ghast entity.
pub struct GhastEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<GhastEntityData>,
}

unsafe impl DowncastType for GhastEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/ghast");
}

impl GhastEntity {
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

        let mut entity_data = GhastEntityData::new();
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

impl Entity for GhastEntity {
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

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
    }
}

impl LivingEntity for GhastEntity {
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
        Some(&sound_events::ENTITY_GHAST_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_GHAST_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.default_ai_step()
    }
}

impl Mob for GhastEntity {
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
        Some(&sound_events::ENTITY_GHAST_AMBIENT)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for GhastEntity {}

#[entity_behavior(class = "HappyGhast")]
/// Vanilla HappyGhast entity.
pub struct HappyGhastEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    harnessed: SyncMutex<bool>,
    entity_data: SyncMutex<HappyGhastEntityData>,
}

unsafe impl DowncastType for HappyGhastEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/happy_ghast");
}

impl HappyGhastEntity {
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
        let ageable_base = AgeableMobBase::new();

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(7, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(8, LookAtPlayerGoal::new(8.0));
            goal_selector.add_goal(8, RandomLookAroundGoal::new());
        }

        let mut entity_data = HappyGhastEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            harnessed: SyncMutex::new(false),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns whether the happy ghast is harnessed.
    #[must_use]
    pub fn is_harnessed(&self) -> bool {
        *self.harnessed.lock()
    }

    /// Sets whether the happy ghast is harnessed.
    pub fn set_harnessed(&self, harnessed: bool) {
        *self.harnessed.lock() = harnessed;
    }

    /// Returns whether the happy ghast stays still.
    #[must_use]
    pub fn stays_still(&self) -> bool {
        *self.entity_data.lock().stays_still.get()
    }

    /// Sets whether the happy ghast stays still.
    pub fn set_stays_still(&self, stays_still: bool) {
        self.entity_data.lock().stays_still.set(stays_still);
    }

    /// Returns whether the happy ghast is a leash holder.
    #[must_use]
    pub fn is_leash_holder(&self) -> bool {
        *self.entity_data.lock().is_leash_holder.get()
    }

    /// Sets whether the happy ghast is a leash holder.
    pub fn set_is_leash_holder(&self, holder: bool) {
        self.entity_data.lock().is_leash_holder.set(holder);
    }

    fn set_ridden_rotation(&self, controller_yaw: f32, controller_pitch: f32) {
        self.set_rotation((controller_yaw, controller_pitch * 0.5));
        self.base.set_old_yaw_to_current();
        let yaw = self.rotation().0;
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);
    }
}

impl Entity for HappyGhastEntity {
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

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {}

    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if self.is_harnessed()
            && let Some(passenger) = self.first_passenger()
            && passenger.as_player().is_some()
        {
            return Some(passenger);
        }
        self.controlling_passenger_mob()
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        nbt.insert("Harness", i8::from(self.is_harnessed()));
        nbt.insert("StaysStill", i8::from(self.stays_still()));
        nbt.insert("IsLeashHolder", i8::from(self.is_leash_holder()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        if let Some(harness) = nbt.byte("Harness") {
            self.set_harnessed(harness != 0);
        }
        if let Some(still) = nbt.byte("StaysStill") {
            self.set_stays_still(still != 0);
        }
        if let Some(holder) = nbt.byte("IsLeashHolder") {
            self.set_is_leash_holder(holder != 0);
        }
    }
}

impl LivingEntity for HappyGhastEntity {
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
        Some(&sound_events::ENTITY_HAPPY_GHAST_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_HAPPY_GHAST_DEATH)
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        match slot {
            EquipmentSlot::Saddle | EquipmentSlot::Body => {
                Entity::is_alive(self) && !AgeableMob::is_baby(self)
            }
            _ => false,
        }
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Saddle || slot == EquipmentSlot::Body
    }

    fn equip_sound(&self, slot: EquipmentSlot, _stack: &ItemStack) -> Option<SoundEventRef> {
        match slot {
            EquipmentSlot::Saddle | EquipmentSlot::Body => {
                Some(&sound_events::ENTITY_HAPPY_GHAST_EQUIP)
            }
            _ => None,
        }
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn tick_ridden(&self, controller: &Player, _ridden_input: DVec3) {
        let (yaw, pitch) = controller.rotation();
        self.set_ridden_rotation(yaw, pitch);
    }

    fn ridden_input(&self, _controller: &Player, _self_input: DVec3) -> DVec3 {
        DVec3::new(0.0, 0.0, 1.0)
    }

    fn ridden_speed(&self, _controller: &Player) -> f32 {
        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED) as f32;
        movement_speed * 0.225
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        result
    }
}

impl AgeableMob for HappyGhastEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }

    fn age_boundary_changed(&self, _baby: bool) {
        self.refresh_dimensions();
    }
}

impl Mob for HappyGhastEntity {
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
        Some(&sound_events::ENTITY_HAPPY_GHAST_AMBIENT)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let stack = inventory.get_item_in_hand(hand);
            stack.copy_with_count(stack.count())
        };

        if !item_stack.is_empty() {
            if LivingEntity::is_equippable_in_slot(self, &item_stack, EquipmentSlot::Saddle)
                || LivingEntity::is_equippable_in_slot(self, &item_stack, EquipmentSlot::Body)
            {
                self.set_harnessed(true);
                return LivingEntity::interact_living_entity_with_equippable(self, player, hand);
            }
        }

        if !AgeableMob::is_baby(self) && !player.is_secondary_use_active() {
            if let Some(world) = self.level()
                && let Some(vehicle) = world.get_entity_by_id(self.id())
            {
                player.start_riding(&vehicle);
            }
            return InteractionResult::Success;
        }

        InteractionResult::Pass
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for HappyGhastEntity {}
