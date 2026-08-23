//! Hopper menu.

use steel_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a hopper menu.
#[must_use]
pub fn hopper(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    let container = container.into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::HOPPER, container_id);
    let hopper = builder.section(&container, 5);
    let player = builder.player_inventory(&inventory);

    builder.route(hopper, player.all(), FillDirection::Backward);
    builder.route(player.all(), hopper, FillDirection::Forward);

    builder.build(HopperKind { container })
}

/// Per-menu hopper state.
pub struct HopperKind {
    container: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind within the process.
unsafe impl steel_utils::DowncastType for HopperKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/hopper");
}

impl MenuKind for HopperKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}
