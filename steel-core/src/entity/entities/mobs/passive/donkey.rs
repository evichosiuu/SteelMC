//! Vanilla Donkey entity implementation.

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
use steel_registry::vanilla_entity_data::DonkeyEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{REGISTRY, TaggedRegistryExt, sound_events, vanilla_attributes, vanilla_items};
use steel_utils::entity_events::EntityStatus;
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};
use uuid::Uuid;

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    TemptGoal, WaterAvoidingRandomStrollGoal,
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

/// Vanilla donkey entity.
#[entity_behavior(class = "Donkey")]
/// Vanilla Donkey entity.
pub struct DonkeyEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    temper: SyncMutex<i32>,
    owner_uuid: SyncMutex<Option<Uuid>>,
    entity_data: SyncMutex<DonkeyEntityData>,
}

unsafe impl DowncastType for DonkeyEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/donkey");
}

impl DonkeyEntity {
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
            goal_selector.add_goal(3, BreedGoal::new(1.0));
            goal_selector.add_goal(
                4,
                TemptGoal::new(
                    1.2,
                    |item_stack| {
                        REGISTRY
                            .items
                            .is_in_tag(item_stack.item(), &ItemTag::HORSE_FOOD)
                    },
                    false,
                ),
            );
            goal_selector.add_goal(5, FollowParentGoal::new(1.1));
            goal_selector.add_goal(6, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(7, LookAtPlayerGoal::new(6.0));
            goal_selector.add_goal(8, RandomLookAroundGoal::new());
        }

        let mut entity_data = DonkeyEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            temper: SyncMutex::new(0),
            owner_uuid: SyncMutex::new(None),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns whether the donkey has a chest equipped.
    #[must_use]
    pub fn has_chest(&self) -> bool {
        *self
            .entity_data
            .lock()
            .abstract_chested_horse()
            .id_chest
            .get()
    }

    /// Sets whether the donkey has a chest equipped.
    pub fn set_has_chest(&self, chest: bool) {
        self.entity_data
            .lock()
            .abstract_chested_horse_mut()
            .id_chest
            .set(chest);
    }

    /// Returns whether the donkey is tamed.
    #[must_use]
    pub fn is_tamed(&self) -> bool {
        (self.get_horse_flags() & FLAG_TAMED) != 0
    }

    /// Sets whether the donkey is tamed.
    pub fn set_tamed(&self, tamed: bool) {
        self.set_horse_flag(FLAG_TAMED, tamed);
    }

    /// Returns whether the donkey is saddled.
    #[must_use]
    pub fn is_saddled(&self) -> bool {
        (self.get_horse_flags() & FLAG_SADDLED) != 0
    }

    /// Sets whether the donkey is saddled.
    pub fn set_saddled(&self, saddled: bool) {
        self.set_horse_flag(FLAG_SADDLED, saddled);
    }

    /// Returns the donkey's current temper.
    #[must_use]
    pub fn temper(&self) -> i32 {
        *self.temper.lock()
    }

    /// Sets the donkey's temper.
    pub fn set_temper(&self, temper: i32) {
        *self.temper.lock() = temper;
    }

    /// Increases temper by delta and returns the new temper value.
    pub fn modify_temper(&self, delta: i32) -> i32 {
        let mut lock = self.temper.lock();
        *lock = (*lock + delta).clamp(0, 100);
        *lock
    }

    /// Returns the owner's UUID if tamed.
    #[must_use]
    pub fn owner_uuid(&self) -> Option<Uuid> {
        *self.owner_uuid.lock()
    }

    /// Sets the owner's UUID.
    pub fn set_owner_uuid(&self, uuid: Option<Uuid>) {
        *self.owner_uuid.lock() = uuid;
    }

    fn get_horse_flags(&self) -> i8 {
        *self
            .entity_data
            .lock()
            .abstract_chested_horse()
            .abstract_horse()
            .id_flags
            .get()
    }

    fn set_horse_flag(&self, flag: i8, set: bool) {
        let mut data = self.entity_data.lock();
        let current = *data
            .abstract_chested_horse()
            .abstract_horse()
            .id_flags
            .get();
        let next = if set {
            current | flag
        } else {
            current & !flag
        };
        data.abstract_chested_horse_mut()
            .abstract_horse_mut()
            .id_flags
            .set(next);
    }

