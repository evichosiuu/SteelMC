//! Furnace, Blast Furnace, and Smoker block entity implementation.

use std::{
    mem,
    sync::{Arc, Weak},
};

use simdnbt::ToNbtTag;
use simdnbt::borrow::{BaseNbtCompound as BorrowedNbtCompound, NbtCompound as NbtCompoundView};
use simdnbt::owned::{NbtCompound, NbtList, NbtTag};
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey, locks::SyncMutex};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::inventory::container::Container;
use crate::inventory::lock::{ContainerRef, SharedContainer};
use crate::world::World;

/// Number of slots in a furnace (3 slots: 0 = input, 1 = fuel, 2 = result).
pub const FURNACE_SLOTS: usize = 3;

/// Furnace / Blast Furnace / Smoker block entity.
pub struct FurnaceBlockEntity {
    base: Arc<BlockEntityBase>,
    container: Arc<SyncMutex<FurnaceContainer>>,
    container_ref: ContainerRef,
}

struct FurnaceContainer {
    items: Vec<ItemStack>,
}

// SAFETY: This key is owned by Steel and uniquely identifies `FurnaceBlockEntity`.
unsafe impl DowncastType for FurnaceBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/furnace");
}

// SAFETY: This key is owned by Steel and uniquely identifies `FurnaceContainer`.
unsafe impl DowncastType for FurnaceContainer {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:container/furnace");
}

impl FurnaceBlockEntity {
    /// Creates a new furnace block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::with_type(level, &vanilla_block_entity_types::FURNACE, pos, state)
    }

    /// Creates a new blast furnace block entity.
    #[must_use]
    pub fn new_blast(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::with_type(
            level,
            &vanilla_block_entity_types::BLAST_FURNACE,
            pos,
            state,
        )
    }

    /// Creates a new smoker block entity.
    #[must_use]
    pub fn new_smoker(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        Self::with_type(level, &vanilla_block_entity_types::SMOKER, pos, state)
    }

    /// Creates a new furnace block entity with a specific block entity type.
    #[must_use]
    pub fn with_type(
        level: Weak<World>,
        entity_type: BlockEntityTypeRef,
        pos: BlockPos,
        state: BlockStateId,
    ) -> Self {
        let base = Arc::new(BlockEntityBase::new(entity_type, level, pos, state));
        let container = Arc::new(SyncMutex::new(FurnaceContainer {
            items: vec![ItemStack::empty(); FURNACE_SLOTS],
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

impl BlockEntity for FurnaceBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn pre_remove_side_effects(&self, pos: BlockPos, _state: BlockStateId) {
        let items = {
            let mut container = self.container.lock();
            mem::replace(&mut container.items, vec![ItemStack::empty(); FURNACE_SLOTS])
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
                    if slot < FURNACE_SLOTS {
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

impl Container for FurnaceContainer {
    fn items(&self) -> &[ItemStack] {
        &self.items
    }

    fn items_mut(&mut self) -> &mut [ItemStack] {
        &mut self.items
    }

    fn get_container_size(&self) -> usize {
        FURNACE_SLOTS
    }

    fn set_item(&mut self, slot: usize, mut stack: ItemStack) {
        if slot < FURNACE_SLOTS {
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
