//! Thrown egg entity (`ThrownEgg`).
//!
//! Mirrors vanilla `ThrownEgg` on the Steel
//! `Projectile → ThrowableProjectile → ThrowableItemProjectile` trait stack.
//! On impact, deals 0 damage with type `vanilla_damage_types::THROWN`, and has a
//! 1 in 8 (12.5%) chance to spawn a baby chicken. If successful, there is a further
//! 1 in 32 chance to spawn 4 baby chickens instead of 1.

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::vanilla_entity_data::EggEntityData;
use steel_registry::{vanilla_damage_types, vanilla_entities, vanilla_items};
use steel_utils::locks::SyncMutex;
use steel_utils::{DowncastType, DowncastTypeKey};

use crate::entity::damage::DamageSource;
use crate::entity::{
    ENTITIES, Entity, EntityBase, EntityBaseLoad, EntitySpawnReason, EntitySyncedData, Projectile,
    ProjectileBase, ProjectileHit, RemovalReason, SharedEntity, ThrowableItemProjectile,
    ThrowableProjectile, next_entity_id,
};
use crate::world::World;

/// A thrown egg projectile.
#[entity_behavior(class = "ThrownEgg")]
pub struct EggEntity {
    /// Common entity fields (id, uuid, position, etc.).
    base: EntityBase,
    /// Vanilla entity type registered for this implementation.
    entity_type: EntityTypeRef,
    /// Synced data carrying the rendered item stack.
    entity_data: SyncMutex<EggEntityData>,
    /// Shared `Projectile` state (owner / left-owner / has-been-shot).
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EggEntity`.
unsafe impl DowncastType for EggEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/egg");
}

impl EggEntity {
    /// Creates a new thrown egg with no owner and default rendered item.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(EggEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates an egg from saved base data.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(EggEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Spawns chickens on impact according to vanilla RNG logic.
    fn try_spawn_chickens(&self, world: &Arc<World>) {
        // Vanilla: 1 in 8 chance (rand.nextInt(8) == 0)
        if rand::random::<u8>() % 8 != 0 {
            return;
        }

        // Vanilla: 1 in 32 chance for 4 chickens, otherwise 1 chicken
        let count = if rand::random::<u8>() % 32 == 0 {
            4
        } else {
            1
        };

        let pos = self.position();
        let world_weak = Arc::downgrade(world);

        for _ in 0..count {
            let Some(chicken) = ENTITIES.create(
                &vanilla_entities::CHICKEN,
                next_entity_id(),
                pos,
                world_weak.clone(),
            ) else {
                break;
            };

            if let Some(ageable) = chicken.as_ageable_mob() {
                ageable.set_baby(true);
            }

            if let Some(mob) = chicken.as_mob() {
                mob.finalize_spawn(world, EntitySpawnReason::Event, None);
            }

            if let Err(error) = world.try_add_entity(chicken) {
                log::debug!("failed to spawn chicken from egg: {error}");
            }
        }
    }
}

impl Entity for EggEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn tick(&self) {
        self.throwable_projectile_tick();
    }

    fn get_default_gravity(&self) -> f64 {
        self.throwable_default_gravity()
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn spawn_data(&self) -> i32 {
        self.get_owner().map_or(0, |owner| owner.id())
    }

    fn restore_owner_reference(&self, owner: &SharedEntity) {
        self.cache_owner_entity(owner);
    }

    fn projectile_owner_uuid(&self) -> Option<uuid::Uuid> {
        self.owner_uuid()
    }

    fn projectile_owner(&self) -> Option<SharedEntity> {
        self.get_owner()
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

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_projectile(nbt);
        self.save_throwable_item(nbt);
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_projectile(nbt);
        self.load_throwable_item(nbt);
    }
}

impl Projectile for EggEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_hit_entity(&self, entity: &SharedEntity, _location: DVec3) {
        let mut damage = DamageSource::environment(&vanilla_damage_types::THROWN)
            .with_direct_entity(self.id());
        if let Some(owner) = self.get_owner() {
            damage = damage.with_causing_entity(owner.id());
        }

        if let Some(world) = entity.level() {
            entity.hurt(&world, &damage, 0.0);
        }
    }

    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);

        if let Some(world) = self.level() {
            self.try_spawn_chickens(&world);
        }

        self.set_removed(RemovalReason::Discarded);
    }
}

impl ThrowableProjectile for EggEntity {}

impl ThrowableItemProjectile for EggEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::EGG
    }

    fn set_item(&self, item: ItemStack) {
        self.entity_data
            .lock()
            .throwable_item_projectile
            .item_stack
            .set(item);
    }

    fn get_item(&self) -> ItemStack {
        self.entity_data
            .lock()
            .throwable_item_projectile
            .item_stack
            .get()
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};

    use crate::entity::ThrowableItemProjectile;
    use crate::world::World;

    use super::EggEntity;

    #[test]
    fn default_item_is_egg() {
        init_vanilla_registry();

        let egg = EggEntity::new(
            &vanilla_entities::EGG,
            1,
            DVec3::ZERO,
            Weak::<World>::new(),
        );
        assert_eq!(egg.get_default_item().key, vanilla_items::EGG.key);
    }
}
