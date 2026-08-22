//! Vanilla Primed TNT entity implementation.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_blocks;
use steel_registry::vanilla_entity_data::TntEntityData;
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockStateId, DowncastType, DowncastTypeKey};

use crate::entity::synced_data::EntitySyncedData;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, RemovalReason, SharedEntity, next_entity_id,
};
use crate::physics::MoverType;
use crate::world::explosion::ExplosionBlockInteraction;
use crate::world::World;

#[entity_behavior(class = "PrimedTnt")]
/// Primed TNT entity created when TNT is ignited.
pub struct PrimedTntEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<TntEntityData>,
    fuse: SyncMutex<i32>,
    owner_id: SyncMutex<Option<i32>>,
    block_state: SyncMutex<BlockStateId>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `PrimedTntEntity`.
unsafe impl DowncastType for PrimedTntEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/tnt");
}

impl PrimedTntEntity {
    /// Creates a new Primed TNT entity with default fuse (80 ticks).
    #[must_use]
    pub fn new(
        entity_type: EntityTypeRef,
        id: i32,
        position: DVec3,
        world: Weak<World>,
    ) -> Self {
        Self::new_with_fuse(entity_type, id, position, world, 80, None)
    }

    /// Creates a new Primed TNT entity with custom fuse and owner.
    #[must_use]
    pub fn new_with_fuse(
        entity_type: EntityTypeRef,
        id: i32,
        position: DVec3,
        world: Weak<World>,
        fuse: i32,
        owner_id: Option<i32>,
    ) -> Self {
        let mut entity_data = TntEntityData::new();
        entity_data.primed_tnt_mut().fuse.set(fuse);
        entity_data
            .primed_tnt_mut()
            .block_state
            .set(vanilla_blocks::TNT.default_state());

        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(entity_data),
            fuse: SyncMutex::new(fuse),
            owner_id: SyncMutex::new(owner_id),
            block_state: SyncMutex::new(vanilla_blocks::TNT.default_state()),
        }
    }

    /// Reconstructs a Primed TNT entity from saved base state.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        let entity_data = TntEntityData::new();
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(entity_data),
            fuse: SyncMutex::new(80),
            owner_id: SyncMutex::new(None),
            block_state: SyncMutex::new(vanilla_blocks::TNT.default_state()),
        }
    }

    /// Spawns a new Primed TNT entity in the world with initial velocity.
    pub fn spawn(
        world: &Arc<World>,
        position: DVec3,
        fuse: i32,
        owner_id: Option<i32>,
    ) -> Result<SharedEntity, crate::entity::AddEntityError> {
        let tnt = Arc::new(Self::new_with_fuse(
            &steel_registry::vanilla_entities::TNT,
            next_entity_id(),
            position,
            Arc::downgrade(world),
            fuse,
            owner_id,
        ));

        let vx = (rand::random::<f64>() * 0.02 - 0.01) * 1.0;
        let vz = (rand::random::<f64>() * 0.02 - 0.01) * 1.0;
        tnt.set_velocity(DVec3::new(vx, 0.2, vz));

        world.try_add_entity(tnt.clone())?;
        Ok(tnt)
    }

    /// Returns the remaining fuse ticks.
    #[must_use]
    pub fn fuse(&self) -> i32 {
        *self.fuse.lock()
    }

    /// Sets the remaining fuse ticks.
    pub fn set_fuse(&self, fuse: i32) {
        *self.fuse.lock() = fuse;
        self.entity_data.lock().primed_tnt_mut().fuse.set(fuse);
    }

    /// Returns the owner entity ID responsible for this TNT.
    #[must_use]
    pub fn owner_id(&self) -> Option<i32> {
        *self.owner_id.lock()
    }

    /// Returns the block state rendered for this TNT entity.
    #[must_use]
    pub fn block_state(&self) -> BlockStateId {
        *self.block_state.lock()
    }
}

impl Entity for PrimedTntEntity {
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
        self.entity_base_tick();

        // Gravity and motion damping
        let mut velocity = self.velocity();
        velocity.y -= 0.04;
        self.set_velocity(velocity);

        let _ = self.move_entity(MoverType::SelfMovement, self.velocity());

        let velocity = self.velocity();
        self.set_velocity(DVec3::new(velocity.x * 0.98, velocity.y * 0.98, velocity.z * 0.98));

        // Decrement fuse
        let current_fuse = self.fuse() - 1;
        self.set_fuse(current_fuse);

        if current_fuse <= 0 {
            self.set_removed(RemovalReason::Discarded);
            if let Some(world) = self.level() {
                let center = self.position() + DVec3::new(0.0, 0.49, 0.0);
                let owner_entity = self.owner_id().and_then(|id| world.get_entity_by_id(id));
                world.explode(
                    owner_entity,
                    None,
                    center,
                    4.0,
                    false,
                    ExplosionBlockInteraction::DestroyWithDecay,
                );
            }
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Blocks
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert("Fuse", NbtTag::Short(self.fuse() as i16));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        if let Some(fuse) = nbt.short("Fuse") {
            self.set_fuse(i32::from(fuse));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::init_vanilla_registry;
    use steel_registry::vanilla_entities;

    #[test]
    fn tnt_entity_initializes_with_default_fuse() {
        init_vanilla_registry();
        let tnt = PrimedTntEntity::new(&vanilla_entities::TNT, 1, DVec3::ZERO, Weak::new());
        assert_eq!(tnt.fuse(), 80);
    }

    #[test]
    fn tnt_entity_custom_fuse_and_owner() {
        init_vanilla_registry();
        let tnt = PrimedTntEntity::new_with_fuse(&vanilla_entities::TNT, 1, DVec3::ZERO, Weak::new(), 40, Some(42));
        assert_eq!(tnt.fuse(), 40);
        assert_eq!(tnt.owner_id(), Some(42));
    }
}
