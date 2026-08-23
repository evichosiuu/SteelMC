//! Merchant / trading menu implementation.

use std::sync::{
    Arc,
    atomic::{AtomicI32, Ordering},
};

use steel_protocol::packets::game::CMerchantOffers;
use steel_registry::{item_stack::ItemStack, vanilla_menu_types};
use steel_utils::locks::{IntoShared, Shared, SyncMutex};

use crate::{
    inventory::{
        container::{ResultContainer, SimpleContainer},
        lock::{ContainerId, ContainerRef},
        prelude::*,
        slots::ResultHandler,
        trade::TradeOffer,
    },
    player::Player,
    player::player_inventory::PlayerInventory,
};

/// Shared state and hooks provided by the villager or merchant entity.
pub struct MerchantVillagerInfo {
    /// Container counter allocated for the trading screen.
    pub container_id: u8,
    /// Shared current level of the villager.
    pub villager_level: Arc<AtomicI32>,
    /// Shared current experience points of the villager.
    pub experience: Arc<AtomicI32>,
    /// Shared trade offers of the villager.
    pub offers: Arc<SyncMutex<Vec<TradeOffer>>>,
    /// Callback invoked when a trade is executed by a player.
    pub on_trade: Arc<dyn Fn(&Player, usize) + Send + Sync>,
}

/// Assembles a merchant trading menu.
#[must_use]
pub fn merchant(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    villager_info: MerchantVillagerInfo,
) -> Menu {
    let input_container = SimpleContainer::new(2).into_shared();
    let result_container = ResultContainer::new().into_shared();
    let selected_trade = Arc::new(AtomicI32::new(-1));

    let handler = MerchantResultHandler {
        input_container: input_container.clone(),
        result_container: result_container.clone(),
        selected_trade: selected_trade.clone(),
        villager_info_offers: villager_info.offers.clone(),
        on_trade: villager_info.on_trade.clone(),
    };

    let mut builder = MenuBuilder::new(&vanilla_menu_types::MERCHANT, container_id);

    let input = builder.section_all(&input_container);
    let result = builder.result_slot(handler);
    let player = builder.player_inventory(&inventory);

    builder.route_with_remainder_policy(
        result,
        player.all(),
        FillDirection::Backward,
        FakeResultRemainderPolicy::Discard,
    );
    builder.route(input, player.all(), FillDirection::Forward);
    builder.route(player.hotbar(), input, FillDirection::Forward);
    builder.route(player.main(), input, FillDirection::Forward);
    builder.drain(input);

    builder.build(MerchantKind {
        input_container,
        result_container,
        selected_trade,
        villager_info,
    })
}

struct MerchantResultHandler {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    selected_trade: Arc<AtomicI32>,
    villager_info_offers: Arc<SyncMutex<Vec<TradeOffer>>>,
    on_trade: Arc<dyn Fn(&Player, usize) + Send + Sync>,
}

impl ResultHandler for MerchantResultHandler {
    fn result_container(&self) -> ContainerRef {
        ContainerRef::from(self.result_container.clone())
    }

    fn dependencies(&self) -> Vec<ContainerRef> {
        vec![ContainerRef::from(self.input_container.clone())]
    }

    fn update_result(&self, guard: &mut ContainerLockGuard) {
        let trade_index = self.selected_trade.load(Ordering::Relaxed);
        let Some([input_container, result_container]) = guard.get_disjoint_mut([
            ContainerId::from_arc(&self.input_container),
            ContainerId::from_arc(&self.result_container),
        ]) else {
            return;
        };

        if trade_index < 0 {
            result_container.set_item(0, ItemStack::empty());
            return;
        }

        let offers = self.villager_info_offers.lock();
        let Some(offer) = offers.get(trade_index as usize) else {
            result_container.set_item(0, ItemStack::empty());
            return;
        };

        if offer.is_out_of_stock() {
            result_container.set_item(0, ItemStack::empty());
            return;
        }

        let req_a = offer.get_cost_a();
        let req_b = offer.get_cost_b();

        let in_a = input_container.get_item(0);
        let in_b = input_container.get_item(1);

        let matches_a = in_a.count >= req_a.count && ItemStack::is_same_item(in_a, &req_a);
        let matches_b = match req_b {
            Some(ref b) => in_b.count >= b.count && ItemStack::is_same_item(in_b, b),
            None => in_b.is_empty(),
        };

        if matches_a && matches_b {
            result_container.set_item(0, offer.result.clone());
        } else {
            result_container.set_item(0, ItemStack::empty());
        }
    }

