//! Vanilla Villager entity with POI workstation claiming, trading, restocking, and level progression.

use std::str::FromStr;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicI32, Ordering},
};

use glam::DVec3;
use simdnbt::borrow::NbtCompound as BorrowedNbtCompoundView;
use simdnbt::owned::NbtCompound;
use steel_macros::entity_behavior;
use steel_protocol::packets::game::{CMerchantOffers, SoundSource};
use steel_registry::entity_data::VillagerData;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_entity_data::VillagerEntityData;
use steel_registry::villager_profession::VillagerProfessionRef;
use steel_registry::{
    REGISTRY, RegistryEntry, RegistryExt, sound_events, vanilla_attributes,
    vanilla_villager_professions, vanilla_villager_types,
};
use steel_utils::locks::SyncMutex;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Downcast, DowncastType, DowncastTypeKey, Identifier};

use crate::behavior::InteractionResult;
use crate::entity::ai::goal::{
    FloatGoal, LookAtPlayerGoal, MoveToBlockGoal, PanicGoal, RandomLookAroundGoal,
    WaterAvoidingRandomStrollGoal,
};
use crate::entity::damage::DamageSource;
use crate::entity::{
    AgeableMob, AgeableMobBase, Entity, EntityBase, EntityBaseLoad, EntitySyncedData,
    LivingEntity, LivingEntityBase, Mob, MobBase, PathfinderMob,
};
use crate::inventory::menu::kinds::{MerchantVillagerInfo, merchant};
use crate::inventory::trade::{TradeOffer, generate_trades_for_level};
use crate::physics::MoveResult;
use crate::player::Player;
use crate::world::World;

const DEFAULT_STEP_HEIGHT: f32 = 0.6;

#[entity_behavior(class = "Villager")]
/// Vanilla villager entity.
pub struct VillagerEntity {
    base: EntityBase,
    entity_type: EntityTypeRef,
    living_base: LivingEntityBase,
    mob_base: MobBase,
    ageable_base: AgeableMobBase,
    entity_data: SyncMutex<VillagerEntityData>,
    offers: Arc<SyncMutex<Vec<TradeOffer>>>,
    xp: Arc<AtomicI32>,
    level: Arc<AtomicI32>,
    job_site: SyncMutex<Option<BlockPos>>,
    last_restock: SyncMutex<i64>,
    restocks_today: SyncMutex<i32>,
    trading_player: SyncMutex<Option<i32>>,
}

unsafe impl DowncastType for VillagerEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:entity/villager");
}

impl VillagerEntity {
    /// Creates a new villager at runtime.
    #[must_use]
    pub fn new(entity_type: EntityTypeRef, id: i32, position: DVec3, world: Weak<World>) -> Self {
        Self::new_with_base(
            EntityBase::new(id, position, entity_type.dimensions, world),
            entity_type,
        )
    }

    /// Reconstructs a villager from persisted base entity state.
    #[must_use]
    pub fn from_saved(entity_type: EntityTypeRef, load: EntityBaseLoad) -> Self {
        Self::new_with_base(
            EntityBase::from_load(load, entity_type.dimensions),
            entity_type,
        )
    }

    fn new_with_base(base: EntityBase, entity_type: EntityTypeRef) -> Self {
        let living_base = LivingEntityBase::new(entity_type);
        let mob_base = MobBase::new();
        let ageable_base = AgeableMobBase::new();
        let mut entity_data = VillagerEntityData::new();
        living_base.initialize_synced_data(&mut entity_data);

        let default_type = REGISTRY
            .villager_types
            .by_key(&vanilla_villager_types::PLAINS.key)
            .map_or(0, |v| v.id() as i32);
        let default_prof = REGISTRY
            .villager_professions
            .by_key(&vanilla_villager_professions::NONE.key)
            .map_or(0, |v| v.id() as i32);

        entity_data
            .villager_mut()
            .villager_data
            .set(VillagerData::new(default_type, default_prof, 1));

        {
            let mut goal_selector = mob_base.goal_selector().lock();
            goal_selector.add_goal(0, FloatGoal::new(&mob_base));
            goal_selector.add_goal(1, PanicGoal::new(2.0));
            goal_selector.add_goal(2, MoveToBlockGoal::new(1.0, 16, |_level, _pos| false));
            goal_selector.add_goal(5, WaterAvoidingRandomStrollGoal::new(1.0));
            goal_selector.add_goal(6, LookAtPlayerGoal::new(6.0));
            goal_selector.add_goal(7, RandomLookAroundGoal::new());
        }

        Self {
            base,
            entity_type,
            living_base,
            mob_base,
            ageable_base,
            entity_data: SyncMutex::new(entity_data),
            offers: Arc::new(SyncMutex::new(Vec::new())),
            xp: Arc::new(AtomicI32::new(0)),
            level: Arc::new(AtomicI32::new(1)),
            job_site: SyncMutex::new(None),
            last_restock: SyncMutex::new(0),
            restocks_today: SyncMutex::new(0),
            trading_player: SyncMutex::new(None),
        }
    }

