//! Vanilla ZombieHorse entity implementation.

use std::str::FromStr;
use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::ZombieHorseEntityData;
use steel_registry::{sound_events, vanilla_attributes};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    FloatGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad,
    EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob, SharedEntity,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

const FLAG_TAMED: i8 = 0x02;
const FLAG_SADDLED: i8 = 0x04;

/// Vanilla zombie horse entity.
#[entity_behavior(class = "ZombieHorse")]
/// Vanilla ZombieHorse entity.
pub struct ZombieHorseEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    owner_uuid: SyncMutex<Option<Uuid>>,
    entity_data: SyncMutex<ZombieHorseEntityData>,
}

unsafe impl DowncastType for ZombieHorseEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/zombie_horse");
}

impl ZombieHorseEntity {
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
        let animal_base = AnimalBase::new();
        AnimalBase::initialize_pathfinding_malus(&mob_base);

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(1, PanicGoal::new(1.25));
            goal_selector.add_goal(5, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(6, LookAtPlayerGoal::new(6.0));
            goal_selector.add_goal(7, RandomLookAroundGoal::new());
        }

        let mut entity_data = ZombieHorseEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            owner_uuid: SyncMutex::new(None),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns whether the horse is tamed.
    #[must_use]
    pub fn is_tamed(&self) -> bool {
        (self.get_horse_flags() & FLAG_TAMED) != 0
    }

    /// Sets whether the horse is tamed.
    pub fn set_tamed(&self, tamed: bool) {
        self.set_horse_flag(FLAG_TAMED, tamed);
    }

    /// Returns whether the horse is saddled.
    #[must_use]
    pub fn is_saddled(&self) -> bool {
        (self.get_horse_flags() & FLAG_SADDLED) != 0
    }

    /// Sets whether the horse is saddled.
    pub fn set_saddled(&self, saddled: bool) {
        self.set_horse_flag(FLAG_SADDLED, saddled);
    }

    fn get_horse_flags(&self) -> i8 {
        *self.entity_data.lock().abstract_horse().id_flags.get()
    }

    fn set_horse_flag(&self, flag: i8, set: bool) {
        let mut data = self.entity_data.lock();
        let current = *data.abstract_horse().id_flags.get();
        let next = if set {
            current | flag
        } else {
            current & !flag
        };
        data.abstract_horse_mut().id_flags.set(next);
    }

    fn set_ridden_rotation(&self, controller_yaw: f32, controller_pitch: f32) {
        self.set_rotation((controller_yaw, controller_pitch * 0.5));
        self.base.set_old_yaw_to_current();
        let yaw = self.rotation().0;
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);
    }
}

impl Entity for ZombieHorseEntity {
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

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        self.play_sound(&sound_events::ENTITY_HORSE_STEP, 0.15, 1.0);
    }

    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if self.is_saddled()
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
        self.save_animal(nbt);
        nbt.insert("Tame", i8::from(self.is_tamed()));
        nbt.insert("Saddled", i8::from(self.is_saddled()));
        if let Some(owner) = *self.owner_uuid.lock() {
            nbt.insert("Owner", owner.to_string());
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        if let Some(tame) = nbt.byte("Tame") {
            self.set_tamed(tame != 0);
        }
        if let Some(saddled) = nbt.byte("Saddled") {
            self.set_saddled(saddled != 0);
        }
        if let Some(owner_str) = nbt.string("Owner")
            && let Ok(uuid) = Uuid::from_str(owner_str.to_str().as_ref())
        {
            *self.owner_uuid.lock() = Some(uuid);
        }
    }
}

impl LivingEntity for ZombieHorseEntity {
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
        Some(&sound_events::ENTITY_ZOMBIE_HORSE_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ZOMBIE_HORSE_DEATH)
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        match slot {
            EquipmentSlot::Saddle => Entity::is_alive(self) && !AgeableMob::is_baby(self),
            _ => false,
        }
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Saddle
    }

    fn equip_sound(&self, slot: EquipmentSlot, _stack: &ItemStack) -> Option<SoundEventRef> {
        match slot {
            EquipmentSlot::Saddle => Some(&sound_events::ENTITY_HORSE_SADDLE),
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
        Animal::tick_animal_love(self);
        result
    }
}

impl AgeableMob for ZombieHorseEntity {
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

impl Animal for ZombieHorseEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }
}

impl Mob for ZombieHorseEntity {
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
        Some(&sound_events::ENTITY_ZOMBIE_HORSE_AMBIENT)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let stack = inventory.get_item_in_hand(hand);
            stack.copy_with_count(stack.count())
        };

        if !item_stack.is_empty()
            && LivingEntity::is_equippable_in_slot(self, &item_stack, EquipmentSlot::Saddle)
        {
            self.set_saddled(true);
            return LivingEntity::interact_living_entity_with_equippable(self, player, hand);
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

impl PathfinderMob for ZombieHorseEntity {}
