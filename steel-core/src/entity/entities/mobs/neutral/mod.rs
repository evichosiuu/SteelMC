//! Neutral entity implementations.
//!
//! Those mobs are neutral to players, but will attack if provoked.

mod bee;
mod camel;
mod cave_spider;
pub mod enderman;
mod goat;
mod iron_golem;
mod piglin;
mod polar_bear;
mod spider;
mod wolf;
mod zombified_piglin;

#[cfg(test)]
mod tests;

pub use bee::BeeEntity;
pub use camel::{CamelEntity, CamelHuskEntity};
pub use cave_spider::CaveSpiderEntity;
pub use enderman::EnderManEntity;
pub use goat::GoatEntity;
pub use iron_golem::IronGolemEntity;
pub use piglin::PiglinEntity;
pub use polar_bear::PolarBearEntity;
pub use spider::SpiderEntity;
pub use wolf::WolfEntity;
pub use zombified_piglin::ZombifiedPiglinEntity;
