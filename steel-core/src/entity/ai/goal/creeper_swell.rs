//! Vanilla Creeper swell AI goal.

use steel_utils::Downcast;

use crate::entity::ai::goal::selector::{Goal, GoalControls};
use crate::entity::entities::CreeperEntity;
use crate::entity::{Entity, LivingEntity, Mob, PathfinderMob};

/// Creeper goal that approaches the target and swells/fuses when close enough.
pub struct CreeperSwellGoal {
    target_distance_sq: f64,
}

impl CreeperSwellGoal {
    /// Creates a new `CreeperSwellGoal`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            target_distance_sq: 9.0, // 3.0 blocks squared
        }
    }
}

impl Default for CreeperSwellGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl Goal for CreeperSwellGoal {
    fn controls(&self) -> GoalControls {
        GoalControls::MOVE
    }

    fn can_use(&mut self, mob: &dyn PathfinderMob) -> bool {
        let Some(creeper) = mob.downcast_ref::<CreeperEntity>() else {
            return false;
        };

        let target = creeper.target();
        creeper.swell_dir() > 0
            || target.is_some_and(|t| {
                creeper.position().distance_squared(t.position()) < self.target_distance_sq
            })
    }

    fn start(&mut self, mob: &dyn PathfinderMob) {
        mob.mob_base().navigation().lock().stop();
    }

    fn stop(&mut self, mob: &dyn PathfinderMob) {
        if let Some(creeper) = mob.downcast_ref::<CreeperEntity>() {
            creeper.set_swell_dir(-1);
        }
    }

    fn tick(&mut self, mob: &dyn PathfinderMob) {
        let Some(creeper) = mob.downcast_ref::<CreeperEntity>() else {
            return;
        };

        let Some(target) = creeper.target() else {
            creeper.set_swell_dir(-1);
            return;
        };

        if creeper.position().distance_squared(target.position()) > 49.0 {
            // Target too far
            creeper.set_swell_dir(-1);
        } else if !creeper.has_line_of_sight(target.as_ref()) {
            // Target lost line of sight
            creeper.set_swell_dir(-1);
        } else {
            // Target within range and in line of sight
            creeper.set_swell_dir(1);
        }
    }
}