    /// Returns current villager level (1 to 5).
    #[must_use]
    pub fn villager_level(&self) -> i32 {
        self.level.load(Ordering::Relaxed)
    }

    /// Sets villager level and updates synced entity data.
    pub fn set_villager_level(&self, level: i32) {
        self.level.store(level, Ordering::Relaxed);
        let mut entity_data = self.entity_data.lock();
        let current = entity_data.villager().villager_data.get().clone();
        entity_data
            .villager_mut()
            .villager_data
            .set(VillagerData::new(
                current.villager_type,
                current.profession,
                level,
            ));
    }

    /// Returns current villager XP.
    #[must_use]
    pub fn villager_xp(&self) -> i32 {
        self.xp.load(Ordering::Relaxed)
    }

    /// Adds XP and checks for level progression.
    pub fn add_villager_xp(&self, amount: i32) {
        let new_xp = self.xp.fetch_add(amount, Ordering::Relaxed) + amount;
        let current_level = self.villager_level();

        let xp_threshold = match current_level {
            1 => 10,
            2 => 70,
            3 => 150,
            4 => 250,
            _ => i32::MAX,
        };

        if new_xp >= xp_threshold && current_level < 5 {
            let next_level = current_level + 1;
            self.set_villager_level(next_level);

            if let Some(prof) = self.profession() {
                let new_trades = generate_trades_for_level(&prof.key, next_level);
                self.offers.lock().extend(new_trades);
            }

            self.play_sound(&sound_events::ENTITY_VILLAGER_YES, 1.0, 1.0);
        }
    }

    /// Returns active villager profession.
    #[must_use]
    pub fn profession(&self) -> Option<VillagerProfessionRef> {
        let prof_id = self
            .entity_data
            .lock()
            .villager()
            .villager_data
            .get()
            .profession;
        REGISTRY
            .villager_professions
            .by_id(usize::try_from(prof_id).ok()?)
    }

    /// Sets villager profession.
    pub fn set_profession(&self, profession: VillagerProfessionRef) {
        let mut entity_data = self.entity_data.lock();
        let current = entity_data.villager().villager_data.get().clone();
        entity_data
            .villager_mut()
            .villager_data
            .set(VillagerData::new(
                current.villager_type,
                profession.id() as i32,
                current.level,
            ));
    }

    /// Restocks all trade offers up to 2 times per day.
    pub fn restock(&self) {
        let mut count = self.restocks_today.lock();
        if *count >= 2 {
            return;
        }

        let mut offers = self.offers.lock();
        for offer in offers.iter_mut() {
            offer.restock();
        }
        *count += 1;
    }

    /// Returns claiming POI profession for a block state.
    fn profession_for_block_state(state_id: BlockStateId) -> Option<VillagerProfessionRef> {
        let poi_type = REGISTRY.poi_types.type_for_state(state_id)?;

        let key_str = poi_type.key.path.as_ref();
        let prof_key = match key_str {
            "armorer" => &vanilla_villager_professions::ARMORER.key,
            "butcher" => &vanilla_villager_professions::BUTCHER.key,
            "cartographer" => &vanilla_villager_professions::CARTOGRAPHER.key,
            "cleric" => &vanilla_villager_professions::CLERIC.key,
            "farmer" => &vanilla_villager_professions::FARMER.key,
            "fisherman" => &vanilla_villager_professions::FISHERMAN.key,
            "fletcher" => &vanilla_villager_professions::FLETCHER.key,
            "leatherworker" => &vanilla_villager_professions::LEATHERWORKER.key,
            "librarian" => &vanilla_villager_professions::LIBRARIAN.key,
            "mason" => &vanilla_villager_professions::MASON.key,
            "shepherd" => &vanilla_villager_professions::SHEPHERD.key,
            "toolsmith" => &vanilla_villager_professions::TOOLSMITH.key,
            "weaponsmith" => &vanilla_villager_professions::WEAPONSMITH.key,
            _ => return None,
        };

        REGISTRY.villager_professions.by_key(prof_key)
    }

