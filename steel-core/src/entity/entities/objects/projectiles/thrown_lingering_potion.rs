//! Thrown lingering potion projectile entity (`ThrownLingeringPotionEntity`).

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::data_components::{PotionContents, vanilla_components};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::vanilla_entity_data::LingeringPotionEntityData;
use steel_registry::{
    level_events, sound_events, vanilla_entities, vanilla_items,
};
use steel_utils::locks::SyncMutex;
use steel_utils::{BlockPos, DowncastType, DowncastTypeKey};

use crate::behavior::potion_utils::is_instant_effect;
use crate::entity::damage::DamageSource;
use crate::entity::entities::AreaEffectCloudEntity;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileHit, RemovalReason, SharedEntity, ThrowableItemProjectile, ThrowableProjectile,
    next_entity_id,
};
use crate::world::World;

/// A thrown lingering potion projectile.
#[entity_behavior(class = "ThrownLingeringPotion")]
pub struct ThrownLingeringPotionEntity {
    /// Common entity fields.
    base: EntityBase,
    /// Entity type ref.
    entity_type: EntityTypeRef,
    /// Synced entity data.
    entity_data: SyncMutex<LingeringPotionEntityData>,
    /// Projectile base fields.
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ThrownLingeringPotionEntity`.
unsafe impl DowncastType for ThrownLingeringPotionEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/thrown_lingering_potion");
}

impl ThrownLingeringPotionEntity {
    /// Creates a new thrown lingering potion entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(LingeringPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a thrown lingering potion entity from saved base load state.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(LingeringPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    fn splash(&self, world: &Arc<World>, hit_pos: DVec3) {
        let item_stack = self.get_item();
        let contents = item_stack
            .get(vanilla_components::POTION_CONTENTS)
            .cloned()
            .unwrap_or_else(PotionContents::empty);

        // Spawn AreaEffectCloud
        let cloud = Arc::new(AreaEffectCloudEntity::new(
            &vanilla_entities::AREA_EFFECT_CLOUD,
            next_entity_id(),
            hit_pos,
            Arc::downgrade(world),
        ));

        if let Some(owner) = self.get_owner() {
            cloud.set_owner_entity(Some(&owner));
        }

        cloud.set_radius(3.0);
        cloud.set_radius_on_use(-0.5);
        cloud.set_wait_time(10);
        cloud.set_radius_per_tick(-cloud.radius() / cloud.duration() as f32);
        cloud.set_potion_contents(contents.clone());

        let _ = world.try_add_entity(cloud as SharedEntity);

        let is_instant = contents.custom_effects().iter().any(|e| is_instant_effect(e.effect()))
            || contents.potion().map_or(false, |p| {
                p.value().effects.iter().any(|e| is_instant_effect(e.effect))
            });

        let event_id = if is_instant {
            level_events::PARTICLES_INSTANT_POTION_SPLASH
        } else {
            level_events::PARTICLES_SPELL_POTION_SPLASH
        };

        world.level_event(
            event_id,
            BlockPos::containing(hit_pos.x, hit_pos.y, hit_pos.z),
            contents.custom_color().unwrap_or(0x385dc6),
            None,
        );

        world.play_sound_at(
            &sound_events::ENTITY_GENERIC_SPLASH,
            SoundSource::Neutral,
            hit_pos,
            1.0,
            rand::random::<f32>() * 0.1 + 0.9,
            None,
        );

        self.set_removed(RemovalReason::Discarded);
    }
}

impl Entity for ThrownLingeringPotionEntity {
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

impl Projectile for ThrownLingeringPotionEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        if let Some(world) = self.level() {
            let location = match hit {
                ProjectileHit::Block { location, .. } => *location,
                ProjectileHit::Entity(entity_hit) => entity_hit.location,
            };
            self.splash(&world, location);
        }
    }
}

impl ThrowableProjectile for ThrownLingeringPotionEntity {}

impl ThrowableItemProjectile for ThrownLingeringPotionEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::LINGERING_POTION
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
