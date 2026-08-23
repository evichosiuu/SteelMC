//! Villager trade offers and trade generation.

use simdnbt::{borrow::NbtCompound as BorrowedNbtCompound, owned::NbtCompound};
use steel_protocol::packets::game::MerchantOffer;
use steel_registry::{
    item_stack::ItemStack,
    vanilla_items,
};
use steel_utils::Identifier;

/// A single trade offer from a villager or merchant.
#[derive(Clone, Debug, PartialEq)]
pub struct TradeOffer {
    /// Base item stack required in slot 0.
    pub base_cost_a: ItemStack,
    /// Optional item stack required in slot 1.
    pub cost_b: Option<ItemStack>,
    /// Result item stack provided in slot 2.
    pub result: ItemStack,
    /// Current number of times this trade has been executed.
    pub uses: i32,
    /// Maximum number of times this trade can be used before restocking.
    pub max_uses: i32,
    /// Villager experience points granted when this trade is executed.
    pub xp: i32,
    /// Special price difference (discount or penalty).
    pub special_price_diff: i32,
    /// Demand price multiplier.
    pub price_multiplier: f32,
    /// Accumulated demand modifier.
    pub demand: i32,
}

impl TradeOffer {
    /// Creates a new trade offer with zero initial uses and demand.
    #[must_use]
    pub fn new(
        base_cost_a: ItemStack,
        cost_b: Option<ItemStack>,
        result: ItemStack,
        max_uses: i32,
        xp: i32,
        price_multiplier: f32,
    ) -> Self {
        Self {
            base_cost_a,
            cost_b,
            result,
            uses: 0,
            max_uses,
            xp,
            special_price_diff: 0,
            price_multiplier,
            demand: 0,
        }
    }

    /// Returns whether this trade offer is out of stock.
    #[must_use]
    pub const fn is_out_of_stock(&self) -> bool {
        self.uses >= self.max_uses
    }

    /// Returns cost A after applying discounts and demand multipliers.
    #[must_use]
    pub fn get_cost_a(&self) -> ItemStack {
        if self.base_cost_a.is_empty() {
            return ItemStack::empty();
        }

        let base_count = self.base_cost_a.count() as i32;
        let demand_adjustment =
            ((self.demand as f32) * self.price_multiplier * (base_count as f32)).round() as i32;
        let adjusted_count = (base_count + self.special_price_diff + demand_adjustment)
            .clamp(1, self.base_cost_a.max_stack_size() as i32);

        ItemStack::with_count(self.base_cost_a.item(), adjusted_count)
    }

    /// Returns cost B (unmodified by discounts/demand in vanilla).
    #[must_use]
    pub fn get_cost_b(&self) -> Option<ItemStack> {
        self.cost_b.clone()
    }

    /// Increments trade uses by 1 and updates demand.
    pub fn increment_uses(&mut self) {
        self.uses += 1;
        self.demand += 1;
    }

    /// Resets uses (used when restocking).
    pub fn restock(&mut self) {
        self.uses = 0;
    }

    /// Converts this offer into a clientbound `MerchantOffer` packet representation.
    #[must_use]
    pub fn to_packet_offer(&self) -> MerchantOffer {
        MerchantOffer {
            base_cost_a: self.get_cost_a(),
            result: self.result.clone(),
            cost_b: self.get_cost_b(),
            out_of_stock: self.is_out_of_stock(),
            uses: self.uses,
            max_uses: self.max_uses,
            xp: self.xp,
            special_price_diff: self.special_price_diff,
            price_multiplier: self.price_multiplier,
            demand: self.demand,
        }
    }

    /// Serializes this trade offer into NBT compound format.
    pub fn save_nbt(&self, compound: &mut NbtCompound) {
        compound.insert("buy", self.base_cost_a.to_nbt_tag_ref());
        if let Some(ref cost_b) = self.cost_b {
            compound.insert("buyB", cost_b.to_nbt_tag_ref());
        }
        compound.insert("sell", self.result.to_nbt_tag_ref());
        compound.insert("uses", self.uses);
        compound.insert("maxUses", self.max_uses);
        compound.insert("xp", self.xp);
        compound.insert("specialPrice", self.special_price_diff);
        compound.insert("priceMultiplier", self.price_multiplier);
        compound.insert("demand", self.demand);
    }