    /// Scans surrounding blocks for workstations and updates profession/job site.
    fn check_workstation_claiming(&self) {
        let Some(world) = self.level() else {
            return;
        };

        // Reset restocks count on a day cycle (every 24,000 ticks)
        if world.game_time() % 24000 == 0 {
            *self.restocks_today.lock() = 0;
        }

        let is_unemployed = self
            .profession()
            .is_none_or(|p| p.key == vanilla_villager_professions::NONE.key);

        let current_job_site = *self.job_site.lock();

        if let Some(pos) = current_job_site {
            let state = world.get_block_state(pos);
            let valid = Self::profession_for_block_state(state).is_some();

            if !valid {
                *self.job_site.lock() = None;
                if self.villager_xp() == 0 {
                    if let Some(none_prof) = REGISTRY
                        .villager_professions
                        .by_key(&vanilla_villager_professions::NONE.key)
                    {
                        self.set_profession(none_prof);
                    }
                    self.offers.lock().clear();
                }
            } else {
                // Restock if near job site
                let (cx, cy, cz) = pos.get_center();
                if self.position().distance_squared(DVec3::new(cx, cy, cz)) <= 9.0 {
                    self.restock();
                }
            }
            return;
        }

        if !is_unemployed {
            return;
        }

        let center = self.block_position();
        'search: for dx in -8..=8 {
            for dy in -3..=3 {
                for dz in -8..=8 {
                    let target_pos = center.offset(dx, dy, dz);
                    let state = world.get_block_state(target_pos);
                    if let Some(prof) = Self::profession_for_block_state(state) {
                        *self.job_site.lock() = Some(target_pos);
                        self.set_profession(prof);
                        self.set_villager_level(1);

                        let initial_trades = generate_trades_for_level(&prof.key, 1);
                        *self.offers.lock() = initial_trades;

                        self.play_sound(&sound_events::ENTITY_VILLAGER_YES, 1.0, 1.0);
                        break 'search;
                    }
                }
            }
        }
    }
}

impl Entity for VillagerEntity {
    fn base(&self) -> &EntityBase {
        &self.base
    }

    fn entity_type(&self) -> EntityTypeRef {
        self.entity_type
    }

    fn base_tick(&self) {
        Mob::base_tick_mob(self);
    }

    fn synced_data(&self) -> Option<&dyn EntitySyncedData> {
        Some(&self.entity_data)
    }

    fn max_up_step(&self) -> f32 {
        self.attributes()
            .lock()
            .get_value(vanilla_attributes::STEP_HEIGHT)
            .unwrap_or(f64::from(DEFAULT_STEP_HEIGHT)) as f32
    }

