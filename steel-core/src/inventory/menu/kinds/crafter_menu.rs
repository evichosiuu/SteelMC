//! Crafter menu (3x3 grid).

use steel_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a crafter menu.
#[must_use]
pub fn crafter(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    let container = container.into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::CRAFTER_3X3, container_id);
    let crafter = builder.section(&container, 9);
    let player = builder.player_inventory(&inventory);

    builder.route(crafter, player.all(), FillDirection::Backward);
    builder.route(player.all(), crafter, FillDirection::Forward);

    builder.build(CrafterKind { container })
}

/// Per-menu crafter state.
pub struct CrafterKind {
    container: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind within the process.
unsafe impl steel_utils::DowncastType for CrafterKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/crafter");
}

impl MenuKind for CrafterKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}
