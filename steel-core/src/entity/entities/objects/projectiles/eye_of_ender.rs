//! Thrown Eye of Ender entity (`EyeOfEnder`).
//!
//! Spawns when a player throws an Eye of Ender. It flies toward the nearest
//! stronghold (or target block position) and after 80 ticks either drops as an
//! [`ItemEntity`] or shatters with particles.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_entity_data::EyeOfEnderEntityData;
use steel_registry::{level_events, vanilla_entities};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey};

use crate::entity::damage::DamageSource;
use crate::entity::entities::ItemEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason, next_entity_id,
};
use crate::world::World;

/// Eye of Ender entity.
#[entity_behavior(class = "EyeOfEnder")]
pub struct EyeOfEnderEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<EyeOfEnderEntityData>,
    target_pos: SyncMutex<DVec3>,
    life: SyncMutex<i32>,
    survive_after_death: SyncMutex<bool>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EyeOfEnderEntity`.
unsafe impl DowncastType for EyeOfEnderEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/eye_of_ender");
}

impl EyeOfEnderEntity {
    /// Creates a new Eye of Ender entity with default item.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(EyeOfEnderEntityData::new()),
            target_pos: SyncMutex::new(position),
            life: SyncMutex::new(0),
            survive_after_death: SyncMutex::new(true),
        }
    }

    /// Creates an Eye of Ender entity from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(EyeOfEnderEntityData::new()),
            target_pos: SyncMutex::new(DVec3::ZERO),
            life: SyncMutex::new(0),
            survive_after_death: SyncMutex::new(true),
        }
    }

    /// Sets the rendered item stack.
    pub fn set_item(&self, item: ItemStack) {
        self.entity_data.lock().eye_of_ender_mut().item_stack.set(item);
    }

    /// Gets the rendered item stack.
    #[must_use]
    pub fn get_item(&self) -> ItemStack {
        self.entity_data
            .lock()
            .eye_of_ender()
            .item_stack
            .get()
            .clone()
    }

    /// Signals the Eye of Ender towards the given target block position.
    pub fn signal_to(&self, target: BlockPos) {
        let pos = self.position();
        let dx = f64::from(target.x()) - pos.x;
        let dz = f64::from(target.z()) - pos.z;
        let h_dist = (dx * dx + dz * dz).sqrt();

        let (tx, ty, tz) = if h_dist > 12.0 {
            (
                pos.x + dx / h_dist * 12.0,
                pos.y + 8.0,
                pos.z + dz / h_dist * 12.0,
            )
        } else {
            (
                f64::from(target.x()),
                f64::from(target.y()),
                f64::from(target.z()),
            )
        };

        *self.target_pos.lock() = DVec3::new(tx, ty, tz);
        *self.life.lock() = 0;
        *self.survive_after_death.lock() = rand::random::<f32>() < 0.8;
    }

    /// Explicitly sets whether this entity drops an item when it expires.
    pub fn set_survive_after_death(&self, survive: bool) {
        *self.survive_after_death.lock() = survive;
    }

    /// Returns whether this entity will drop an item when it expires.
    #[must_use]
    pub fn survive_after_death(&self) -> bool {
        *self.survive_after_death.lock()
    }

    /// Gets the current target position.
    #[must_use]
    pub fn target_position(&self) -> DVec3 {
        *self.target_pos.lock()
    }
}

impl Entity for EyeOfEnderEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        self.default_tick();

        self.set_old_position_to_current();

        let pos = self.position();
        let target = *self.target_pos.lock();
        let delta = target - pos;
        let dist = delta.length();

        let mut vel = self.velocity();

        if dist > 0.001 {
            let dir = delta / dist;
            vel.x += (dir.x * 0.1 - vel.x) * 0.1;
            vel.y += (dir.y * 0.1 - vel.y) * 0.1;
            vel.z += (dir.z * 0.1 - vel.z) * 0.1;
        } else {
            vel *= 0.95;
        }

        self.set_velocity(vel);
        let new_pos = pos + vel;
        let _ = self.try_set_position(new_pos);

        let horizontal_speed = (vel.x * vel.x + vel.z * vel.z).sqrt();
        if horizontal_speed > 0.001 {
            let yaw = vel.z.atan2(vel.x).to_degrees() as f32 - 90.0;
            let pitch = (-vel.y.atan2(horizontal_speed)).to_degrees() as f32;
            self.set_rotation((yaw, pitch));
        }

        let mut life = self.life.lock();
        *life += 1;

        if *life >= 80 {
            if let Some(world) = self.level() {
                if *self.survive_after_death.lock() {
                    let item_entity = Arc::new(ItemEntity::with_item(
                        &vanilla_entities::ITEM,
                        next_entity_id(),
                        self.position(),
                        self.get_item(),
                        Arc::downgrade(&world),
                    ));
                    if let Err(err) = world.try_add_entity(item_entity) {
                        log::debug!("failed to spawn dropped ender eye item: {err}");
                    }
                } else {
                    world.level_event(
                        level_events::PARTICLES_EYE_OF_ENDER_DEATH,
                        self.block_position(),
                        0,
                        None,
                    );
                }
            }
            self.set_removed(RemovalReason::Discarded);
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

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
    use steel_utils::BlockPos;

    use crate::world::World;

    use super::EyeOfEnderEntity;

    #[test]
    fn default_item_is_ender_eye() {
        init_vanilla_registry();

        let eye = EyeOfEnderEntity::new(
            &vanilla_entities::EYE_OF_ENDER,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        assert_eq!(eye.get_item().item().key, vanilla_items::ENDER_EYE.key);
    }

    #[test]
    fn signal_to_calculates_target_positions() {
        init_vanilla_registry();

        let eye = EyeOfEnderEntity::new(
            &vanilla_entities::EYE_OF_ENDER,
            1,
            DVec3::new(0.0, 64.0, 0.0),
            Weak::<World>::new(),
        );

        eye.signal_to(BlockPos::new(100, 64, 0));
        let target = eye.target_position();
        assert!((target.x - 12.0).abs() < 1e-4);
        assert!((target.y - 72.0).abs() < 1e-4);
        assert!((target.z - 0.0).abs() < 1e-4);
    }
}
