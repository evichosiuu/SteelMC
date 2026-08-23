//! Furnace, Blast Furnace, and Smoker menu.

use steel_registry::menu_type::MenuTypeRef;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a furnace, blast furnace, or smoker menu.
#[must_use]
pub fn furnace(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
    menu_type: MenuTypeRef,
) -> Menu {
    let container = container.into();

    let mut builder = MenuBuilder::new(menu_type, container_id);
    let furnace = builder.section(&container, 3);
    let player = builder.player_inventory(&inventory);

    builder.route(furnace, player.all(), FillDirection::Backward);
    builder.route(player.all(), furnace, FillDirection::Forward);

    builder.build(FurnaceKind { container })
}

/// Per-menu furnace state.
pub struct FurnaceKind {
    container: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind within the process.
unsafe impl steel_utils::DowncastType for FurnaceKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/furnace");
}

impl MenuKind for FurnaceKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}
