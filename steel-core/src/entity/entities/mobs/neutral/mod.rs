//! Neutral entity implementations.
//!
//! Those mobs are neutral to players, but will attack if provoked.

pub mod enderman;

#[cfg(test)]
mod tests;

pub use enderman::EnderManEntity;
