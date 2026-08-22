//! TNT minecart entity implementation.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_utils::axis::Axis;
use steel_utils::block_util::FoundRectangle;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, reset_forward_direction_of_relative_portal_position,
};
use crate::portal::portal_shape::PortalShape;
use crate::world::World;

/// TNT minecart entity.
#[entity_behavior(class = "MinecartTNT")]
pub struct TntMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    state: SyncMutex<TntMinecartState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `TntMinecartEntity`.
unsafe impl DowncastType for TntMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/tnt_minecart");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TntMinecartState {
    first_tick: bool,
    fuse: i32,
}

impl TntMinecartState {
    const fn new(first_tick: bool) -> Self {
        Self {
            first_tick,
            fuse: -1,
        }
    }
}

impl TntMinecartEntity {
    /// Creates a new TNT minecart entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            state: SyncMutex::new(TntMinecartState::new(true)),
        }
    }

    /// Creates a TNT minecart entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            state: SyncMutex::new(TntMinecartState::new(false)),
        }
    }

    const fn nbt_bool(value: bool) -> i8 {
        if value { 1 } else { 0 }
    }
}

impl Entity for TntMinecartEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn is_pickable(&self) -> bool {
        !self.is_removed()
    }

    fn is_pushable(&self) -> bool {
        true
    }

    fn blocks_building(&self) -> bool {
        true
    }

    fn dimension_changing_delay(&self) -> i32 {
        10
    }

    fn get_relative_portal_position(&self, axis: Axis, portal_area: FoundRectangle) -> DVec3 {
        reset_forward_direction_of_relative_portal_position(PortalShape::get_relative_position(
            portal_area,
            axis,
            self.position(),
            self.dimensions_for_pose(self.pose()),
        ))
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        nbt.insert("FlippedRotation", Self::nbt_bool(false));
        let state = self.state.lock();
        nbt.insert("HasTicked", Self::nbt_bool(state.first_tick));
        nbt.insert("TNTFuse", NbtTag::Int(state.fuse));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        if let Some(first_tick) = nbt.byte("HasTicked") {
            state.first_tick = first_tick != 0;
        }
        if let Some(fuse) = nbt.int("TNTFuse") {
            state.fuse = fuse;
        }
    }
}
