use std::io::{Cursor, Result, Write};

use steel_macros::ClientPacket;
use steel_registry::{
    data_component_predicate::DataComponentMatchers,
    item_stack::ItemStack,
    items::ItemRef,
    packets::play::C_MERCHANT_OFFERS,
    REGISTRY, RegistryEntry, RegistryExt,
};
use steel_utils::{
    codec::VarInt,
    serial::{ReadFrom, WriteTo},
};

#[derive(Clone, Debug, PartialEq)]
pub struct ItemCost {
    pub item: ItemRef,
    pub count: i32,
    pub components: DataComponentMatchers,
}

impl ItemCost {
    #[must_use]
    pub fn new(item: ItemRef, count: i32, components: DataComponentMatchers) -> Self {
        Self {
            item,
            count,
            components,
        }
    }

    #[must_use]
    pub fn from_item_stack(stack: &ItemStack) -> Self {
        Self {
            item: stack.item(),
            count: stack.count(),
            components: DataComponentMatchers::ANY,
        }
    }
}

impl WriteTo for ItemCost {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.item.id() as i32).write(writer)?;
        VarInt(self.count).write(writer)?;
        self.components.write(writer)?;
        Ok(())
    }
}

impl ReadFrom for ItemCost {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let item_id = VarInt::read(data)?.0;
        let item_id = usize::try_from(item_id)
            .map_err(|_| std::io::Error::other(format!("Invalid item id: {item_id}")))?;
        let item = REGISTRY
            .items
            .by_id(item_id)
            .ok_or_else(|| std::io::Error::other(format!("Unknown item id: {item_id}")))?;
        let count = VarInt::read(data)?.0;
        let components = DataComponentMatchers::read(data)?;

        Ok(Self {
            item,
            count,
            components,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MerchantOffer {
    pub base_cost_a: ItemCost,
    pub result: ItemStack,
    pub cost_b: Option<ItemCost>,
    pub out_of_stock: bool,
    pub uses: i32,
    pub max_uses: i32,
    pub xp: i32,
    pub special_price_diff: i32,
    pub price_multiplier: f32,
    pub demand: i32,
}

impl WriteTo for MerchantOffer {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.base_cost_a.write(writer)?;
        self.result.write(writer)?;
        if let Some(ref cost_b) = self.cost_b {
            true.write(writer)?;
            cost_b.write(writer)?;
        } else {
            false.write(writer)?;
        }
        self.out_of_stock.write(writer)?;
        VarInt(self.uses).write(writer)?;
        VarInt(self.max_uses).write(writer)?;
        VarInt(self.xp).write(writer)?;
        VarInt(self.special_price_diff).write(writer)?;
        self.price_multiplier.write(writer)?;
        VarInt(self.demand).write(writer)?;
        Ok(())
    }
}

impl ReadFrom for MerchantOffer {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let base_cost_a = ItemCost::read(data)?;
        let result = ItemStack::read_untrusted(data)?;
        let has_cost_b = bool::read(data)?;
        let cost_b = if has_cost_b {
            Some(ItemCost::read(data)?)
        } else {
            None
        };
        let out_of_stock = bool::read(data)?;
        let uses = VarInt::read(data)?.0;
        let max_uses = VarInt::read(data)?.0;
        let xp = VarInt::read(data)?.0;
        let special_price_diff = VarInt::read(data)?.0;
        let price_multiplier = f32::read(data)?;
        let demand = VarInt::read(data)?.0;

        Ok(Self {
            base_cost_a,
            result,
            cost_b,
            out_of_stock,
            uses,
            max_uses,
            xp,
            special_price_diff,
            price_multiplier,
            demand,
        })
    }
}

#[derive(ClientPacket, Clone, Debug, PartialEq)]
#[packet_id(Play = C_MERCHANT_OFFERS)]
pub struct CMerchantOffers {
    pub container_id: i32,
    pub offers: Vec<MerchantOffer>,
    pub villager_level: i32,
    pub experience: i32,
    pub is_regular_villager: bool,
    pub can_restock: bool,
}

impl WriteTo for CMerchantOffers {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(self.container_id).write(writer)?;
        VarInt(self.offers.len() as i32).write(writer)?;
        for offer in &self.offers {
            offer.write(writer)?;
        }
        VarInt(self.villager_level).write(writer)?;
        VarInt(self.experience).write(writer)?;
        self.is_regular_villager.write(writer)?;
        self.can_restock.write(writer)?;
        Ok(())
    }
}

impl ReadFrom for CMerchantOffers {
    fn read(data: &mut Cursor<&[u8]>) -> Result<Self> {
        let container_id = VarInt::read(data)?.0;
        let count = VarInt::read(data)?.0 as usize;
        let mut offers = Vec::with_capacity(count);
        for _ in 0..count {
            offers.push(MerchantOffer::read(data)?);
        }
        let villager_level = VarInt::read(data)?.0;
        let experience = VarInt::read(data)?.0;
        let is_regular_villager = bool::read(data)?;
        let can_restock = bool::read(data)?;

        Ok(Self {
            container_id,
            offers,
            villager_level,
            experience,
            is_regular_villager,
            can_restock,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::{init_vanilla_registry, vanilla_items};

    #[test]
    fn test_merchant_offer_roundtrip() {
        init_vanilla_registry();
        let offer = MerchantOffer {
            base_cost_a: ItemCost {
                item: &vanilla_items::EMERALD,
                count: 1,
                components: DataComponentMatchers::ANY,
            },
            result: ItemStack::empty(),
            cost_b: None,
            out_of_stock: false,
            uses: 2,
            max_uses: 12,
            xp: 5,
            special_price_diff: -1,
            price_multiplier: 0.05,
            demand: 1,
        };

        let mut buf = Vec::new();
        offer.write(&mut buf).unwrap();
        let decoded = MerchantOffer::read(&mut Cursor::new(buf.as_slice())).unwrap();
        assert_eq!(offer, decoded);
    }

    #[test]
    fn test_c_merchant_offers_roundtrip() {
        init_vanilla_registry();
        let packet = CMerchantOffers {
            container_id: 1,
            offers: vec![MerchantOffer {
                base_cost_a: ItemCost {
                    item: &vanilla_items::EMERALD,
                    count: 1,
                    components: DataComponentMatchers::ANY,
                },
                result: ItemStack::empty(),
                cost_b: None,
                out_of_stock: false,
                uses: 0,
                max_uses: 12,
                xp: 2,
                special_price_diff: 0,
                price_multiplier: 0.05,
                demand: 0,
            }],
            villager_level: 2,
            experience: 10,
            is_regular_villager: true,
            can_restock: true,
        };

        let mut buf = Vec::new();
        packet.write(&mut buf).unwrap();
        let decoded = CMerchantOffers::read(&mut Cursor::new(buf.as_slice())).unwrap();
        assert_eq!(packet, decoded);
    }
}
