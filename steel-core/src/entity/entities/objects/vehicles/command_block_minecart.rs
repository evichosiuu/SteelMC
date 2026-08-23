//! Command block minecart entity implementation.

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

use super::abstract_minecart::AbstractMinecart;
use steel_registry::vanilla_items;
use crate::entity::{
    DamageSource, Entity, EntityBase, EntityBaseLoad,
    reset_forward_direction_of_relative_portal_position,
};
use crate::portal::portal_shape::PortalShape;
use crate::world::World;

/// Command block minecart entity.
#[entity_behavior(class = "MinecartCommandBlock")]
pub struct CommandBlockMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    state: SyncMutex<CommandBlockMinecartState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CommandBlockMinecartEntity`.
unsafe impl DowncastType for CommandBlockMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/command_block_minecart");
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandBlockMinecartState {
    first_tick: bool,
    command: String,
    success_count: i32,
    last_output: String,
    track_output: bool,
}

impl CommandBlockMinecartState {
    const fn new(first_tick: bool) -> Self {
        Self {
            first_tick,
            command: String::new(),
            success_count: 0,
            last_output: String::new(),
            track_output: true,
        }
    }
}

impl CommandBlockMinecartEntity {
    /// Creates a new command block minecart entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            state: SyncMutex::new(CommandBlockMinecartState::new(true)),
        }
    }

    /// Creates a command block minecart entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            state: SyncMutex::new(CommandBlockMinecartState::new(false)),
        }
    }

    /// Returns the command stored in this minecart.
    #[must_use]
    pub fn command(&self) -> String {
        self.state.lock().command.clone()
    }

    /// Sets the command stored in this minecart.
    pub fn set_command(&self, command: impl Into<String>) {
        let mut state = self.state.lock();
        state.command = command.into();
    }

    /// Returns whether this command block tracks output.
    #[must_use]
    pub fn track_output(&self) -> bool {
        self.state.lock().track_output
    }

    /// Sets whether this command block tracks output.
    pub fn set_track_output(&self, track_output: bool) {
        let mut state = self.state.lock();
        state.track_output = track_output;
    }

    const fn nbt_bool(value: bool) -> i8 {
        if value { 1 } else { 0 }
    }
}

impl Entity for CommandBlockMinecartEntity {
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

    fn is_on_rails(&self) -> bool {
        AbstractMinecart::is_on_rails(self)
    }

    fn tick(&self) {
        AbstractMinecart::tick_minecart(self, None, None, None);
    }

    fn hurt(&self, world: &World, source: &DamageSource, _amount: f32) -> bool {
        AbstractMinecart::hurt_minecart(self, world, source, &vanilla_items::COMMAND_BLOCK_MINECART)
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
        nbt.insert("Command", state.command.clone());
        nbt.insert("SuccessCount", NbtTag::Int(state.success_count));
        nbt.insert("LastOutput", state.last_output.clone());
        nbt.insert("TrackOutput", Self::nbt_bool(state.track_output));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        if let Some(first_tick) = nbt.byte("HasTicked") {
            state.first_tick = first_tick != 0;
        }
        if let Some(cmd) = nbt.string("Command") {
            state.command = cmd.to_string();
        }
        if let Some(count) = nbt.int("SuccessCount") {
            state.success_count = count;
        }
        if let Some(output) = nbt.string("LastOutput") {
            state.last_output = output.to_string();
        }
        if let Some(track) = nbt.byte("TrackOutput") {
            state.track_output = track != 0;
        }
    }
}
