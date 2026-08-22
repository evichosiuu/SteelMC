//! Vanilla EnderMan entity with AI goals, teleportation, staring provocation, and Endermite targeting.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::equipment::EquipmentSlot;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::EnderManEntityData;
use steel_registry::{
    sound_events, vanilla_attributes, vanilla_damage_type_tags, vanilla_damage_types,
    vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, BlockStateId, Downcast, DowncastType, DowncastTypeKey};

use crate::entity::ai::goal::{
    FloatGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::entities::EndermiteEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, LivingEntity, LivingEntityBase, Mob,
    MobBase, PathfinderMob,
};
use crate::inventory::equipment::EntityEquipment;
use crate::physics::MoveResult;
use crate::world::World;

const DEFAULT_STEP_HEIGHT: f32 = 1.0;

#[entity_behavior(class = "EnderMan")]
/// Vanilla Enderman entity that teleports, becomes aggressive when stared at, and targets players/Endermites.
pub struct EnderManEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<EnderManEntityData>,
    health: SyncMutex<f32>,
    mob_flags: SyncMutex<i8>,
    stare_sound_timer: SyncMutex<i32>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EnderManEntity`.
unsafe impl DowncastType for EnderManEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/enderman");
}

impl EnderManEntity {
    /// Creates a new Enderman at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs an Enderman from persisted base entity state.
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
        let mut entity_data = EnderManEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(1, MeleeAttackGoal::new(1.0, false));
            goal_selector.add_goal(7, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(8, LookAtPlayerGoal::new(8.0));
            goal_selector.add_goal(8, RandomLookAroundGoal::new());

            let mut target_selector = mob_base.target_selector().lock();
            target_selector.add_goal(1, HurtByTargetGoal::new());
            target_selector.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |_target, _world| true),
            );
            target_selector.add_goal(
                3,
                NearestAttackableTargetGoal::new(true, |target, _| {
                    target
                        .downcast_ref::<EndermiteEntity>()
                        .is_some_and(|endermite| endermite.is_player_spawned())
                }),
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
            health: SyncMutex::new(max_health),
            mob_flags: SyncMutex::new(0),
            stare_sound_timer: SyncMutex::new(0),
        }
    }

    /// Returns the carried block state if the Enderman is holding a block.
    #[must_use]
    pub fn carried_block(&self) -> Option<BlockStateId> {
        self.entity_data.lock().carry_state.get().clone()
    }

    /// Sets the carried block state.
    pub fn set_carried_block(&self, block_state: Option<BlockStateId>) {
        self.entity_data.lock().carry_state.set(block_state);
    }

    /// Returns whether this Enderman is in creepy / screaming state.
    #[must_use]
    pub fn is_creepy(&self) -> bool {
        *self.entity_data.lock().creepy.get()
    }

    /// Sets whether this Enderman is in creepy / screaming state.
    pub fn set_creepy(&self, creepy: bool) {
        self.entity_data.lock().creepy.set(creepy);
    }

    /// Returns whether this Enderman is marked as stared at.
    #[must_use]
    pub fn is_stared_at(&self) -> bool {
        *self.entity_data.lock().stared_at.get()
    }

    /// Sets whether this Enderman is marked as stared at.
    pub fn set_stared_at(&self, stared_at: bool) {
        self.entity_data.lock().stared_at.set(stared_at);
    }

    /// Attempts to teleport the Enderman to a random position within 64x16x64 blocks.
    pub fn teleport_randomly(&self) -> bool {
        let Some(world) = self.level() else {
            return false;
        };

        let pos = self.position();

        for _ in 0..16 {
            let dx = (rand::random::<f64>() - 0.5) * 64.0;
            let dy = (rand::random_range(-16..16)) as f64;
            let dz = (rand::random::<f64>() - 0.5) * 64.0;

            let target_pos = DVec3::new(pos.x + dx, pos.y + dy, pos.z + dz);

            if self.try_teleport_to(target_pos, &world) {
                return true;
            }
        }

        false
    }

    fn try_teleport_to(&self, target_pos: DVec3, world: &Arc<World>) -> bool {
        let old_pos = self.position();

        if let Ok(()) = self.try_set_position(target_pos) {
            world.play_sound_at(
                &sound_events::ENTITY_ENDERMAN_TELEPORT,
                SoundSource::Hostile,
                old_pos,
                1.0,
                1.0,
                None,
            );
            world.play_sound_at(
                &sound_events::ENTITY_ENDERMAN_TELEPORT,
                SoundSource::Hostile,
                target_pos,
                1.0,
                1.0,
                None,
            );
            return true;
        }

        false
    }

    fn check_player_stare(&self, world: &Arc<World>) {
        if self.target().is_some() {
            return;
        }

        let enderman_eye = DVec3::new(self.position().x, self.get_eye_y(), self.position().z);
        let enderman_uuid = self.uuid();

        let nearest_player = world.nearest_player(enderman_eye, 64.0, |player| {
            if player.uuid() == enderman_uuid {
                return false;
            }

            // Players wearing carved pumpkins on head are exempt from provoking endermen by gaze
            let inventory = player.inventory.lock();
            let head_item = inventory.get_ref(EquipmentSlot::Head);
            if head_item.is(&vanilla_items::CARVED_PUMPKIN) {
                return false;
            }

            let player_eye = DVec3::new(player.position().x, player.get_eye_y(), player.position().z);
            let dir_to_enderman = enderman_eye - player_eye;
            let dist = dir_to_enderman.length();
            if dist <= 0.001 {
                return false;
            }
            let dir_normalized = dir_to_enderman / dist;

            let look = player.calculate_view_vector(player.rotation().1, player.rotation().0);

            // Stare dot product threshold: angle must align closely with enderman's head
            look.dot(dir_normalized) > 1.0 - 0.025 / dist
        });

        if let Some(player) = nearest_player {
            self.set_creepy(true);
            self.set_stared_at(true);

            let mut sound_timer = self.stare_sound_timer.lock();
            if *sound_timer <= 0 {
                world.play_sound_at(
                    &sound_events::ENTITY_ENDERMAN_STARE,
                    SoundSource::Hostile,
                    self.position(),
                    2.0,
                    1.0,
                    None,
                );
                *sound_timer = 20;
            }

            let _ = self.set_target(Some(&(player as Arc<dyn Entity>)));
        }
    }

    fn update_stare_sound_timer(&self) {
        let mut timer = self.stare_sound_timer.lock();
        if *timer > 0 {
            *timer -= 1;
        }
    }
}

impl Entity for EnderManEntity {
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
        self.play_sound(&sound_events::ENTITY_ENDERMAN_AMBIENT, 0.15, 1.0);
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn hurt(&self, world: &World, source: &DamageSource, amount: f32) -> bool {
        // Endermen are immune to projectile damage and teleport away when targeted by projectiles
        if source.direct_entity_id.is_some() && source.direct_entity_id != source.causing_entity_id
            || source.is(&vanilla_damage_type_tags::DamageTypeTag::IS_PROJECTILE)
        {
            self.teleport_randomly();
            return false;
        }

        let hurt_result = self.hurt_server(world, source, amount);

        // 50% chance to teleport on non-projectile damage
        if hurt_result && rand::random::<bool>() {
            self.teleport_randomly();
        }

        hurt_result
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("creepy", self.is_creepy());
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.set_creepy(nbt.byte("creepy").is_some_and(|b| b != 0));
    }
}

impl LivingEntity for EnderManEntity {
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
        Some(&sound_events::ENTITY_ENDERMAN_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_ENDERMAN_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        self.update_stare_sound_timer();

        if let Some(world) = self.level() {
            self.check_player_stare(&world);

            if self.is_in_water() {
                let water_damage = DamageSource::environment(&vanilla_damage_types::DROWN);
                self.hurt(&world, &water_damage, 1.0);
                self.teleport_randomly();
            }
        }

        self.default_ai_step()
    }
}

impl Mob for EnderManEntity {
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
        if self.is_creepy() {
            Some(&sound_events::ENTITY_ENDERMAN_SCREAM)
        } else {
            Some(&sound_events::ENTITY_ENDERMAN_AMBIENT)
        }
    }

    fn mob_flags(&self) -> i8 {
        *self.mob_flags.lock()
    }

    fn set_mob_flags(&self, flags: i8) {
        *self.mob_flags.lock() = flags;
    }
}

impl PathfinderMob for EnderManEntity {}
