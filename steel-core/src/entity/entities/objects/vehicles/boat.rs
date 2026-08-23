//! Standard boat entity implementation.

use std::sync::Weak;

use glam::DVec3;
use steel_macros::entity_behavior;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::{RegistryExt, REGISTRY};
use steel_utils::types::InteractionHand;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::behavior::InteractionResult;
use crate::entity::{DamageSource, Entity, EntityBase, EntityBaseLoad, RemovalReason};
use crate::physics::MoverType;
use crate::player::Player;
use crate::world::World;

/// Standard rideable boat entity (2 seats).
#[entity_behavior(class = "Boat")]
pub struct BoatEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
}

// SAFETY: This key is owned by Steel and uniquely identifies `BoatEntity`.
unsafe impl DowncastType for BoatEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/boat");
}

impl BoatEntity {
    /// Creates a new boat entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        }
    }

    /// Creates a boat entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        }
    }
}

impl Entity for BoatEntity {
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

    fn can_add_passenger(&self, _passenger: &dyn Entity) -> bool {
        self.passengers().len() < 2
    }

    fn tick(&self) {
        self.base_tick();
        if self.is_removed() {
            return;
        }

        let mut vel = self.velocity();
        if self.is_in_water() {
            vel.y = (vel.y + 0.005).min(0.05);
            let drag = if self.is_vehicle() { 0.95 } else { 0.90 };
            vel.x *= drag;
            vel.z *= drag;
        } else if self.on_ground() {
            let drag = 0.6;
            vel.x *= drag;
            vel.z *= drag;
            vel.y = (vel.y - 0.04).max(-0.98);
        } else {
            vel.y = (vel.y - 0.04).max(-0.98);
            vel.x *= 0.98;
            vel.z *= 0.98;
        }

        let _ = self.move_entity(MoverType::SelfMovement, vel);
        self.set_velocity(vel);
        self.mark_velocity_sync();

        if let Some(world) = self.level() {
            let search_aabb = self.bounding_box().inflate(0.2);
            let pushable = world.get_pushable_entities(self, &search_aabb);
            for other in pushable {
                self.push_entity(other.as_ref());
            }
        }
    }

    fn push_entity(&self, entity: &dyn Entity) {
        if self.can_add_passenger(entity) && !entity.is_passenger() && entity.as_mob().is_some() {
            if let Some(world) = self.level() {
                if let Some(boat_shared) = world.get_entity_by_id(self.id()) {
                    if entity.can_ride(self) && entity.start_riding(&boat_shared) {
                        return;
                    }
                }
            }
        }

        let mut x = entity.position().x - self.position().x;
        let mut z = entity.position().z - self.position().z;
        let mut distance = x.abs().max(z.abs());
        if distance >= 0.01 {
            distance = distance.sqrt();
            x /= distance;
            z /= distance;
            let scale = (1.0 / distance).min(1.0) * 0.05;
            x *= scale;
            z *= scale;

            if !self.is_vehicle() && self.is_pushable() {
                self.push_impulse(DVec3::new(-x, 0.0, -z));
            }
            if !entity.is_vehicle() && entity.is_pushable() {
                entity.push_impulse(DVec3::new(x, 0.0, z));
            }
        }
    }

    fn hurt(&self, world: &World, source: &DamageSource, _amount: f32) -> bool {
        if self.is_removed() {
            return false;
        }

        if self.is_invulnerable() && !source.bypasses_invulnerability() {
            return false;
        }

        let is_creative_player = source
            .causing_entity_id
            .and_then(|id| world.players.get_by_entity_id(id))
            .is_some_and(|p| p.has_infinite_materials());

        self.set_removed(RemovalReason::Killed);

        if !is_creative_player {
            if let Some(item) = REGISTRY.items.by_key(&self.entity_type.key) {
                self.spawn_at_location(ItemStack::new(item), 0.0);
            }
        }

        true
    }

    fn interact(
        &self,
        player: &Player,
        _hand: InteractionHand,
        _location: DVec3,
    ) -> InteractionResult {
        if player.is_secondary_use_active() {
            return InteractionResult::Pass;
        }

        if !self.can_add_passenger(player) {
            return InteractionResult::Pass;
        }

        let Some(world) = self.level() else {
            return InteractionResult::Pass;
        };

        let Some(boat_shared) = world.get_entity_by_id(self.id()) else {
            return InteractionResult::Pass;
        };

        if player.start_riding(&boat_shared) {
            InteractionResult::Success
        } else {
            InteractionResult::Pass
        }
    }
}
