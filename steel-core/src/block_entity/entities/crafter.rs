//! Crafter block entity implementation.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Number of slots in a crafter (3x3 grid = 9 slots).
pub const CRAFTER_SLOTS: usize = 9;

/// Crafter block entity.
pub struct CrafterBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<CrafterContainer>>,
    container_ref: ContainerRef,
}

struct CrafterContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `CrafterBlockEntity`.
unsafe impl DowncastType for CrafterBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/crafter");
}

// SAFETY: This key is owned by Steel and uniquely identifies `CrafterContainer`.
unsafe impl DowncastType for CrafterContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/crafter");
}

impl CrafterBlockEntity {
    /// Creates a new crafter block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::CRAFTER,
            level,
            pos,
            state,
        ));
        let container = Arc::new(SyncMutex::new(CrafterContainer {
            items: vec![ItemStack::empty(); CRAFTER_SLOTS],
        }));
        let shared_container: SharedContainer = container.clone();
        Self {
            container_ref: ContainerRef::owned_by_block_entity(
                shared_container,
                Arc::clone(&base),
            ),
            base,
            container,
        }
    }
}

impl BlockEntity for CrafterBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items, vec![ItemStack::empty(); CRAFTER_SLOTS])
        };
        let Some(world) = self.get_level() else {
            return;
        };
        for item in items {
            world.drop_item_stack(pos, item);
        }
    }

    fn load_additional(&self, nbt: &BorrowedNbtCompound<'_>) {
        let nbt_view: NbtCompoundView<'_, '_> = nbt.into();
        let mut container = self.container.lock();
        container.items.fill(ItemStack::empty());

        if let Some(items_list) = nbt_view.list("Items")
            && let Some(compounds) = items_list.compounds()
        {
            for compound in compounds {
                if let Some(slot) = compound.byte("Slot") {
                    let slot = slot as usize;
                    if slot < CRAFTER_SLOTS {
                        if let Some(item) = ItemStack::from_borrowed_compound(&compound) {
                            container.items[slot] = item;
                        }
                    }
                }
            }
        }
    }

    fn save_additional(&self, nbt: &mut NbtCompound) {
        let container = self.container.lock();
        let mut items: Vec<NbtCompound> = Vec::new();
        for (slot, item) in container.items.iter().enumerate() {
            if !item.is_empty() {
                if let NbtTag::Compound(mut item_nbt) = item.clone().to_nbt_tag() {
                    item_nbt.insert("Slot", slot as i8);
                    items.push(item_nbt);
                }
            }
        }
        nbt.insert("Items", NbtList::Compound(items));
    }

    fn container_ref(&self) -> Option<ContainerRef> {
        Some(self.container_ref.clone())
    }
}

impl Container for CrafterContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        CRAFTER_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < CRAFTER_SLOTS {
            let max_stack_size = self.get_max_stack_size_for_item(&stack);
            if !stack.is_empty() && stack.count() > max_stack_size {
                stack.set_count(max_stack_size);
            }
            self.items[slot] = stack;
        }
    }

    fn get_max_stack_size(&self) -> i32 {
        64
    }

    fn set_changed(&mut self) {}
}