    fn set_ridden_rotation(&self, controller_yaw: f32, controller_pitch: f32) {
        self.set_rotation((controller_yaw, controller_pitch * 0.5));
        self.base.set_old_yaw_to_current();
        let yaw = self.rotation().0;
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);
    }

    fn handle_eating_and_temper_item(&self, player: &Player, item_stack: &ItemStack) -> bool {
        let mut healed = false;
        let mut age_delta = 0;
        let mut temper_delta = 0;

        if item_stack.is(&vanilla_items::SUGAR) {
            healed = true;
            age_delta = 30;
            temper_delta = 3;
        } else if item_stack.is(&vanilla_items::WHEAT) {
            healed = true;
            age_delta = 20;
            temper_delta = 3;
        } else if item_stack.is(&vanilla_items::APPLE) {
            healed = true;
            age_delta = 60;
            temper_delta = 3;
        } else if item_stack.is(&vanilla_items::GOLDEN_CARROT) {
            healed = true;
            age_delta = 60;
            temper_delta = 5;
            if self.is_tamed() && self.get_age() == 0 && !Animal::is_in_love(self) {
                Animal::set_in_love(self, Some(player));
            }
        } else if item_stack.is(&vanilla_items::GOLDEN_APPLE)
            || item_stack.is(&vanilla_items::ENCHANTED_GOLDEN_APPLE)
        {
            healed = true;
            age_delta = 240;
            temper_delta = 10;
            if self.is_tamed() && self.get_age() == 0 && !Animal::is_in_love(self) {
                Animal::set_in_love(self, Some(player));
            }
        } else if item_stack.is(&vanilla_items::HAY_BLOCK) {
            healed = true;
            age_delta = 180;
        }

        if self.get_health() < self.get_max_health() && healed {
            self.heal(2.0);
        }

        if AgeableMob::is_baby(self) && age_delta > 0 {
            AgeableMob::age_up(self, age_delta, true);
        }

        if !self.is_tamed() && temper_delta > 0 {
            self.modify_temper(temper_delta);
        }

        if healed {
            self.play_sound(&sound_events::ENTITY_DONKEY_EAT, 1.0, 1.0);
            return true;
        }

        false
    }
}

impl Entity for DonkeyEntity {
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

    fn is_tame_owned_by(&self, owner: &dyn LivingEntity) -> bool {
        self.is_tamed() && self.owner_uuid() == Some(owner.uuid())
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("Temper", self.temper());
        nbt.insert("Tame", i8::from(self.is_tamed()));
        nbt.insert("Saddled", i8::from(self.is_saddled()));
        nbt.insert("ChestedHorse", i8::from(self.has_chest()));
        if let Some(owner) = self.owner_uuid() {
            nbt.insert("Owner", owner.to_string());
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        if let Some(temper) = nbt.int("Temper") {
            self.set_temper(temper);
        }
        if let Some(tame) = nbt.byte("Tame") {
            self.set_tamed(tame != 0);
        }
        if let Some(saddled) = nbt.byte("Saddled") {
            self.set_saddled(saddled != 0);
        }
        if let Some(chested) = nbt.byte("ChestedHorse") {
            self.set_has_chest(chested != 0);
        }
        if let Some(owner_str) = nbt.string("Owner")
            && let Ok(uuid) = Uuid::from_str(owner_str.to_str().as_ref())
        {
            self.set_owner_uuid(Some(uuid));
        }
    }
}

impl LivingEntity for DonkeyEntity {
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
        Some(&sound_events::ENTITY_DONKEY_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_DONKEY_DEATH)
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

        if !self.is_tamed() {
            let temper = self.temper();
            if temper > 0 && rand::random_range(0..100) < temper {
                self.set_tamed(true);
                self.set_owner_uuid(Some(controller.uuid()));
                self.broadcast_entity_event(EntityStatus::TamingSucceeded);
            } else {
                self.modify_temper(5);
                self.broadcast_entity_event(EntityStatus::TamingFailed);
                controller.stop_riding();
            }
        }
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

impl AgeableMob for DonkeyEntity {
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

impl Animal for DonkeyEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }
}

impl Mob for DonkeyEntity {
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
        Some(&sound_events::ENTITY_DONKEY_AMBIENT)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let stack = inventory.get_item_in_hand(hand);
            stack.copy_with_count(stack.count())
        };

        if !item_stack.is_empty() {
            if self.handle_eating_and_temper_item(player, &item_stack) {
                if !player.abilities.lock().instabuild {
                    let mut inventory = player.inventory.lock();
                    let mut current_stack = inventory.get_item_in_hand(hand).clone();
                    current_stack.shrink(1);
                    inventory.set_item_in_hand(hand, current_stack);
                }
                return InteractionResult::Success;
            }

            if LivingEntity::is_equippable_in_slot(self, &item_stack, EquipmentSlot::Saddle) {
                self.set_saddled(true);
                return LivingEntity::interact_living_entity_with_equippable(self, player, hand);
            }

            if item_stack.is(&vanilla_items::CHEST)
                && self.is_tamed()
                && !self.has_chest()
                && !AgeableMob::is_baby(self)
            {
                self.set_has_chest(true);
                self.play_sound(&sound_events::ENTITY_DONKEY_CHEST, 1.0, 1.0);
                if !player.abilities.lock().instabuild {
                    let mut inventory = player.inventory.lock();
                    let mut current_stack = inventory.get_item_in_hand(hand).clone();
                    current_stack.shrink(1);
                    inventory.set_item_in_hand(hand, current_stack);
                }
                return InteractionResult::Success;
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

impl PathfinderMob for DonkeyEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::{init_vanilla_registry, vanilla_entities};

    #[test]
    fn test_donkey_chest_saddle_and_tame_flags() {
        init_vanilla_registry();
        let donkey = DonkeyEntity::new(
            &vanilla_entities::DONKEY,
            1,
            DVec3::ZERO,
            Weak::new(),
        );

        assert!(!donkey.is_tamed());
        assert!(!donkey.is_saddled());
        assert!(!donkey.has_chest());

        donkey.set_tamed(true);
        assert!(donkey.is_tamed());

        donkey.set_saddled(true);
        assert!(donkey.is_saddled());

        donkey.set_has_chest(true);
        assert!(donkey.has_chest());
    }
}
