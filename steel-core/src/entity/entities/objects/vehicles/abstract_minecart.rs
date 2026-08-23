//! Common abstract minecart movement, ticking, rail physics, and damage handling.

use glam::DVec3;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, RailShape};
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_blocks;
use steel_utils::{BlockPos, BlockStateId};

use crate::entity::{DamageSource, Entity, RemovalReason};
use crate::physics::MoverType;
use crate::world::explosion::ExplosionBlockInteraction;
use crate::world::World;

/// Helper functions for minecart entities.
pub struct AbstractMinecart;

impl AbstractMinecart {
    /// Returns the rail position and block state if the entity is currently on a rail.
    #[must_use]
    pub fn get_rail_pos_and_state(entity: &dyn Entity) -> Option<(BlockPos, BlockStateId)> {
        let world = entity.level()?;
        let pos = entity.position();
        let block_pos = BlockPos::containing(pos.x, pos.y, pos.z);
        let state = world.get_block_state(block_pos);

        if state.get_block().has_tag(&BlockTag::RAILS) {
            return Some((block_pos, state));
        }

        let block_pos_below = block_pos.below();
        let state_below = world.get_block_state(block_pos_below);
        if state_below.get_block().has_tag(&BlockTag::RAILS) {
            return Some((block_pos_below, state_below));
        }

        None
    }

    /// Returns whether the minecart is currently on rails.
    #[must_use]
    pub fn is_on_rails(entity: &dyn Entity) -> bool {
        Self::get_rail_pos_and_state(entity).is_some()
    }

    /// Performs a single game tick of movement and physics for a minecart entity.
    pub fn tick_minecart(
        entity: &dyn Entity,
        furnace_fuel: Option<&mut i16>,
        furnace_push: Option<(&mut f64, &mut f64)>,
        tnt_fuse: Option<&mut i32>,
    ) {
        entity.base_tick();
        if entity.is_removed() {
            return;
        }

        // Handle TNT fuse ticking if present
        if let Some(fuse) = tnt_fuse {
            if *fuse >= 0 {
                *fuse -= 1;
                if *fuse == 0 {
                    if let Some(world) = entity.level() {
                        let pos = entity.position();
                        world.explode(
                            None,
                            None,
                            pos,
                            4.0,
                            false,
                            ExplosionBlockInteraction::Destroy,
                        );
                    }
                    entity.set_removed(RemovalReason::Killed);
                    return;
                }
            }
        }

        let rail_info = Self::get_rail_pos_and_state(entity);
        let mut vel = entity.velocity();

        if let Some((rail_pos, rail_state)) = rail_info {
            let shape = rail_state
                .try_get_value(&BlockStateProperties::RAIL_SHAPE)
                .or_else(|| rail_state.try_get_value(&BlockStateProperties::RAIL_SHAPE_STRAIGHT))
                .unwrap_or(RailShape::NorthSouth);

            // Handle sloped rail gravity
            match shape {
                RailShape::AscendingEast => vel.x -= 0.0078125,
                RailShape::AscendingWest => vel.x += 0.0078125,
                RailShape::AscendingNorth => vel.z += 0.0078125,
                RailShape::AscendingSouth => vel.z -= 0.0078125,
                _ => {}
            }

            // Handle powered rail / activator rail / furnace push
            let block = rail_state.get_block();
            if block == &vanilla_blocks::POWERED_RAIL {
                let is_powered = rail_state
                    .try_get_value(&BlockStateProperties::POWERED)
                    .unwrap_or(false);
                if is_powered {
                    let speed = (vel.x * vel.x + vel.z * vel.z).sqrt();
                    if speed > 0.01 {
                        vel.x += (vel.x / speed) * 0.06;
                        vel.z += (vel.z / speed) * 0.06;
                    } else {
                        if matches!(
                            shape,
                            RailShape::EastWest
                                | RailShape::AscendingEast
                                | RailShape::AscendingWest
                        ) {
                            vel.x += 0.06;
                        } else {
                            vel.z += 0.06;
                        }
                    }
                } else {
                    if (vel.x * vel.x + vel.z * vel.z).sqrt() < 0.03 {
                        vel.x = 0.0;
                        vel.z = 0.0;
                    } else {
                        vel.x *= 0.5;
                        vel.z *= 0.5;
                    }
                }
            } else if block == &vanilla_blocks::ACTIVATOR_RAIL {
                let is_powered = rail_state
                    .try_get_value(&BlockStateProperties::POWERED)
                    .unwrap_or(false);
                if is_powered {
                    for passenger in entity.passengers() {
                        passenger.stop_riding();
                    }
                }
            }

            // Furnace fuel pushing force
            if let (Some(fuel), Some((push_x, push_z))) = (furnace_fuel, furnace_push) {
                if *fuel > 0 {
                    *fuel -= 1;
                    if *push_x * *push_x + *push_z * *push_z > 0.0001 {
                        vel.x += *push_x * 0.1;
                        vel.z += *push_z * 0.1;
                    }
                } else {
                    *push_x = 0.0;
                    *push_z = 0.0;
                }
            }

            // Apply rail drag
            let drag = if entity.is_vehicle() { 0.98 } else { 0.96 };
            vel.x *= drag;
            vel.z *= drag;

            // Cap max speed
            let max_speed = 0.4;
            let speed = (vel.x * vel.x + vel.z * vel.z).sqrt();
            if speed > max_speed {
                vel.x = (vel.x / speed) * max_speed;
                vel.z = (vel.z / speed) * max_speed;
            }

            // Project velocity and position onto track segment
            let (off1, off2) = get_rail_offsets(shape);
            let center = DVec3::new(
                f64::from(rail_pos.x()) + 0.5,
                f64::from(rail_pos.y()) + 0.5,
                f64::from(rail_pos.z()) + 0.5,
            );
            let pt_a = center + off1;
            let pt_b = center + off2;
            let dir = pt_b - pt_a;
            let len = (dir.x * dir.x + dir.z * dir.z).sqrt();

            let (start_pt, u_x, u_z, v_proj) = if len > 0.0 {
                let u_x = dir.x / len;
                let u_z = dir.z / len;
                let v_proj = vel.x * u_x + vel.z * u_z;
                if v_proj < 0.0 {
                    (pt_b, -u_x, -u_z, -v_proj)
                } else {
                    (pt_a, u_x, u_z, v_proj)
                }
            } else {
                (pt_a, 0.0, 0.0, 0.0)
            };

            let rel_pos = entity.position() - start_pt;
            let dist_along_track = (rel_pos.x * u_x + rel_pos.z * u_z).clamp(0.0, len);
            let new_dist = (dist_along_track + v_proj).clamp(0.0, len);

            let target_x = start_pt.x + u_x * new_dist;
            let target_z = start_pt.z + u_z * new_dist;

            // Calculate Y elevation
            let mut target_y = f64::from(rail_pos.y()) + 0.0625;
            match shape {
                RailShape::AscendingEast => {
                    target_y += (target_x - f64::from(rail_pos.x())).clamp(0.0, 1.0);
                }
                RailShape::AscendingWest => {
                    target_y += 1.0 - (target_x - f64::from(rail_pos.x())).clamp(0.0, 1.0);
                }
                RailShape::AscendingSouth => {
                    target_y += (target_z - f64::from(rail_pos.z())).clamp(0.0, 1.0);
                }
                RailShape::AscendingNorth => {
                    target_y += 1.0 - (target_z - f64::from(rail_pos.z())).clamp(0.0, 1.0);
                }
                _ => {}
            }

            vel.x = v_proj * u_x;
            vel.z = v_proj * u_z;

            let new_pos = DVec3::new(target_x, target_y, target_z);
            let _ = entity.try_set_position(new_pos);
            entity.set_velocity(DVec3::new(vel.x, 0.0, vel.z));
            entity.mark_velocity_sync();
        } else {
            // Off rail physics
            vel.y = (vel.y - 0.04).max(-0.98);
            let drag = if entity.on_ground() { 0.5 } else { 0.95 };
            vel.x *= drag;
            vel.z *= drag;
            vel.y *= 0.98;

            let _ = entity.move_entity(MoverType::SelfMovement, vel);
            entity.set_velocity(vel);
            entity.mark_velocity_sync();
        }

        // Check nearby pushable entities / player push
        if let Some(world) = entity.level() {
            let search_aabb = entity.bounding_box().inflate(0.2);
            let pushable = world.get_pushable_entities(entity, &search_aabb);
            for other in pushable {
                entity.push_entity(other.as_ref());
            }
        }
    }

