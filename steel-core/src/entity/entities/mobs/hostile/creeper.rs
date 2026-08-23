//! Vanilla Creeper entity implementation with AI goals, swell behavior, and charging.

use std::sync::{LazyLock, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::items::Item;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::CreeperEntityData;
use steel_registry::{sound_events, vanilla_attributes, vanilla_entities, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::CreeperSwellGoal;
use crate::entity::ai::goal::{
    AvoidEntityGoal, FloatGoal, HurtByTargetGoal, LookAtPlayerGoal, MeleeAttackGoal,
    NearestAttackableTargetGoal, RandomLookAroundGoal, WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::synced_data::EntitySyncedData;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, LivingEntity, LivingEntityBase, Mob, MobBase,
    PathfinderMob, RemovalReason,
};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::explosion::ExplosionBlockInteraction;
use crate::world::World;

const DEFAULT_STEP_HEIGHT: f32 = 0.6;

fn random_music_disc() -> &'static Item {
    let discs: &[&'static LazyLock<Item>] = &[
        &vanilla_items::MUSIC_DISC_13,
        &vanilla_items::MUSIC_DISC_CAT,
        &vanilla_items::MUSIC_DISC_BLOCKS,
        &vanilla_items::MUSIC_DISC_CHIRP,
        &vanilla_items::MUSIC_DISC_FAR,
        &vanilla_items::MUSIC_DISC_MALL,
        &vanilla_items::MUSIC_DISC_MELLOHI,
        &vanilla_items::MUSIC_DISC_STAL,
        &vanilla_items::MUSIC_DISC_STRAD,
        &vanilla_items::MUSIC_DISC_WARD,
        &vanilla_items::MUSIC_DISC_11,
        &vanilla_items::MUSIC_DISC_WAIT,
        &vanilla_items::MUSIC_DISC_OTHERSIDE,
        &vanilla_items::MUSIC_DISC_PIGSTEP,
        &vanilla_items::MUSIC_DISC_RELIC,
        &vanilla_items::MUSIC_DISC_CREATOR,
        &vanilla_items::MUSIC_DISC_CREATOR_MUSIC_BOX,
        &vanilla_items::MUSIC_DISC_PRECIPICE,
    ];
    let index = rand::random_range(0..discs.len());
    &**discs[index]
}

#[entity_behavior(class = "Creeper")]
/// Vanilla Creeper mob that swells and explodes when near targets.
pub struct CreeperEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    entity_data: SyncMutex<CreeperEntityData>,
    health: SyncMutex<f32>,
    mob_flags: SyncMutex<i8>,
    fuse: SyncMutex<i32>,
    max_fuse: SyncMutex<i32>,
    explosion_radius: SyncMutex<u8>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CreeperEntity`.
unsafe impl DowncastType for CreeperEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/creeper");
}

impl CreeperEntity {
    /// Creates a new creeper at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs a creeper from persisted base entity state.
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
            goal_selector.add_goal(1, FloatGoal::new(&mob_base));
            goal_selector.add_goal(2, CreeperSwellGoal::new());
            goal_selector.add_goal(
                3,
                AvoidEntityGoal::with_selector(
                    6.0,
                    1.0,
                    1.2,
                    |target, _| {
                        let type_ref = target.entity_type();
                        type_ref == &vanilla_entities::CAT || type_ref == &vanilla_entities::OCELOT
                    },
                ),
            );
            goal_selector.add_goal(4, MeleeAttackGoal::new(1.0, false));
            goal_selector.add_goal(5, WaterAvoidingRandomStrollGoal::new(0.8));
            goal_selector.add_goal(6, LookAtPlayerGoal::new(8.0));
            goal_selector.add_goal(6, RandomLookAroundGoal::new());

            let mut target_selector = mob_base.target_selector().lock();
            target_selector.add_goal(1, HurtByTargetGoal::new());
            target_selector.add_goal(
                2,
                NearestAttackableTargetGoal::new_for_players(true, |target, _world| {
                    target.can_be_seen_as_enemy()
                }),
            );
        }

        let max_health = living_base
            .attributes()
            .lock()
            .required_value(vanilla_attributes::MAX_HEALTH) as f32;

        let entity_data = CreeperEntityData::new();

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            entity_data: SyncMutex::new(entity_data),
            health: SyncMutex::new(max_health),
            mob_flags: SyncMutex::new(0),
            fuse: SyncMutex::new(0),
            max_fuse: SyncMutex::new(30),
            explosion_radius: SyncMutex::new(3),
        }
    }

    /// Returns whether this creeper is charged by lightning.
    #[must_use]
    pub fn is_powered(&self) -> bool {
        *self.entity_data.lock().creeper().is_powered.get()
    }

    /// Sets whether this creeper is charged by lightning.
    pub fn set_powered(&self, powered: bool) {
        self.entity_data.lock().creeper_mut().is_powered.set(powered);
    }

    /// Returns whether this creeper is ignited by flint and steel or fire charge.
    #[must_use]
    pub fn is_ignited(&self) -> bool {
        *self.entity_data.lock().creeper().is_ignited.get()
    }

    /// Sets whether this creeper is ignited.
    pub fn set_ignited(&self, ignited: bool) {
        self.entity_data.lock().creeper_mut().is_ignited.set(ignited);
    }

    /// Returns the swell direction (1 for swelling, -1 for relaxing).
    #[must_use]
    pub fn swell_dir(&self) -> i32 {
        *self.entity_data.lock().creeper().swell_dir.get()
    }

    /// Sets the swell direction.
    pub fn set_swell_dir(&self, dir: i32) {
        self.entity_data.lock().creeper_mut().swell_dir.set(dir);
    }

    /// Returns the current swell fuse count.
    #[must_use]
    pub fn fuse(&self) -> i32 {
        *self.fuse.lock()
    }

    /// Sets the current swell fuse count.
    pub fn set_fuse(&self, fuse: i32) {
        *self.fuse.lock() = fuse;
    }

    /// Returns the maximum fuse threshold before explosion.
    #[must_use]
    pub fn max_fuse(&self) -> i32 {
        *self.max_fuse.lock()
    }

    /// Returns the explosion radius.
    #[must_use]
    pub fn explosion_radius(&self) -> u8 {
        *self.explosion_radius.lock()
    }

    /// Triggers the creeper explosion.
    pub fn explode_creeper(&self) {
        let radius = if self.is_powered() {
            f32::from(self.explosion_radius()) * 2.0
        } else {
            f32::from(self.explosion_radius())
        };

        self.set_removed(RemovalReason::Killed);

        if let Some(world) = self.level() {
            let self_shared = world.get_entity_by_id(self.id());
            world.explode(
                self_shared,
                None,
                self.position(),
                radius,
                false,
                ExplosionBlockInteraction::DestroyWithDecay,
            );
        }
    }
}

impl Entity for CreeperEntity {
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

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        nbt.insert("powered", NbtTag::Byte(i8::from(self.is_powered())));
        nbt.insert("Fuse", NbtTag::Short(self.max_fuse() as i16));
        nbt.insert(
            "ExplosionRadius",
            NbtTag::Byte(self.explosion_radius() as i8),
        );
        nbt.insert("ignited", NbtTag::Byte(i8::from(self.is_ignited())));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        if let Some(powered) = nbt.byte("powered") {
            self.set_powered(powered != 0);
        }
        if let Some(fuse) = nbt.short("Fuse") {
            *self.max_fuse.lock() = i32::from(fuse);
        }
        if let Some(radius) = nbt.byte("ExplosionRadius") {
            *self.explosion_radius.lock() = radius as u8;
        }
        if let Some(ignited) = nbt.byte("ignited") {
            self.set_ignited(ignited != 0);
        }
    }
}

impl LivingEntity for CreeperEntity {
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
        Some(&sound_events::ENTITY_CREEPER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_CREEPER_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        if Entity::is_alive(self) {
            if self.is_ignited() {
                self.set_swell_dir(1);
            }

            let swell = self.swell_dir();
            if swell > 0 {
                if self.fuse() == 0 {
                    self.play_sound(&sound_events::ENTITY_CREEPER_PRIMED, 1.0, 0.5);
                }

                let new_fuse = self.fuse() + 1;
                self.set_fuse(new_fuse);

                if new_fuse >= self.max_fuse() {
                    self.explode_creeper();
                }
            } else {
                let new_fuse = (self.fuse() - 1).max(0);
                self.set_fuse(new_fuse);
            }
        }

        self.default_ai_step()
    }

    fn drop_custom_death_loot(&self, source: &DamageSource, _killed_by_player: bool) {
        let Some(world) = self.level() else {
            return;
        };

        let Some(causing_id) = source.causing_entity_id else {
            return;
        };

        let Some(killer) = world.get_entity_by_id(causing_id) else {
            return;
        };

        if killer.entity_type() == &vanilla_entities::SKELETON {
            let disc = random_music_disc();
            self.spawn_at_location(steel_registry::item_stack::ItemStack::new(disc), 0.0);
        }
    }
}

impl Mob for CreeperEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn mob_flags(&self) -> i8 {
        *self.mob_flags.lock()
    }

    fn set_mob_flags(&self, flags: i8) {
        *self.mob_flags.lock() = flags;
    }

    fn mob_interact(&self, player: &Player, hand: InteractionHand) -> InteractionResult {
        let item_is_igniter = player.inventory.lock().get_item_in_hand(hand).is(&vanilla_items::FLINT_AND_STEEL)
            || player.inventory.lock().get_item_in_hand(hand).is(&vanilla_items::FIRE_CHARGE);

        if item_is_igniter {
            self.play_sound(&sound_events::ENTITY_TNT_PRIMED, 1.0, 0.5);
            self.set_ignited(true);
            self.set_swell_dir(1);

            if !player.has_infinite_materials() {
                let is_flint = player.inventory.lock().get_item_in_hand(hand).is(&vanilla_items::FLINT_AND_STEEL);
                if is_flint {
                    player.inventory.lock().mutate_item_in_hand(hand, |item| {
                        item.hurt_and_break(1, false);
                    });
                } else {
                    player.inventory.lock().mutate_item_in_hand(hand, |item| {
                        item.shrink(1);
                    });
                }
            }

            return InteractionResult::Success;
        }

        InteractionResult::Pass
    }
}

impl PathfinderMob for CreeperEntity {}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::init_vanilla_registry;

    #[test]
    fn creeper_initial_state_and_swell() {
        init_vanilla_registry();
        let creeper = CreeperEntity::new(&vanilla_entities::CREEPER, 1, DVec3::ZERO, Weak::new());

        assert!(!creeper.is_powered());
        assert!(!creeper.is_ignited());
        assert_eq!(creeper.swell_dir(), -1);
        assert_eq!(creeper.fuse(), 0);
        assert_eq!(creeper.max_fuse(), 30);
        assert_eq!(creeper.explosion_radius(), 3);

        creeper.set_powered(true);
        assert!(creeper.is_powered());

        creeper.set_ignited(true);
        assert!(creeper.is_ignited());

        creeper.set_swell_dir(1);
        assert_eq!(creeper.swell_dir(), 1);
    }
}
