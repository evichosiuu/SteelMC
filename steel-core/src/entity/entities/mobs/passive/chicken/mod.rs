//! Vanilla Chicken entity with variant + sound-variant parity and AI goals.

use std::str::FromStr;
use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::chicken_sound_variant::ChickenSoundVariantRef;
use steel_registry::chicken_variant::ChickenVariantRef;
use steel_registry::entity_type::{
    EntityAttachmentPoint, EntityAttachments, EntityDimensions, EntityTypeRef,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::ChickenEntityData;
use steel_registry::vanilla_item_tags::ItemTag;
use steel_registry::{
    REGISTRY, RegistryExt, RegistryReference, TaggedRegistryExt, sound_events, vanilla_attributes,
    vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, Identifier};

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    BreedGoal, FloatGoal, FollowParentGoal, LookAtPlayerGoal, PanicGoal, RandomLookAroundGoal,
    TemptGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, Animal, AnimalBase, Entity, EntityBase, EntityBaseLoad, EntityPose,
    EntitySpawnReason, EntitySyncedData, LivingEntity, LivingEntityBase, Mob, MobBase,
    PathfinderMob, SpawnGroupData,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

const CHICKEN_BABY_PASSENGER_ATTACHMENTS: [EntityAttachmentPoint; 1] =
    [EntityAttachmentPoint::new(0.0, 0.4, 0.0)];
const CHICKEN_BABY_WIDTH: f32 = 0.2;
const CHICKEN_BABY_HEIGHT: f32 = 0.35;
const CHICKEN_BABY_EYE_HEIGHT: f32 = 0.32;

const CHICKEN_BABY_DIMENSIONS: EntityDimensions = EntityDimensions::new_with_attachments(
    CHICKEN_BABY_WIDTH,
    CHICKEN_BABY_HEIGHT,
    CHICKEN_BABY_EYE_HEIGHT,
    EntityAttachments::new(&CHICKEN_BABY_PASSENGER_ATTACHMENTS, &[], &[], &[]),
);
const DEFAULT_STEP_HEIGHT: f32 = 0.6;

#[entity_behavior(class = "Chicken")]
/// Vanilla chicken entity with slow-falling, egg-laying, synced variant, and AI goals.
pub struct ChickenEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    animal_base: AnimalBase,
    entity_data: SyncMutex<ChickenEntityData>,
    egg_time: SyncMutex<i32>,
    is_chicken_jockey: SyncMutex<bool>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ChickenEntity`.
unsafe impl DowncastType for ChickenEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/chicken");
}

impl ChickenEntity {
    /// Creates a new chicken at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs a chicken from persisted base entity state.
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
        let mut entity_data = ChickenEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(1, PanicGoal::new(1.4));
            goal_selector.add_goal(2, BreedGoal::new(1.0));
            goal_selector.add_goal(
                3,
                TemptGoal::new(
                    1.0,
                    |item_stack| ChickenEntity::is_food(item_stack),
                    false,
                ),
            );
            goal_selector.add_goal(4, FollowParentGoal::new(1.1));
            goal_selector.add_goal(5, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(6, LookAtPlayerGoal::new(6.0));
            goal_selector.add_goal(7, RandomLookAroundGoal::new());
        }

        let next_egg_time = rand::random_range(6000..12000);

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            animal_base,
            entity_data: SyncMutex::new(entity_data),
            egg_time: SyncMutex::new(next_egg_time),
            is_chicken_jockey: SyncMutex::new(false),
        }
    }

    /// Sets the active chicken variant by registry entry.
    pub fn set_variant(&self, variant: ChickenVariantRef) {
        self.entity_data
            .lock()
            .variant
            .set(RegistryReference::new(variant));
    }

    /// Returns the active chicken variant.
    #[must_use]
    pub fn variant(&self) -> ChickenVariantRef {
        self.entity_data.lock().variant.get().value()
    }

    /// Sets the active chicken sound variant by registry entry.
    pub fn set_sound_variant(&self, sound_variant: ChickenSoundVariantRef) {
        self.entity_data
            .lock()
            .sound_variant
            .set(RegistryReference::new(sound_variant));
    }

    /// Returns the active chicken sound variant.
    #[must_use]
    pub fn sound_variant(&self) -> ChickenSoundVariantRef {
        self.entity_data.lock().sound_variant.get().value()
    }

    /// Returns whether this chicken is a jockey mount.
    #[must_use]
    pub fn is_chicken_jockey(&self) -> bool {
        *self.is_chicken_jockey.lock()
    }

    /// Sets whether this chicken is a jockey mount.
    pub fn set_chicken_jockey(&self, jockey: bool) {
        *self.is_chicken_jockey.lock() = jockey;
    }

    fn set_variant_by_key(&self, key: &Identifier) -> bool {
        let Some(variant) = REGISTRY.chicken_variants.by_key(key) else {
            return false;
        };
        self.set_variant(variant);
        true
    }

    fn set_sound_variant_by_key(&self, key: &Identifier) {
        if let Some(sound_variant) = REGISTRY.chicken_sound_variants.by_key(key) {
            self.set_sound_variant(sound_variant);
        }
    }

    fn update_dirty_mob_effect_entity_data(&self) {
        if !self.living_base.take_effects_dirty() {
            return;
        }

        let display = self.living_base.mob_effect_display_state();

        {
            let mut entity_data = self.entity_data.lock();
            let living = entity_data.living_entity_mut();
            living.effect_particles.set(display.particles);
            living.effect_ambience.set(display.ambient);
        }

        self.entity_data.set_base_invisible_flag(display.invisible);
        self.entity_data
            .set_base_glowing_flag(self.has_glowing_tag() || display.glowing);
    }

    /// Returns whether an item stack matches the vanilla chicken food tag.
    #[must_use]
    pub fn is_food(item_stack: &ItemStack) -> bool {
        REGISTRY
            .items
            .is_in_tag(item_stack.item(), &ItemTag::CHICKEN_FOOD)
    }

    fn handle_egg_laying(&self) {
        if AgeableMob::is_baby(self) || self.is_chicken_jockey() {
            return;
        }

        let Some(world) = self.level() else {
            return;
        };

        let mut egg_time = self.egg_time.lock();
        *egg_time -= 1;
        if *egg_time <= 0 {
            world.play_sound_at(
                &sound_events::ENTITY_CHICKEN_EGG,
                SoundSource::Neutral,
                self.position(),
                1.0,
                (rand::random::<f32>() - rand::random::<f32>()) * 0.2 + 1.0,
                None,
            );

            let _ = world.spawn_item(self.position(), ItemStack::new(&vanilla_items::EGG));
            *egg_time = rand::random_range(6000..12000);
        }
    }

    fn handle_slow_falling(&self) {
        let mut vel = self.velocity();
        if !self.on_ground() && vel.y < 0.0 {
            vel.y *= 0.6;
            self.set_velocity(vel);
            self.reset_fall_distance();
        }
    }
}

impl Entity for ChickenEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn dimensions_for_pose(&self, _pose: EntityPose) -> EntityDimensions {
        let scale = LivingEntity::get_scale(self);
        if AgeableMob::is_baby(self) {
            CHICKEN_BABY_DIMENSIONS.scale(scale)
        } else if self.entity_type.fixed {
            self.entity_type.dimensions
        } else {
            self.entity_type.dimensions.scale(scale)
        }
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn update_data_before_sync(&self) {
        self.update_dirty_mob_effect_entity_data();
    }

    fn max_up_step(&self) -> f32 {
        self.attributes()
            .lock()
            .get_value(vanilla_attributes::STEP_HEIGHT)
            .unwrap_or(f64::from(DEFAULT_STEP_HEIGHT)) as f32
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn play_step_sound(&self, _pos: BlockPos, _block_state: BlockStateId) {
        let sound_var = self.sound_variant();
        let sounds = if AgeableMob::is_baby(self) {
            &sound_var.baby_sounds
        } else {
            &sound_var.adult_sounds
        };
        self.play_sound(sounds.step_sound, 0.15, 1.0);
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);
        self.save_animal(nbt);
        nbt.insert("EggLayTime", *self.egg_time.lock());
        nbt.insert("IsChickenJockey", self.is_chicken_jockey());
        nbt.insert("variant", self.variant().key.to_string());
        nbt.insert("sound_variant", self.sound_variant().key.to_string());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);
        self.load_animal(nbt);

        if let Some(egg_time) = nbt.int("EggLayTime") {
            *self.egg_time.lock() = egg_time;
        }
        self.set_chicken_jockey(nbt.byte("IsChickenJockey").is_some_and(|b| b != 0));

        if let Some(variant) = nbt.string("variant")
            && let Ok(key) = Identifier::from_str(variant.to_str().as_ref())
        {
            self.set_variant_by_key(&key);
        }
        if let Some(sound_variant) = nbt.string("sound_variant")
            && let Ok(key) = Identifier::from_str(sound_variant.to_str().as_ref())
        {
            self.set_sound_variant_by_key(&key);
        }
    }
}

impl LivingEntity for ChickenEntity {
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
        let sound_var = self.sound_variant();
        let sounds = if AgeableMob::is_baby(self) {
            &sound_var.baby_sounds
        } else {
            &sound_var.adult_sounds
        };
        Some(sounds.hurt_sound)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        let sound_var = self.sound_variant();
        let sounds = if AgeableMob::is_baby(self) {
            &sound_var.baby_sounds
        } else {
            &sound_var.adult_sounds
        };
        Some(sounds.death_sound)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();

        AgeableMob::tick_ageable_mob(self);
        Animal::tick_animal_love(self);
        self.handle_slow_falling();
        self.handle_egg_laying();

        result
    }
}

impl AgeableMob for ChickenEntity {
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

impl Animal for ChickenEntity {
    fn animal_base(&self) -> &AnimalBase {
        &self.animal_base
    }

    fn is_food(&self, item_stack: &ItemStack) -> bool {
        ChickenEntity::is_food(item_stack)
    }

    fn breed_variant_key(&self) -> Option<&Identifier> {
        Some(&self.variant().key)
    }

    fn set_breed_variant_key(&self, key: &Identifier) -> bool {
        self.set_variant_by_key(key)
    }

    fn initialize_breed_offspring(&self, partner: &dyn Animal, offspring: &dyn Animal) {
        let use_self_variant = rand::random::<bool>();
        let variant_key = if use_self_variant {
            self.breed_variant_key()
        } else {
            partner.breed_variant_key()
        };
        let Some(variant_key) = variant_key else {
            return;
        };

        if !offspring.set_breed_variant_key(variant_key) {
            log::error!("chicken offspring could not inherit breeding variant {variant_key}");
        }
    }
}

impl Mob for ChickenEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn custom_server_ai_step(&self) {
        Animal::custom_server_ai_step_animal(self);
    }

    fn ambient_sound(&self) -> Option<SoundEventRef> {
        let sound_var = self.sound_variant();
        let sounds = if AgeableMob::is_baby(self) {
            &sound_var.baby_sounds
        } else {
            &sound_var.adult_sounds
        };
        Some(sounds.ambient_sound)
    }

    fn finalize_spawn(
        &self,
        world: &Arc<World>,
        spawn_reason: EntitySpawnReason,
        group_data: Option<SpawnGroupData>,
    ) -> Option<SpawnGroupData> {
        let biome = world.biome_at(self.block_position());
        let (variant, sound_variant) = {
            let mut random = LegacyRandom::from_seed(rand::random());
            let variant = biome.and_then(|biome| {
                REGISTRY
                    .chicken_variants
                    .select_spawn_variant(biome, &mut random)
            });
            let sound_variant = REGISTRY.chicken_sound_variants.pick_random(&mut random);
            (variant, sound_variant)
        };

        if let Some(variant) = variant {
            self.set_variant(variant);
        }

        if let Some(sound_variant) = sound_variant {
            self.set_sound_variant(sound_variant);
        }

        self.finalize_spawn_ageable_mob(world, spawn_reason, group_data)
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        Animal::mob_interact_animal(self, player, hand)
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for ChickenEntity {}

#[cfg(test)]
mod tests;