    fn sound_source(&self) -> SoundSource {
        SoundSource::Neutral
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        self.save_mob(nbt);
        self.save_ageable_mob(nbt);

        let vdata = self.entity_data.lock().villager().villager_data.get().clone();
        let vtype = REGISTRY
            .villager_types
            .by_id(vdata.villager_type as usize)
            .map_or("minecraft:plains", |v| v.key.path.as_ref());
        let prof = REGISTRY
            .villager_professions
            .by_id(vdata.profession as usize)
            .map_or("minecraft:none", |v| v.key.path.as_ref());

        let mut villager_data_nbt = NbtCompound::new();
        villager_data_nbt.insert("type", vtype);
        villager_data_nbt.insert("profession", prof);
        villager_data_nbt.insert("level", self.villager_level());

        nbt.insert("VillagerData", villager_data_nbt);
        nbt.insert("Xp", self.villager_xp());
        nbt.insert("LastRestock", *self.last_restock.lock());
        nbt.insert("RestocksToday", *self.restocks_today.lock());

        let offers = self.offers.lock();
        let mut recipe_list = Vec::new();
        for offer in offers.iter() {
            let mut tag = NbtCompound::new();
            offer.save_nbt(&mut tag);
            recipe_list.push(simdnbt::owned::NbtTag::Compound(tag));
        }
        let mut offers_nbt = NbtCompound::new();
        offers_nbt.insert(
            "Recipes",
            simdnbt::owned::NbtTag::List(simdnbt::owned::NbtList::Compound(
                recipe_list
                    .into_iter()
                    .filter_map(|t| match t {
                        simdnbt::owned::NbtTag::Compound(c) => Some(c),
                        _ => None,
                    })
                    .collect(),
            )),
        );
        nbt.insert("Offers", offers_nbt);

        if let Some(pos) = *self.job_site.lock() {
            let mut mems = NbtCompound::new();
            let mut pos_nbt = NbtCompound::new();
            pos_nbt.insert("pos", simdnbt::owned::NbtTag::IntArray(vec![pos.x(), pos.y(), pos.z()]));
            mems.insert("minecraft:job_site", pos_nbt);
            let mut brain = NbtCompound::new();
            brain.insert("memories", mems);
            nbt.insert("Brain", brain);
        }
    }

    fn load_additional(&self, nbt: BorrowedNbtCompoundView<'_, '_>) {
        self.load_mob(nbt);
        self.load_ageable_mob(nbt);

        if let Some(xp) = nbt.int("Xp") {
            self.xp.store(xp, Ordering::Relaxed);
        }
        if let Some(lr) = nbt.long("LastRestock") {
            *self.last_restock.lock() = lr;
        }
        if let Some(rt) = nbt.int("RestocksToday") {
            *self.restocks_today.lock() = rt;
        }

        if let Some(vdata) = nbt.compound("VillagerData") {
            let level = vdata.int("level").unwrap_or(1);
            self.set_villager_level(level);

            if let Some(prof_str) = vdata.string("profession")
                && let Ok(key) = Identifier::from_str(prof_str.to_str().as_ref())
                && let Some(prof) = REGISTRY.villager_professions.by_key(&key)
            {
                self.set_profession(prof);
            }
        }

        if let Some(offers_nbt) = nbt.compound("Offers")
            && let Some(recipes) = offers_nbt.list("Recipes")
            && let Some(compounds) = recipes.compounds()
        {
            let mut loaded_offers = Vec::new();
            for recipe_compound in compounds {
                if let Some(offer) = TradeOffer::load_nbt(recipe_compound) {
                    loaded_offers.push(offer);
                }
            }
            *self.offers.lock() = loaded_offers;
        }

        if let Some(brain) = nbt.compound("Brain")
            && let Some(mems) = brain.compound("memories")
            && let Some(job) = mems.compound("minecraft:job_site")
            && let Some(pos_list) = job.int_array("pos")
            && pos_list.len() == 3
        {
            *self.job_site.lock() = Some(BlockPos::new(pos_list[0], pos_list[1], pos_list[2]));
        }
    }
}

impl LivingEntity for VillagerEntity {
    fn living_base(&self) -> &LivingEntityBase {
        &self.living_base
    }

    fn get_health(&self) -> f32 {
        *self.entity_data.lock().living_entity().health.get()
    }

    fn set_health(&self, health: f32) {
        let max_health = self.get_max_health();
        let clamped = health.clamp(0.0, max_health);
        self.entity_data
            .lock()
            .living_entity_mut()
            .health
            .set(clamped);
    }

    fn sound_volume(&self) -> f32 {
        0.4
    }

    fn hurt_sound(&self, _source: &DamageSource) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_VILLAGER_HURT)
    }

    fn death_sound(&self) -> Option<SoundEventRef> {
        Some(&sound_events::ENTITY_VILLAGER_DEATH)
    }

    fn server_ai_step(&self) {
        Mob::mob_server_ai_step(self);
    }

    fn ai_step(&self) -> Option<MoveResult> {
        let result = self.default_ai_step();
        AgeableMob::tick_ageable_mob(self);
        if self.base().tick_count() % 100 == 0 {
            self.check_workstation_claiming();
        }
        result
    }
}