    /// Deserializes a trade offer from NBT compound format.
    pub fn load_nbt(compound: BorrowedNbtCompound<'_, '_>) -> Option<Self> {
        let buy = compound
            .compound("buy")
            .and_then(|c| ItemStack::from_borrowed_compound(&c))?;
        let buy_b = compound
            .compound("buyB")
            .and_then(|c| ItemStack::from_borrowed_compound(&c));
        let sell = compound
            .compound("sell")
            .and_then(|c| ItemStack::from_borrowed_compound(&c))?;
        let uses = compound.int("uses").unwrap_or(0);
        let max_uses = compound.int("maxUses").unwrap_or(12);
        let xp = compound.int("xp").unwrap_or(1);
        let special_price = compound.int("specialPrice").unwrap_or(0);
        let price_multiplier = compound.float("priceMultiplier").unwrap_or(0.05);
        let demand = compound.int("demand").unwrap_or(0);

        Some(Self {
            base_cost_a: buy,
            cost_b: buy_b,
            result: sell,
            uses,
            max_uses,
            xp,
            special_price_diff: special_price,
            price_multiplier,
            demand,
        })
    }
}

/// Generates random trades for a profession at a given level (1 through 5).
#[must_use]
pub fn generate_trades_for_level(profession: &Identifier, level: i32) -> Vec<TradeOffer> {
    let mut pool = Vec::new();
    let em = || ItemStack::new(&vanilla_items::EMERALD);
    let ems = |count| ItemStack::with_count(&vanilla_items::EMERALD, count);

    match profession.path.as_ref() {
        "armorer" => match level {
            1 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::COAL, 15), None, em(), 16, 2, 0.05));
                pool.push(TradeOffer::new(ems(5), None, ItemStack::new(&vanilla_items::IRON_HELMET), 12, 1, 0.2));
                pool.push(TradeOffer::new(ems(9), None, ItemStack::new(&vanilla_items::IRON_CHESTPLATE), 12, 1, 0.2));
            }
            2 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::IRON_INGOT, 4), None, em(), 12, 10, 0.05));
                pool.push(TradeOffer::new(ems(36), None, ItemStack::new(&vanilla_items::BELL), 12, 5, 0.2));
            }
            3 => {
                pool.push(TradeOffer::new(ItemStack::new(&vanilla_items::LAVA_BUCKET), None, em(), 12, 20, 0.05));
                pool.push(TradeOffer::new(ems(5), None, ItemStack::new(&vanilla_items::CHAINMAIL_LEGGINGS), 12, 10, 0.2));
            }
            4 => {
                pool.push(TradeOffer::new(ems(17), None, ItemStack::new(&vanilla_items::DIAMOND_LEGGINGS), 12, 15, 0.2));
                pool.push(TradeOffer::new(ems(13), None, ItemStack::new(&vanilla_items::DIAMOND_BOOTS), 12, 15, 0.2));
            }
            5 => {
                pool.push(TradeOffer::new(ems(31), None, ItemStack::new(&vanilla_items::DIAMOND_CHESTPLATE), 12, 30, 0.2));
                pool.push(TradeOffer::new(ems(19), None, ItemStack::new(&vanilla_items::DIAMOND_HELMET), 12, 30, 0.2));
            }
            _ => {}
        },
        "farmer" => match level {
            1 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::WHEAT, 20), None, em(), 16, 2, 0.05));
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::POTATO, 26), None, em(), 16, 2, 0.05));
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::CARROT, 22), None, em(), 16, 2, 0.05));
                pool.push(TradeOffer::new(em(), None, ItemStack::with_count(&vanilla_items::BREAD, 6), 16, 1, 0.05));
            }
            2 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::PUMPKIN, 6), None, em(), 12, 10, 0.05));
                pool.push(TradeOffer::new(em(), None, ItemStack::with_count(&vanilla_items::PUMPKIN_PIE, 4), 12, 5, 0.05));
            }
            3 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::MELON, 4), None, em(), 12, 20, 0.05));
                pool.push(TradeOffer::new(em(), None, ItemStack::with_count(&vanilla_items::COOKIE, 18), 12, 10, 0.05));
            }
            4 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::BEETROOT, 15), None, em(), 12, 30, 0.05));
                pool.push(TradeOffer::new(em(), None, ItemStack::new(&vanilla_items::CAKE), 12, 15, 0.05));
            }
            5 => {
                pool.push(TradeOffer::new(ems(3), None, ItemStack::with_count(&vanilla_items::GOLDEN_CARROT, 3), 12, 30, 0.05));
            }
            _ => {}
        },
        "librarian" => match level {
            1 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::PAPER, 24), None, em(), 16, 2, 0.05));
                pool.push(TradeOffer::new(ems(9), None, ItemStack::new(&vanilla_items::BOOKSHELF), 12, 1, 0.05));
            }
            2 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::BOOK, 4), None, em(), 12, 10, 0.05));
                pool.push(TradeOffer::new(em(), None, ItemStack::new(&vanilla_items::LANTERN), 12, 5, 0.05));
            }
            3 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::INK_SAC, 5), None, em(), 12, 20, 0.05));
                pool.push(TradeOffer::new(em(), None, ItemStack::with_count(&vanilla_items::GLASS, 4), 12, 10, 0.05));
            }
            4 => {
                pool.push(TradeOffer::new(ems(5), None, ItemStack::new(&vanilla_items::CLOCK), 12, 15, 0.05));
                pool.push(TradeOffer::new(ems(4), None, ItemStack::new(&vanilla_items::COMPASS), 12, 15, 0.05));
            }
            5 => {
                pool.push(TradeOffer::new(ems(20), None, ItemStack::new(&vanilla_items::NAME_TAG), 12, 30, 0.05));
            }
            _ => {}
        },
        "weaponsmith" => match level {
            1 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::COAL, 15), None, em(), 16, 2, 0.05));
                pool.push(TradeOffer::new(ems(3), None, ItemStack::new(&vanilla_items::IRON_SWORD), 12, 1, 0.2));
            }
            2 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::IRON_INGOT, 4), None, em(), 12, 10, 0.05));
                pool.push(TradeOffer::new(ems(36), None, ItemStack::new(&vanilla_items::BELL), 12, 5, 0.2));
            }
            3 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::FLINT, 24), None, em(), 12, 20, 0.05));
            }
            4 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::DIAMOND, 1), None, em(), 12, 30, 0.05));
            }
            5 => {
                pool.push(TradeOffer::new(ems(21), None, ItemStack::new(&vanilla_items::DIAMOND_SWORD), 12, 30, 0.2));
            }
            _ => {}
        },
        _ => match level {
            1 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::COAL, 15), None, em(), 16, 2, 0.05));
                pool.push(TradeOffer::new(em(), None, ItemStack::with_count(&vanilla_items::BREAD, 4), 16, 1, 0.05));
            }
            2 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::IRON_INGOT, 4), None, em(), 12, 10, 0.05));
            }
            3 => {
                pool.push(TradeOffer::new(ItemStack::with_count(&vanilla_items::FLINT, 24), None, em(), 12, 20, 0.05));
            }
            4 => {
                pool.push(TradeOffer::new(ems(5), None, ItemStack::new(&vanilla_items::CLOCK), 12, 15, 0.05));
            }
            5 => {
                pool.push(TradeOffer::new(ems(20), None, ItemStack::new(&vanilla_items::NAME_TAG), 12, 30, 0.05));
            }
            _ => {}
        },
    }

    // Pick up to 2 offers from the pool
    if pool.len() <= 2 {
        pool
    } else {
        use rand::seq::SliceRandom;
        let mut rng = rand::rng();
        pool.shuffle(&mut rng);
        pool.truncate(2);
        pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trade_offer_cost_a_discount_and_demand() {
        let mut offer = TradeOffer::new(
            ItemStack::with_count(&vanilla_items::EMERALD, 10),
            None,
            ItemStack::new(&vanilla_items::BREAD),
            12,
            2,
            0.05,
        );

        assert_eq!(offer.get_cost_a().count(), 10);

        offer.special_price_diff = -3;
        assert_eq!(offer.get_cost_a().count(), 7);

        offer.demand = 5;
        // 10 - 3 + (5 * 0.05 * 10).round() = 7 + 3 = 10
        assert_eq!(offer.get_cost_a().count(), 10);
    }
}
