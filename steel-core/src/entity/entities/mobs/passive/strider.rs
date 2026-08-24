//! Vanilla Strider entity implementation.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::StriderEntityData;
use steel_registry::{sound_events, vanilla_attributes, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    TemptGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad,
    EntitySyncedData, ItemBasedSteering, ItemSteerable, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob, SharedEntity,
};
use crate::inventory::equipment::EquipmentSlot;
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

/// Vanilla strider entity.
#[entity_behavior(class = "Strider")]
/// Vanilla Strider entity.
pub struct StriderEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    steering: SyncMutex<ItemBasedSteering>,
    saddled: SyncMutex<bool>,
    entity_data: SyncMutex<StriderEntityData>,
}

unsafe impl DowncastType for StriderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/strider");
}

impl StriderEntity {
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
        let steering = SyncMutex::new(ItemBasedSteering::new());

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(1, PanicGoal::new(1.25));
            goal_selector.add_goal(3, BreedGoal::new(1.0));
            goal_selector.add_goal(
                4,
                TemptGoal::new(
                    1.2,
                    |item_stack| item_stack.is(&vanilla_items::WARPED_FUNGUS_ON_A_STICK),
                    false,
                ),
            );
            goal_selector.add_goal(
                4,
                TemptGoal::new(
                    1.2,
                    |item_stack| item_stack.is(&vanilla_items::WARPED_FUNGUS),
                    false,
                ),
            );
            goal_selector.add_goal(5, FollowParentGoal::new(1.1));
            goal_selector.add_goal(6, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(7, LookAtPlayerGoal::new(6.0));
            goal_selector.add_goal(8, RandomLookAroundGoal::new());
        }

        let mut entity_data = StriderEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            steering,
            saddled: SyncMutex::new(false),
            entity_data: SyncMutex::new(entity_data),
        }
    }

    /// Returns whether the strider is saddled.
    #[must_use]
    pub fn is_saddled(&self) -> bool {
        *self.saddled.lock()
    }

    /// Sets whether the strider is saddled.
    pub fn set_saddled(&self, saddled: bool) {
        *self.saddled.lock() = saddled;
    }

    /// Returns whether the strider is suffocating/cold (not in lava).
    #[must_use]
    pub fn is_shivering(&self) -> bool {
        *self.entity_data.lock().suffocating.get()
    }

    /// Sets whether the strider is shivering.
    pub fn set_shivering(&self, shivering: bool) {
        self.entity_data.lock().suffocating.set(shivering);
    }

    fn set_ridden_rotation(&self, controller_yaw: f32, controller_pitch: f32) {
        self.set_rotation((controller_yaw, controller_pitch * 0.5));
        self.base.set_old_yaw_to_current();
        let yaw = self.rotation().0;
        self.set_y_body_rot(yaw);
        self.set_y_head_rot(yaw);
    }
}

impl Entity for StriderEntity {
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
        self.play_sound(&sound_events::ENTITY_STRIDER_STEP, 0.15, 1.0);
    }

    fn controlling_passenger(&self) -> Option<SharedEntity> {
        if self.is_saddled()
            && let Some(passenger) = self.first_passenger()
            && passenger.as_player().is_some_and(|player| {
                let mut is_holding_stick =
                    |stack: &ItemStack| stack.is(&vanilla_items::WARPED_FUNGUS_ON_A_STICK);
                player.is_holding(&mut is_holding_stick)
            })
        {
            return Some(passenger);
        }
        self.controlling_passenger_mob()
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("Saddled", i8::from(self.is_saddled()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);
        if let Some(saddled) = nbt.byte("Saddled") {
            self.set_saddled(saddled != 0);
        }
    }
}

impl LivingEntity for StriderEntity {
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
        Some(&sound_events::ENTITY_STRIDER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_STRIDER_DEATH)
    }

    fn can_use_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Saddle && Entity::is_alive(self) && !AgeableMob::is_baby(self)
    }

    fn can_dispenser_equip_into_slot(&self, slot: EquipmentSlot) -> bool {
        slot == EquipmentSlot::Saddle
    }

    fn equip_sound(&self, slot: EquipmentSlot, _stack: &ItemStack) -> Option<SoundEventRef> {
        (slot == EquipmentSlot::Saddle).then_some(&sound_events::ENTITY_STRIDER_SADDLE)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn tick_ridden(&self, controller: &Player, _ridden_input: DVec3) {
        let (yaw, pitch) = controller.rotation();
        self.set_ridden_rotation(yaw, pitch);
        ItemSteerable::tick_boost(self);
    }

    fn ridden_input(&self, _controller: &Player, _self_input: DVec3) -> DVec3 {
        DVec3::new(0.0, 0.0, 1.0)
    }

    fn ridden_speed(&self, _controller: &Player) -> f32 {
        let movement_speed = self
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MOVEMENT_SPEED) as f32;
        movement_speed * 0.225 * ItemSteerable::boost_factor(self)
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        result
    }
}

impl AgeableMob for StriderEntity {
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

impl Animal for StriderEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        item_stack.is(&vanilla_items::WARPED_FUNGUS)
    }

    fn play_eating_sound(&self) {
        self.play_sound(&sound_events::ENTITY_STRIDER_EAT, 1.0, 1.0);
    }
}

impl ItemSteerable for StriderEntity {
    fn item_based_steering(&self) -> &SyncMutex<ItemBasedSteering> {
        &self.steering
    }

    fn boost_time_total(&self) -> i32 {
        *self.entity_data.lock().boost_time.get()
    }

    fn set_boost_time_total(&self, boost_time_total: i32) {
        self.entity_data.lock().boost_time.set(boost_time_total);
    }
}

impl Mob for StriderEntity {
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
        Some(&sound_events::ENTITY_STRIDER_AMBIENT)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let stack = inventory.get_item_in_hand(hand);
            stack.copy_with_count(stack.count())
        };

        if item_stack.is(&vanilla_items::WARPED_FUNGUS_ON_A_STICK)
            && self.is_saddled()
            && self.is_vehicle()
        {
            if ItemSteerable::boost(self) {
                return InteractionResult::Success;
            }
            return InteractionResult::Pass;
        }

        if !self.is_saddled() && self.is_food(&item_stack) {
            let interaction_result = Animal::mob_interact_animal(self, player, hand);
            if interaction_result.consumes_action() {
                return interaction_result;
            }
        }

        if !self.is_saddled()
            && LivingEntity::is_equippable_in_slot(self, &item_stack, EquipmentSlot::Saddle)
        {
            self.set_saddled(true);
            return LivingEntity::interact_living_entity_with_equippable(self, player, hand);
        }

        if self.is_saddled() && !self.is_vehicle() && !player.is_secondary_use_active() {
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

impl PathfinderMob for StriderEntity {}
