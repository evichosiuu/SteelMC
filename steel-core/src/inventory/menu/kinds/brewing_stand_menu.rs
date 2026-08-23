//! Brewing stand menu.

use steel_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a brewing stand menu.
#[must_use]
pub fn brewing_stand(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    let container = container.into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::BREWING_STAND, container_id);
    let brewing_stand = builder.section(&container, 5);
    let player = builder.player_inventory(&inventory);

    builder.route(brewing_stand, player.all(), FillDirection::Backward);
    builder.route(player.all(), brewing_stand, FillDirection::Forward);

    builder.build(BrewingStandKind { container })
}

/// Per-menu brewing stand state.
pub struct BrewingStandKind {
    container: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind within the process.
unsafe impl steel_utils::DowncastType for BrewingStandKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/brewing_stand");
}

impl MenuKind for BrewingStandKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}
