//! Thrown splash potion projectile entity (`ThrownSplashPotionEntity`).

use std::sync::{Arc, Weak};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::data_components::{PotionContents, vanilla_components};
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::items::ItemRef;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::vanilla_entity_data::SplashPotionEntityData;
use steel_registry::{
    level_events, sound_events, vanilla_blocks, vanilla_damage_types, vanilla_items,
    vanilla_potions,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, Direction, DowncastType, DowncastTypeKey};

use crate::behavior::potion_utils::{apply_potion_contents_effects, is_instant_effect};
use crate::entity::damage::DamageSource;
use crate::entity::{
    Entity, EntityBase, EntityBaseLoad, EntitySyncedData, Projectile, ProjectileBase,
    ProjectileHit, RemovalReason, SharedEntity, ThrowableItemProjectile, ThrowableProjectile,
};
use crate::world::World;

/// A thrown splash potion projectile.
#[entity_behavior(class = "ThrownSplashPotion")]
pub struct ThrownSplashPotionEntity {
    /// Common entity fields.
    base: EntityBase,
    /// Entity type ref.
    entity_type: EntityTypeRef,
    /// Synced entity data.
    entity_data: SyncMutex<SplashPotionEntityData>,
    /// Projectile base fields.
    projectile_base: ProjectileBase,
}

// SAFETY: This key is owned by Steel and uniquely identifies `ThrownSplashPotionEntity`.
unsafe impl DowncastType for ThrownSplashPotionEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/thrown_splash_potion");
}

impl ThrownSplashPotionEntity {
    /// Creates a new thrown splash potion entity.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self {
            base: EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
            entity_data: SyncMutex::new(SplashPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    /// Creates a thrown splash potion entity from saved base load state.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self {
            base: EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
            entity_data: SyncMutex::new(SplashPotionEntityData::new()),
            projectile_base: ProjectileBase::new(),
        }
    }

    fn splash(&self, world: &Arc<World>, hit_pos: DVec3, hit_direction: Option<Direction>) {
        let item_stack = self.get_item();
        let contents = item_stack
            .get(vanilla_components::POTION_CONTENTS)
            .cloned()
            .unwrap_or_else(PotionContents::empty);

        if contents.is(&vanilla_potions::WATER) {
            self.apply_water_splash(world, hit_pos, hit_direction);
        } else {
            self.apply_splash_effects(world, hit_pos, &contents);
        }

        let is_instant = contents.custom_effects().iter().any(|e| is_instant_effect(e.effect()))
            || contents.potion().map_or(false, |p| {
                p.value().effects.iter().any(|e| is_instant_effect(e.effect))
            });

        let event_id = if is_instant {
            level_events::PARTICLES_INSTANT_POTION_SPLASH
        } else {
            level_events::PARTICLES_SPELL_POTION_SPLASH
        };

        // Emit potion splash particles event
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

    fn apply_water_splash(&self, world: &Arc<World>, hit_pos: DVec3, hit_direction: Option<Direction>) {
        let center = BlockPos::containing(hit_pos.x, hit_pos.y, hit_pos.z);
        let owner = self.get_owner();
        let owner_ref = owner.as_deref();

        // Extinguish fire on entities within 4 blocks
        let bounds = self.bounding_box().inflate(4.0);
        let entities = world.get_entities_in_aabb(&bounds);
        for target in entities {
            if let Some(living) = target.as_living_entity() {
                let dist_sq = target.position().distance_squared(hit_pos);
                if dist_sq < 16.0 {
                    if target.entity_type().flags.is_sensitive_to_water {
                        let mut source = DamageSource::environment(&vanilla_damage_types::INDIRECT_MAGIC);
                        if let Some(src) = owner_ref {
                            source = source.with_causing_entity(src.id()).with_direct_entity(self.id());
                        }
                        living.hurt(world, &source, 1.0);
                    }
                    target.clear_fire();
                }
            }
        }

        // Extinguish fire on hit block or direction
        if let Some(direction) = hit_direction {
            let target_pos = center.relative(direction);
            let state = world.get_block_state(target_pos);
            if state.get_block() == &vanilla_blocks::FIRE {
                world.set_block(target_pos, vanilla_blocks::AIR.default_state(), UpdateFlags::UPDATE_ALL);
            }
        }

        // Convert convertable blocks (e.g. Mud)
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let pos = center.offset(dx, dy, dz);
                    let state = world.get_block_state(pos);
                    if state.get_block().has_tag(&BlockTag::CONVERTABLE_TO_MUD) {
                        world.set_block(pos, vanilla_blocks::MUD.default_state(), UpdateFlags::UPDATE_ALL);
                    }
                }
            }
        }
    }

    fn apply_splash_effects(&self, world: &Arc<World>, hit_pos: DVec3, contents: &PotionContents) {
        let bounds = self.bounding_box().inflate(4.0);
        let entities = world.get_entities_in_aabb(&bounds);
        let owner = self.get_owner();
        let owner_ref = owner.as_deref();

        for target in entities {
            let Some(living) = target.as_living_entity() else {
                continue;
            };

            if !living.is_affected_by_potions() {
                continue;
            }

            let dist_sq = target.position().distance_squared(hit_pos);
            if dist_sq < 16.0 {
                let dist = dist_sq.sqrt();
                let factor = 1.0 - (dist / 4.0);
                if factor > 0.0 {
                    apply_potion_contents_effects(
                        world,
                        living,
                        contents,
                        factor as f32,
                        owner_ref,
                    );
                }
            }
        }
    }
}

impl Entity for ThrownSplashPotionEntity {
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

impl Projectile for ThrownSplashPotionEntity {
    fn projectile_base(&self) -> &ProjectileBase {
        &self.projectile_base
    }

    fn on_hit(&self, hit: &ProjectileHit) {
        self.projectile_on_hit(hit);
        if let Some(world) = self.level() {
            let (location, direction) = match hit {
                ProjectileHit::Block { location, hit: block_hit } => (*location, Some(block_hit.direction)),
                ProjectileHit::Entity(entity_hit) => (entity_hit.location, None),
            };
            self.splash(&world, location, direction);
        }
    }
}

impl ThrowableProjectile for ThrownSplashPotionEntity {}

impl ThrowableItemProjectile for ThrownSplashPotionEntity {
    fn get_default_item(&self) -> ItemRef {
        &vanilla_items::SPLASH_POTION
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
