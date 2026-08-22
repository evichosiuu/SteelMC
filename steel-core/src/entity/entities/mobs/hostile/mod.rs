//! Hostile entity implementations.
//!
//! Those mobs are aggressive creatures that attack players on sight.

pub mod endermite;
pub mod skeleton;
pub mod zombie;

#[cfg(test)]
mod tests;

pub use endermite::EndermiteEntity;
pub use skeleton::SkeletonEntity;
pub use zombie::ZombieEntity;
