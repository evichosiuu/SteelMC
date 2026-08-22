//! Dragon fireball projectile entity (`DragonFireball`).

use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::vanilla_entity_data::DragonFireballEntityData;
use steel_registry::{sound_events, vanilla_damage_types};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileHit, RemovalReason,
};
use crate::world::World;

/// Dragon fireball projectile.
#[entity_behavior(class = "DragonFireball")]
pub struct DragonFireballEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    entity_data: SyncMutex<DragonFireballEntityData>,
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies `DragonFireballEntity`.
unsafe impl DowncastType for DragonFireballEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/dragon_fireball");
}

impl DragonFireballEntity {
    /// Creates a new dragon fireball projectile.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(DragonFireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a dragon fireball from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(DragonFireballEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    fn explode(&self, world: &World) {
        let pos = self.position();
        world.play_sound_at(
            &sound_events::ENTITY_DRAGON_FIREBALL_EXPLODE,
            SoundSource::Hostile,
            pos,
            1.0,
            1.0,
            None,
        );

        let radius = 3.0;
        let radius_sq = radius * radius;
        let search_box = self.bounding_box().inflate(radius);
        let damage_source = DamageSource::environment(&vanilla_damage_types::DRAGON_BREATH)
            .with_direct_entity(self.id());

        for target in world.get_entities_in_aabb_matching(&search_box, Entity::is_living_entity) {
            if pos.distance_squared(target.position()) <= radius_sq {
                target.hurt(world, &damage_source, 12.0);
            }
        }

        self.set_removed(RemovalReason::Discarded);
    }
}

impl Entity for DragonFireballEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        let hit = self.get_hit_result_on_move_vector();
        let movement = self.velocity();
        let _ = self.try_set_position(self.position() + movement);

        if hit.is_some() {
            if let Some(world) = self.level() {
                self.explode(&world);
            }
        }
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Hostile
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn hurt(&self, _world: &World, _source: &DamageSource, _amount: f32) -> bool {
        false
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
    }
}

impl Projectile for DragonFireballEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        if let Some(world) = self.level() {
            self.explode(&world);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::vanilla_entities;

    #[test]
    fn dragon_fireball_creation() {
        let fireball = DragonFireballEntity::new(
            &vanilla_entities::DRAGON_FIREBALL,
            1,
            DVec3::ZERO,
            Weak::new(),
        );
        assert_eq!(fireball.sound_source(), SoundSource::Hostile);
    }
}