    fn on_result_taken(
        &self,
        guard: &mut ContainerLockGuard,
        player: &Player,
    ) -> Option<ItemStack> {
        let trade_index = self.selected_trade.load(Ordering::Relaxed);
        let Some([input_container, _result_container]) = guard.get_disjoint_mut([
            ContainerId::from_arc(&self.input_container),
            ContainerId::from_arc(&self.result_container),
        ]) else {
            return None;
        };

        if trade_index < 0 {
            return None;
        }

        let offers = self.villager_info_offers.lock();
        let Some(offer) = offers.get(trade_index as usize) else {
            return None;
        };

        let req_a = offer.get_cost_a();
        let req_b = offer.get_cost_b();

        input_container.get_item_mut(0).shrink(req_a.count);
        if let Some(ref b) = req_b {
            input_container.get_item_mut(1).shrink(b.count);
        }
        drop(offers);

        (self.on_trade)(player, trade_index as usize);

        None
    }

    fn is_result_valid(&self, guard: &ContainerLockGuard, _player: &Player) -> bool {
        let trade_index = self.selected_trade.load(Ordering::Relaxed);
        let Some(input_container) =
            guard.get_typed::<SimpleContainer>(ContainerId::from_arc(&self.input_container))
        else {
            return false;
        };
        let Some(result_container) =
            guard.get_typed::<ResultContainer>(ContainerId::from_arc(&self.result_container))
        else {
            return false;
        };

        if trade_index < 0 {
            return result_container.get_item(0).is_empty();
        }

        let offers = self.villager_info_offers.lock();
        let Some(offer) = offers.get(trade_index as usize) else {
            return result_container.get_item(0).is_empty();
        };

        if offer.is_out_of_stock() {
            return result_container.get_item(0).is_empty();
        }

        let req_a = offer.get_cost_a();
        let req_b = offer.get_cost_b();

        let in_a = input_container.get_item(0);
        let in_b = input_container.get_item(1);

        let matches_a = in_a.count >= req_a.count && ItemStack::is_same_item(in_a, &req_a);
        let matches_b = match req_b {
            Some(ref b) => in_b.count >= b.count && ItemStack::is_same_item(in_b, b),
            None => in_b.is_empty(),
        };

        if matches_a && matches_b {
            ItemStack::is_same_item_same_components(result_container.get_item(0), &offer.result)
        } else {
            result_container.get_item(0).is_empty()
        }
    }
}

/// Per-menu merchant state handling trade selection and synchronization.
pub struct MerchantKind {
    input_container: Shared<SimpleContainer>,
    result_container: Shared<ResultContainer>,
    selected_trade: Arc<AtomicI32>,
    villager_info: MerchantVillagerInfo,
}

unsafe impl steel_utils::DowncastType for MerchantKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/merchant");
}

impl MenuKind for MerchantKind {
    fn on_open(
        &mut self,
        _behavior: &mut MenuBehavior,
        _guard: &mut ContainerLockGuard,
        player: &Player,
    ) {
        let offers = self
            .villager_info
            .offers
            .lock()
            .iter()
            .map(TradeOffer::to_packet_offer)
            .collect();

        player.send_packet(CMerchantOffers {
            container_id: i32::from(self.villager_info.container_id),
            offers,
            villager_level: self.villager_info.villager_level.load(Ordering::Relaxed),
            experience: self.villager_info.experience.load(Ordering::Relaxed),
            is_regular_villager: true,
            can_restock: true,
        });
    }

    fn on_select_trade(
        &mut self,
        behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        trade_index: usize,
        player: &Player,
    ) {
        self.selected_trade
            .store(trade_index as i32, Ordering::Relaxed);

        let offers = self.villager_info.offers.lock();
        if let Some(offer) = offers.get(trade_index) {
            let req_a = offer.get_cost_a();
            let req_b = offer.get_cost_b();

            let Some(input_container) = guard.get_mut(ContainerId::from_arc(&self.input_container))
            else {
                return;
            };

            input_container.set_item(0, req_a);
            if let Some(b) = req_b {
                input_container.set_item(1, b);
            } else {
                input_container.set_item(1, ItemStack::empty());
            }
        }
        drop(offers);

        self.slots_changed(behavior, guard, player);
        behavior.broadcast_changes(&player.connection);
    }

    fn slots_changed(
        &mut self,
        _behavior: &mut MenuBehavior,
        guard: &mut ContainerLockGuard,
        _player: &Player,
    ) {
        let trade_index = self.selected_trade.load(Ordering::Relaxed);
        if trade_index < 0 {
            return;
        }

        let handler = MerchantResultHandler {
            input_container: self.input_container.clone(),
            result_container: self.result_container.clone(),
            selected_trade: self.selected_trade.clone(),
            villager_info_offers: self.villager_info.offers.clone(),
            on_trade: self.villager_info.on_trade.clone(),
        };

        handler.update_result(guard);
    }
}
