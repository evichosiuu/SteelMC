//! Hopper minecart entity implementation.

use std::str::FromStr;
use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_utils::Identifier;
use steel_utils::axis::Axis;
use steel_utils::block_util::FoundRectangle;
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, reset_forward_direction_of_relative_portal_position,
};
use crate::portal::portal_shape::PortalShape;
use crate::world::World;

/// Hopper minecart entity.
#[entity_behavior(class = "MinecartHopper")]
pub struct HopperMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    state: SyncMutex<HopperMinecartState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `HopperMinecartEntity`.
unsafe impl DowncastType for HopperMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/hopper_minecart");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HopperMinecartState {
    first_tick: bool,
    enabled: bool,
    transfer_cooldown: i32,
    loot_table: Option<Identifier>,
    loot_table_seed: i64,
}

impl HopperMinecartState {
    const fn new(first_tick: bool) -> Self {
        Self {
            first_tick,
            enabled: true,
            transfer_cooldown: 0,
            loot_table: None,
            loot_table_seed: 0,
        }
    }
}

impl HopperMinecartEntity {
    /// Creates a new hopper minecart entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            state: SyncMutex::new(HopperMinecartState::new(true)),
        }
    }

    /// Creates a hopper minecart entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            state: SyncMutex::new(HopperMinecartState::new(false)),
        }
    }

    /// Sets whether this hopper minecart is enabled.
    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.state.lock();
        state.enabled = enabled;
    }

    /// Returns whether this hopper minecart is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.state.lock().enabled
    }

    const fn nbt_bool(value: bool) -> i8 {
        if value { 1 } else { 0 }
    }
}

impl Entity for HopperMinecartEntity {
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
        nbt.insert("Enabled", Self::nbt_bool(state.enabled));
        nbt.insert("TransferCooldown", NbtTag::Int(state.transfer_cooldown));

        if let Some(loot_table) = state.loot_table.as_ref() {
            nbt.insert("LootTable", loot_table.to_string());
            if state.loot_table_seed != 0 {
                nbt.insert("LootTableSeed", NbtTag::Long(state.loot_table_seed));
            }
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let loot_table = nbt
            .string("LootTable")
            .and_then(|value| Identifier::from_str(&value.to_string()).ok());
        let mut state = self.state.lock();
        if let Some(first_tick) = nbt.byte("HasTicked") {
            state.first_tick = first_tick != 0;
        }
        if let Some(enabled) = nbt.byte("Enabled") {
            state.enabled = enabled != 0;
        }
        if let Some(cooldown) = nbt.int("TransferCooldown") {
            state.transfer_cooldown = cooldown;
        }
        state.loot_table = loot_table;
        state.loot_table_seed = nbt.long("LootTableSeed").unwrap_or(0);
    }
}
