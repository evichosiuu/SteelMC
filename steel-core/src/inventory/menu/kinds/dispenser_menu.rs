//! Dispenser and Dropper menu (3x3 grid).

use steel_registry::vanilla_menu_types;

use crate::inventory::prelude::*;
use crate::player::player_inventory::PlayerInventory;

/// Builds a dispenser/dropper menu (3x3 grid).
#[must_use]
pub fn dispenser(
    inventory: Shared<PlayerInventory>,
    container_id: u8,
    container: impl Into<ContainerRef>,
) -> Menu {
    let container = container.into();

    let mut builder = MenuBuilder::new(&vanilla_menu_types::GENERIC_3X3, container_id);
    let dispenser = builder.section(&container, 9);
    let player = builder.player_inventory(&inventory);

    builder.route(dispenser, player.all(), FillDirection::Backward);
    builder.route(player.all(), dispenser, FillDirection::Forward);

    builder.build(DispenserKind { container })
}

/// Per-menu dispenser/dropper state.
pub struct DispenserKind {
    container: ContainerRef,
}

// SAFETY: This Steel-owned key uniquely identifies the concrete menu kind within the process.
unsafe impl steel_utils::DowncastType for DispenserKind {
    const TYPE_KEY: steel_utils::DowncastTypeKey =
        steel_utils::DowncastTypeKey::new("steel:menu/dispenser");
}

impl MenuKind for DispenserKind {
    fn still_valid(&self, _behavior: &MenuBehavior, player: &Player) -> bool {
        self.container.still_valid(player)
    }
}
