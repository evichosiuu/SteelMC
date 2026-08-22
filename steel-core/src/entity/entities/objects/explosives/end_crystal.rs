//! End Crystal entity implementation.

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::{NbtCompound, NbtTag};
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_entity_data::EndCrystalEntityData;
use steel_registry::{sound_events, vanilla_blocks, vanilla_damage_types};
use steel_utils::{BlockPos, locks::SyncMutex, types::UpdateFlags};
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::damage::DamageSource;
use crate::entity::{Entity, EntityBase, EntityBaseLoad, EntitySyncedData, RemovalReason};
use crate::world::World;

/// End Crystal entity state used by worldgen, combat, and persistence.
#[entity_behavior(class = "EndCrystal")]
pub struct EndCrystalEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<EndCrystalEntityData>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EndCrystalEntity`.
unsafe impl DowncastType for EndCrystalEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/end_crystal");
}

impl EndCrystalEntity {
    /// Creates a new End Crystal entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(EndCrystalEntityData::new()),
        }
    }

    /// Creates an End Crystal entity from saved data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(EndCrystalEntityData::new()),
        }
    }

    /// Sets the optional beam target.
    pub fn set_beam_target(&self, target: Option<BlockPos>) {
        self.entity_data.lock().beam_target.set(target);
    }

    /// Returns the optional beam target.
    #[must_use]
    pub fn beam_target(&self) -> Option<BlockPos> {
        *self.entity_data.lock().beam_target.get()
    }

    /// Sets whether the crystal renders its bedrock base.
    pub fn set_show_bottom(&self, show_bottom: bool) {
        self.entity_data.lock().show_bottom.set(show_bottom);
    }

    /// Returns whether the crystal renders its bedrock base.
    #[must_use]
    pub fn shows_bottom(&self) -> bool {
        *self.entity_data.lock().show_bottom.get()
    }

    /// Sets position and rotation, matching vanilla `Entity.snapTo`.
    ///
    /// # Panics
    ///
    /// Panics if the active world entity manager rejects the snap position. This is an invariant
    /// failure for loaded end crystals.
    pub fn snap_to(&self, position: DVec3, yaw: f32, pitch: f32) {
        if let Err(error) = self.base.try_set_position(position) {
            panic!(
                "failed to commit end crystal {} snap position: {error}",
                self.base.id()
            );
        }
        self.base.set_rotation((yaw, pitch));
        self.set_old_position_to_current();
    }

    const fn nbt_bool(value: bool) -> i8 {
        if value { 1 } else { 0 }
    }
}

impl Entity for EndCrystalEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        if let Some(world) = self.level() {
            let pos = self.position();
            let block_pos = BlockPos::containing(pos.x, pos.y, pos.z);
            if self.shows_bottom()
                && world
                    .get_block_state(block_pos)
                    .is_air()
            {
                let _ = world.set_block(block_pos, vanilla_blocks::FIRE.default_state(), UpdateFlags::UPDATE_ALL);
            }
        }
    }

    fn is_pickable(&self) -> bool {
        true
    }

    fn blocks_building(&self) -> bool {
        true
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn hurt(&self, world: &World, source: &DamageSource, _amount: f32) -> bool {
        if self.is_removed() || (self.is_invulnerable() && !source.bypasses_invulnerability()) {
            return false;
        }

        self.set_removed(RemovalReason::Killed);

        let pos = self.position();
        world.play_sound_at(
            &sound_events::ENTITY_GENERIC_EXPLODE,
            SoundSource::Blocks,
            pos,
            4.0,
            (1.0 + (rand::random::<f32>() - rand::random::<f32>()) * 0.2) * 0.7,
            None,
        );

        let radius = 6.0;
        let radius_sq = radius * radius;
        let search_box = self.bounding_box().inflate(radius);
        let explosion_source = DamageSource::environment(&vanilla_damage_types::EXPLOSION)
            .with_direct_entity(self.id());
        for target in world.get_entities_in_aabb_matching(&search_box, Entity::is_living_entity) {
            let dist_sq = pos.distance_squared(target.position());
            if dist_sq <= radius_sq {
                let dist = dist_sq.sqrt();
                let scale = ((radius - dist) / radius).max(0.0);
                let damage = scale as f32 * 12.0;
                target.hurt(world, &explosion_source, damage);
            }
        }

        world.on_end_crystal_destroyed(self, source);

        true
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        if let Some(target) = self.beam_target() {
            nbt.insert(
                "beam_target",
                NbtTag::IntArray(vec![target.x(), target.y(), target.z()]),
            );
        }

        nbt.insert("ShowBottom", Self::nbt_bool(self.shows_bottom()));
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        if let Some(target) = nbt.int_array("beam_target")
            && target.len() == 3
        {
            self.set_beam_target(Some(BlockPos::new(target[0], target[1], target[2])));
        }

        if let Some(show_bottom) = nbt.byte("ShowBottom") {
            self.set_show_bottom(show_bottom != 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use steel_registry::vanilla_entities;
    use crate::test_support::fresh_test_world;

    #[test]
    fn end_crystal_does_not_duplicate_shared_invulnerable_state() {
        let crystal = EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            1,
            DVec3::new(1.5, 2.5, 3.5),
            Weak::new(),
        );
        crystal.set_invulnerable(true);

        let mut nbt = NbtCompound::new();
        crystal.save_additional(&mut nbt);

        assert_eq!(nbt.byte("Invulnerable"), None);
    }

    #[test]
    fn end_crystal_is_pickable_like_vanilla() {
        let crystal = EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            1,
            DVec3::new(1.5, 2.5, 3.5),
            Weak::new(),
        );

        assert!(crystal.is_pickable());
    }

    #[test]
    fn end_crystal_blocks_building_like_vanilla() {
        let crystal =
            EndCrystalEntity::new(&vanilla_entities::END_CRYSTAL, 1, DVec3::ZERO, Weak::new());

        assert!(crystal.blocks_building());
    }

    #[test]
    fn end_crystal_hurt_destroys_crystal_and_returns_true() {
        let world = fresh_test_world("crystal_hurt_test");
        let crystal = EndCrystalEntity::new(
            &vanilla_entities::END_CRYSTAL,
            1,
            DVec3::new(0.0, 70.0, 0.0),
            Arc::downgrade(&world),
        );
        let source = DamageSource::environment(&vanilla_damage_types::GENERIC);

        assert!(crystal.hurt(&world, &source, 1.0));
        assert!(crystal.is_removed());
    }
}
