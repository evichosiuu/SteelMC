//! Hostile entity implementations.
//!
//! Those mobs are aggressive creatures that attack players on sight.

pub mod creeper;
pub mod skeleton;
pub mod zombie;

#[cfg(test)]
mod tests;

pub use creeper::CreeperEntity;
pub use skeleton::SkeletonEntity;
pub use zombie::ZombieEntity;
