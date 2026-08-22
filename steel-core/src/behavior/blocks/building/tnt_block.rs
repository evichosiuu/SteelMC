//! Vanilla TNT block behavior.

use std::sync::Arc;

use glam::DVec3;
use steel_macros::block_behavior;
use steel_protocol::packets::game::SoundSource;
use steel_registry::blocks::BlockRef;
use steel_registry::{sound_events, vanilla_blocks, vanilla_items};
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::{
    BlockBehavior, BlockHitResult, BlockPlaceContext, InteractionResult, InventoryAccess,
};
use crate::entity::entities::PrimedTntEntity;
use crate::entity::projectile::Projectile;
use crate::entity::Entity;
use crate::player::Player;
use crate::world::{ClipHitResult, SignalGetter as _, World};

/// Vanilla `TntBlock` behavior.
#[block_behavior(class = "TntBlock")]
pub struct TntBlock {
    block: BlockRef,
}

impl TntBlock {
    /// Creates TNT block behavior for `block`.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Primes the TNT block, removing it and spawning a Primed TNT entity.
    pub fn prime_tnt(world: &Arc<World>, pos: BlockPos, igniter_id: Option<i32>) {
        world.set_block(
            pos,
            vanilla_blocks::AIR.default_state(),
            UpdateFlags::UPDATE_ALL,
        );

        let spawn_pos = pos.0.as_dvec3() + DVec3::new(0.5, 0.0, 0.5);
        let _ = PrimedTntEntity::spawn(world, spawn_pos, 80, igniter_id);

        world.play_sound(
            &sound_events::ENTITY_TNT_PRIMED,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        );
    }

    fn check_redstone_activation(world: &Arc<World>, pos: BlockPos) {
        if world.has_neighbor_signal(pos) {
            Self::prime_tnt(world, pos, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use steel_registry::init_vanilla_registry;
    use steel_registry::vanilla_blocks;

    #[test]
    fn tnt_block_constructor() {
        init_vanilla_registry();
        let block = TntBlock::new(&vanilla_blocks::TNT);
        assert_eq!(block.block.key, vanilla_blocks::TNT.key);
    }
}

impl BlockBehavior for TntBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        Self::check_redstone_activation(world, pos);
    }

    fn handle_neighbor_changed(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        Self::check_redstone_activation(world, pos);
    }

    fn use_item_on(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let is_flint_and_steel = inv.with_item(|item| item.is(&vanilla_items::FLINT_AND_STEEL));
        let is_fire_charge = inv.with_item(|item| item.is(&vanilla_items::FIRE_CHARGE));

        if is_flint_and_steel {
            inv.with_item(|item| {
                item.hurt_and_break(1, player.has_infinite_materials());
            });
            Self::prime_tnt(world, pos, Some(player.id()));
            InteractionResult::Success
        } else if is_fire_charge {
            if !player.has_infinite_materials() {
                inv.with_item(|item| {
                    item.shrink(1);
                });
            }
            Self::prime_tnt(world, pos, Some(player.id()));
            InteractionResult::Success
        } else {
            InteractionResult::Pass
        }
    }

    fn on_projectile_hit(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        hit: &ClipHitResult,
        projectile: &dyn Projectile,
    ) {
        if projectile.is_on_fire() {
            let igniter_id = projectile.get_owner().map(|owner| owner.id());
            Self::prime_tnt(world, hit.block_pos, igniter_id);
        }
    }
}
