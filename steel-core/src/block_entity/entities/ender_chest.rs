//! Ender Chest block entity implementation.

use std::sync::{
    Arc, Weak,
    atomic::{AtomicU32, Ordering},
};

use simdnbt::borrow::BaseNbtCompound as BorrowedNbtCompound;
use simdnbt::owned::NbtCompound;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, DowncastType, DowncastTypeKey};

use crate::block_entity::{BlockEntity, BlockEntityBase};
use crate::player::Player;
use crate::world::World;

/// Ender chest block entity.
pub struct EnderChestBlockEntity {
    base: Arc<BlockEntityBase>,
    open_count: AtomicU32,
}

// SAFETY: This key is owned by Steel and uniquely identifies `EnderChestBlockEntity`.
unsafe impl DowncastType for EnderChestBlockEntity {
    const TYPE_KEY: DowncastTypeKey = DowncastTypeKey::new("steel:block_entity/ender_chest");
}

impl EnderChestBlockEntity {
    /// Creates a new ender chest block entity.
    #[must_use]
    pub fn new(level: Weak<World>, pos: BlockPos, state: BlockStateId) -> Self {
        let base = Arc::new(BlockEntityBase::new(
            &vanilla_block_entity_types::ENDER_CHEST,
            level,
            pos,
            state,
        ));
        Self {
            base,
            open_count: AtomicU32::new(0),
        }
    }

    /// Returns a cloned Arc pointer to the block entity base.
    #[must_use]
    pub fn base_arc(&self) -> Arc<BlockEntityBase> {
        Arc::clone(&self.base)
    }
}

impl BlockEntity for EnderChestBlockEntity {
    fn base(&self) -> &BlockEntityBase {
        &self.base
    }

    fn load_additional(&self, _nbt: &BorrowedNbtCompound<'_>) {}

    fn save_additional(&self, _nbt: &mut NbtCompound) {}

    fn start_open(&self, _player: &Player) {
        let count = self.open_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count == 1 {
            if let Some(world) = self.get_level() {
                world.block_event(
                    self.get_block_pos(),
                    self.get_block_state().get_block(),
                    1,
                    1,
                );
            }
        }
    }

    fn stop_open(&self, _player: &Player) {
        let prev = self.open_count.fetch_sub(1, Ordering::Relaxed);
        let count = prev.saturating_sub(1);
        if count == 0 {
            if let Some(world) = self.get_level() {
                world.block_event(
                    self.get_block_pos(),
                    self.get_block_state().get_block(),
                    1,
                    0,
                );
            }
        }
    }
}