impl AgeableMob for VillagerEntity {
    fn ageable_base(&self) -> &AgeableMobBase {
        &self.ageable_base
    }

    fn is_age_locked(&self) -> bool {
        *self.entity_data.lock().ageable_mob().age_locked.get()
    }

    fn set_age_locked(&self, age_locked: bool) {
        self.entity_data
            .lock()
            .ageable_mob_mut()
            .age_locked
            .set(age_locked);
    }

    fn set_synced_baby(&self, baby: bool) {
        self.entity_data.lock().ageable_mob_mut().baby.set(baby);
    }

    fn age_boundary_changed(&self, _baby: bool) {
        self.refresh_dimensions();
    }
}

impl Mob for VillagerEntity {
    fn mob_base(&self) -> &MobBase {
        &self.mob_base
    }

    fn tick_goal_selectors(&self) {
        PathfinderMob::tick_pathfinder_goal_selectors(self);
    }

    fn tick_path_navigation(&self) {
        PathfinderMob::tick_pathfinder_path_navigation(self);
    }

    fn mob_interact(&self, player: &Player, _hand: InteractionHand) -> InteractionResult {
        if AgeableMob::is_baby(self) {
            return InteractionResult::Pass;
        }

        let is_nitwit = self
            .profession()
            .is_some_and(|p| p.key == vanilla_villager_professions::NITWIT.key);

        if is_nitwit {
            self.play_sound(&sound_events::ENTITY_VILLAGER_NO, 1.0, 1.0);
            return InteractionResult::Success;
        }

        if self.offers.lock().is_empty() {
            if let Some(prof) = self.profession() {
                if prof.key != vanilla_villager_professions::NONE.key {
                    let trades = generate_trades_for_level(&prof.key, self.villager_level());
                    *self.offers.lock() = trades;
                }
            }
        }

        if self.offers.lock().is_empty() {
            self.play_sound(&sound_events::ENTITY_VILLAGER_NO, 1.0, 1.0);
            return InteractionResult::Success;
        }

        *self.trading_player.lock() = Some(player.id());

        let offers_cb = Arc::clone(&self.offers);
        let level_cb = Arc::clone(&self.level);
        let xp_cb = Arc::clone(&self.xp);
        let villager_id = self.id();
        let world_weak = self.level().map(|w| Arc::downgrade(&w));

        let on_trade = Arc::new(move |trader: &Player, trade_index: usize| {
            let mut offers_guard = offers_cb.lock();
            if let Some(offer) = offers_guard.get_mut(trade_index) {
                offer.increment_uses();
                let trade_xp = offer.xp;
                drop(offers_guard);

                if let Some(ref w_weak) = world_weak
                    && let Some(world) = w_weak.upgrade()
                    && let Some(villager_entity) = world.get_entity_by_id(villager_id)
                    && let Some(villager) = villager_entity.downcast_ref::<VillagerEntity>()
                {
                    villager.add_villager_xp(trade_xp);
                }
            }

            let updated_offers = offers_cb
                .lock()
                .iter()
                .map(TradeOffer::to_packet_offer)
                .collect();

            trader.send_packet(CMerchantOffers {
                container_id: 1, // updated via menu container id in packet broadcast if open
                offers: updated_offers,
                villager_level: level_cb.load(Ordering::Relaxed),
                experience: xp_cb.load(Ordering::Relaxed),
                is_regular_villager: true,
                can_restock: true,
            });
        });

        let offers = Arc::clone(&self.offers);
        let level = Arc::clone(&self.level);
        let xp = Arc::clone(&self.xp);

        player.open_menu("Villager", move |ctx| {
            merchant(
                ctx.player.inventory.clone(),
                ctx.container_id,
                MerchantVillagerInfo {
                    container_id: ctx.container_id,
                    villager_level: level,
                    experience: xp,
                    offers,
                    on_trade,
                },
            )
        });

        InteractionResult::Success
    }

    fn mob_flags(&self) -> i8 {
        *self.entity_data.lock().mob().mob_flags.get()
    }

    fn set_mob_flags(&self, flags: i8) {
        self.entity_data.lock().mob_mut().mob_flags.set(flags);
    }
}

impl PathfinderMob for VillagerEntity {}

#[cfg(test)]
mod tests;
