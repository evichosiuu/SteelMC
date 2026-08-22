//! Area effect cloud entity (`AreaEffectCloud`).

use std::sync::{Arc, Weak};

use glam::DVec3;
use rustc_hash::FxHashMap;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::data_components::PotionContents;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_entity_data::AreaEffectCloudEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::behavior::potion_utils::apply_potion_contents_effects;
use crate::entity::damage::DamageSource;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason, SharedEntity};
use crate::world::World;

/// Area effect cloud entity.
#[entity_behavior(class = "AreaEffectCloud")]
pub struct AreaEffectCloudEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<AreaEffectCloudEntityData>,
    potion_contents: SyncMutex<PotionContents>,
    owner_uuid: SyncMutex<Option<uuid::Uuid>>,
    owner_entity: SyncMutex<Option<Weak<dyn Entity>>>,
    duration: SyncMutex<i32>,
    wait_time: SyncMutex<i32>,
    reapplication_delay: SyncMutex<i32>,
    duration_on_use: SyncMutex<i32>,
    radius_on_use: SyncMutex<f32>,
    radius_per_tick: SyncMutex<f32>,
    victims: SyncMutex<FxHashMap<uuid::Uuid, i32>>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `AreaEffectCloudEntity`.
unsafe impl DowncastType for AreaEffectCloudEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/area_effect_cloud");
}

impl AreaEffectCloudEntity {
    /// Creates a new area effect cloud entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(AreaEffectCloudEntityData::new()),
            potion_contents: SyncMutex::new(PotionContents::empty()),
            owner_uuid: SyncMutex::new(None),
            owner_entity: SyncMutex::new(None),
            duration: SyncMutex::new(600),
            wait_time: SyncMutex::new(20),
            reapplication_delay: SyncMutex::new(20),
            duration_on_use: SyncMutex::new(0),
            radius_on_use: SyncMutex::new(0.0),
            radius_per_tick: SyncMutex::new(0.0),
            victims: SyncMutex::new(FxHashMap::default()),
        }
    }

    /// Creates an area effect cloud entity from saved base load state.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(AreaEffectCloudEntityData::new()),
            potion_contents: SyncMutex::new(PotionContents::empty()),
            owner_uuid: SyncMutex::new(None),
            owner_entity: SyncMutex::new(None),
            duration: SyncMutex::new(600),
            wait_time: SyncMutex::new(20),
            reapplication_delay: SyncMutex::new(20),
            duration_on_use: SyncMutex::new(0),
            radius_on_use: SyncMutex::new(0.0),
            radius_per_tick: SyncMutex::new(0.0),
            victims: SyncMutex::new(FxHashMap::default()),
        }
    }

    /// Sets the radius of the area effect cloud.
    pub fn set_radius(&self, radius: f32) {
        self.entity_data.lock().radius.set(radius);
    }

    /// Gets the current radius of the area effect cloud.
    pub fn radius(&self) -> f32 {
        *self.entity_data.lock().radius.get()
    }

    /// Sets the potion contents for the cloud.
    pub fn set_potion_contents(&self, contents: PotionContents) {
        *self.potion_contents.lock() = contents;
    }

    /// Sets the initial delay in ticks before applying effects.
    pub fn set_wait_time(&self, wait_time: i32) {
        *self.wait_time.lock() = wait_time;
    }

    /// Sets the total duration in ticks.
    pub fn set_duration(&self, duration: i32) {
        *self.duration.lock() = duration;
    }

    /// Gets the remaining duration in ticks.
    pub fn duration(&self) -> i32 {
        *self.duration.lock()
    }

    /// Sets the change in radius when an effect is applied to an entity.
    pub fn set_radius_on_use(&self, radius_on_use: f32) {
        *self.radius_on_use.lock() = radius_on_use;
    }

    /// Sets the change in radius per tick.
    pub fn set_radius_per_tick(&self, radius_per_tick: f32) {
        *self.radius_per_tick.lock() = radius_per_tick;
    }

    /// Sets the owner entity for attributing potion effects.
    pub fn set_owner_entity(&self, owner: Option<&SharedEntity>) {
        if let Some(owner) = owner {
            *self.owner_uuid.lock() = Some(owner.uuid());
            *self.owner_entity.lock() = Some(Arc::downgrade(owner));
        } else {
            *self.owner_uuid.lock() = None;
            *self.owner_entity.lock() = None;
        }
    }

    /// Gets the owner entity, if still resolvable.
    pub fn owner(&self) -> Option<SharedEntity> {
        if let Some(weak) = self.owner_entity.lock().as_ref() {
            if let Some(upgraded) = weak.upgrade() {
                return Some(upgraded);
            }
        }
        if let Some(uuid) = *self.owner_uuid.lock() {
            if let Some(world) = self.level() {
                if let Some(entity) = world.get_entity_by_uuid(&uuid) {
                    *self.owner_entity.lock() = Some(Arc::downgrade(&entity));
                    return Some(entity);
                }
            }
        }
        None
    }
}

impl Entity for AreaEffectCloudEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        self.entity_base_tick();

        let waiting = *self.wait_time.lock() > 0;
        let radius = self.radius();

        if waiting {
            *self.wait_time.lock() -= 1;
            return;
        }

        let Some(world) = self.level() else {
            return;
        };

        let mut duration = self.duration.lock();
        *duration -= 1;
        if *duration <= 0 {
            self.set_removed(RemovalReason::Discarded);
            return;
        }

        let radius_per_tick = *self.radius_per_tick.lock();
        if radius_per_tick != 0.0 {
            let new_radius = radius + radius_per_tick;
            if new_radius <= 0.5 {
                self.set_removed(RemovalReason::Discarded);
                return;
            }
            self.set_radius(new_radius);
        }

        if self.tick_count() % 5 == 0 {
            let reapplication_delay = *self.reapplication_delay.lock();
            let mut victims = self.victims.lock();
            victims.retain(|_, time| {
                if *time > 0 {
                    *time -= 5;
                }
                *time > 0
            });

            let bounds = self.bounding_box();
            let entities = world.get_entities_in_aabb(&bounds);
            let contents = self.potion_contents.lock().clone();
            let owner = self.owner();
            let owner_ref = owner.as_deref();
            let mut used = false;

            for target in entities {
                let Some(living) = target.as_living_entity() else {
                    continue;
                };

                if !living.is_affected_by_potions() {
                    continue;
                }

                let uuid = target.uuid();
                if victims.contains_key(&uuid) {
                    continue;
                }

                let dist_sq = target.position().distance_squared(self.position());
                if dist_sq <= f64::from(radius * radius) {
                    victims.insert(uuid, reapplication_delay);
                    apply_potion_contents_effects(&world, living, &contents, 0.25, owner_ref);
                    used = true;
                }
            }

            if used {
                let radius_on_use = *self.radius_on_use.lock();
                if radius_on_use != 0.0 {
                    let updated_radius = radius + radius_on_use;
                    if updated_radius <= 0.5 {
                        self.set_removed(RemovalReason::Discarded);
                        return;
                    }
                    self.set_radius(updated_radius);
                }

                let duration_on_use = *self.duration_on_use.lock();
                if duration_on_use != 0 {
                    *duration += duration_on_use;
                    if *duration <= 0 {
                        self.set_removed(RemovalReason::Discarded);
                        return;
                    }
                }
            }
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn attackable(&self) -> bool {
        false
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        false
    }

    fn save_additional(&self, _nbt: &mut NbtCompound) {}

    fn load_additional(&self, _nbt: BorrowedNbtCompoundView<'_, '_>) {}
}
