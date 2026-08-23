//! Furnace minecart entity implementation.

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
use steel_utils::types::InteractionHand;

use crate::behavior::InteractionResult;
use crate::entity::{
    DamageSource, Entity, EntityBase, EntityBaseLoad,
    reset_forward_direction_of_relative_portal_position,
};
use crate::player::Player;
use crate::portal::portal_shape::PortalShape;
use crate::world::World;

/// Furnace minecart entity.
#[entity_behavior(class = "MinecartFurnace")]
pub struct FurnaceMinecartEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    state: SyncMutex<FurnaceMinecartState>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `FurnaceMinecartEntity`.
unsafe impl DowncastType for FurnaceMinecartEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/furnace_minecart");
}

#[derive(Debug, Clone, PartialEq)]
struct FurnaceMinecartState {
    first_tick: bool,
    push_x: f64,
    push_z: f64,
    fuel: i16,
}

impl FurnaceMinecartState {
    const fn new(first_tick: bool) -> Self {
        Self {
            first_tick,
            push_x: 0.0,
            push_z: 0.0,
            fuel: 0,
        }
    }
}

impl FurnaceMinecartEntity {
    /// Creates a new furnace minecart entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            state: SyncMutex::new(FurnaceMinecartState::new(true)),
        }
    }

    /// Creates a furnace minecart entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            state: SyncMutex::new(FurnaceMinecartState::new(false)),
        }
    }

    const fn nbt_bool(value: bool) -> i8 {
        if value { 1 } else { 0 }
    }
}

impl Entity for FurnaceMinecartEntity {
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
        let state = self.state.lock();
        let (mut push_x, mut push_z) = (state.push_x, state.push_z);
        let mut fuel = state.fuel;
        drop(state);

        AbstractMinecart::tick_minecart(
            self,
            Some(&mut fuel),
            Some((&mut push_x, &mut push_z)),
            None,
        );

        let mut state = self.state.lock();
        state.fuel = fuel;
        state.push_x = push_x;
        state.push_z = push_z;
    }

    fn hurt(&self, world: &World, source: &DamageSource, _amount: f32) -> bool {
        AbstractMinecart::hurt_minecart(self, world, source, &vanilla_items::FURNACE_MINECART)
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
        nbt.insert("PushX", NbtTag::Double(state.push_x));
        nbt.insert("PushZ", NbtTag::Double(state.push_z));
        nbt.insert("Fuel", NbtTag::Short(state.fuel));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        let mut state = self.state.lock();
        if let Some(first_tick) = nbt.byte("HasTicked") {
            state.first_tick = first_tick != 0;
        }
        if let Some(push_x) = nbt.double("PushX").or_else(|| nbt.double("pushX")) {
            state.push_x = push_x;
        }
        if let Some(push_z) = nbt.double("PushZ").or_else(|| nbt.double("pushZ")) {
            state.push_z = push_z;
        }
        if let Some(fuel) = nbt
            .short("Fuel")
            .or_else(|| nbt.short("fuel"))
            .or_else(|| nbt.int("Fuel").map(|f| f as i16))
        {
            state.fuel = fuel;
        }
    }

    fn interact(
        &self,
        player: &Player,
        hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        let item_stack = {
            let inventory = player.inventory.lock();
            let item = inventory.get_item_in_hand(hand);
            item.copy_with_count(item.count())
        };

        if item_stack.is(&vanilla_items::COAL) || item_stack.is(&vanilla_items::CHARCOAL) {
            let mut state = self.state.lock();
            let new_fuel = (state.fuel + 3600).min(32000);
            if state.fuel != new_fuel {
                state.fuel = new_fuel;
                let push_vec = self.position() - player.position();
                let (push_x, push_z) = if push_vec.x * push_vec.x + push_vec.z * push_vec.z > 0.001 {
                    (push_vec.x, push_vec.z)
                } else {
                    let look_dir = player.look_angle();
                    (look_dir.x, look_dir.z)
                };
                let len = (push_x * push_x + push_z * push_z).sqrt();
                if len > 0.0001 {
                    state.push_x = push_x / len;
                    state.push_z = push_z / len;
                }
                if !player.has_infinite_materials() {
                    player.inventory.lock().shrink_item_in_hand(hand, 1);
                }
                return InteractionResult::Success;
            }
        }

        InteractionResult::Pass
    }
}