    /// Handles damage and destruction for minecart entities.
    pub fn hurt_minecart(
        entity: &dyn Entity,
        world: &World,
        source: &DamageSource,
        item_drop: ItemRef,
    ) -> bool {
        if entity.is_removed() {
            return false;
        }

        if entity.is_invulnerable() && !source.bypasses_invulnerability() {
            return false;
        }

        let is_creative_player = source
            .causing_entity_id
            .and_then(|id| world.players.get_by_entity_id(id))
            .is_some_and(|p| p.has_infinite_materials());

        entity.set_removed(RemovalReason::Killed);

        if !is_creative_player {
            entity.spawn_at_location(ItemStack::new(item_drop), 0.0);
        }

        true
    }
}

fn get_rail_offsets(shape: RailShape) -> (DVec3, DVec3) {
    match shape {
        RailShape::NorthSouth => (DVec3::new(0.0, 0.0, -0.5), DVec3::new(0.0, 0.0, 0.5)),
        RailShape::EastWest => (DVec3::new(-0.5, 0.0, 0.0), DVec3::new(0.5, 0.0, 0.0)),
        RailShape::AscendingEast => (DVec3::new(-0.5, -0.5, 0.0), DVec3::new(0.5, 0.0, 0.0)),
        RailShape::AscendingWest => (DVec3::new(-0.5, 0.0, 0.0), DVec3::new(0.5, -0.5, 0.0)),
        RailShape::AscendingNorth => (DVec3::new(0.0, 0.0, -0.5), DVec3::new(0.0, -0.5, 0.5)),
        RailShape::AscendingSouth => (DVec3::new(0.0, -0.5, -0.5), DVec3::new(0.0, 0.0, 0.5)),
        RailShape::SouthEast => (DVec3::new(0.0, 0.0, 0.5), DVec3::new(0.5, 0.0, 0.0)),
        RailShape::SouthWest => (DVec3::new(-0.5, 0.0, 0.0), DVec3::new(0.0, 0.0, 0.5)),
        RailShape::NorthWest => (DVec3::new(0.0, 0.0, -0.5), DVec3::new(-0.5, 0.0, 0.0)),
        RailShape::NorthEast => (DVec3::new(0.5, 0.0, 0.0), DVec3::new(0.0, 0.0, -0.5)),
    }
}
