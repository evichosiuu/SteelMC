//! Clientbound boss event (boss bar) packet.

use std::io::{Result, Write};
use steel_macros::ClientPacket;
use steel_registry::packets::play::C_BOSS_EVENT;
use steel_utils::{codec::VarInt, serial::WriteTo};
use text_components::TextComponent;
use uuid::Uuid;

/// Color of the boss bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum BossBarColor {
    Pink = 0,
    Blue = 1,
    Red = 2,
    Green = 3,
    Yellow = 4,
    Purple = 5,
    White = 6,
}

impl WriteTo for BossBarColor {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(*self as i32).write(writer)
    }
}

/// Overlay/style of the boss bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum BossBarOverlay {
    Progress = 0,
    Notches6 = 1,
    Notches10 = 2,
    Notches12 = 3,
    Notches20 = 4,
}

impl WriteTo for BossBarOverlay {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        VarInt(*self as i32).write(writer)
    }
}

/// Bitfield flags for boss bar visual/audio behavior.
pub mod boss_bar_flags {
    pub const DARKEN_SCREEN: u8 = 0x01;
    pub const PLAY_BOSS_MUSIC: u8 = 0x02;
    pub const CREATE_WORLD_FOG: u8 = 0x04;
}

/// Boss event action types.
#[derive(Clone, Debug)]
pub enum BossEventAction {
    /// Adds a new boss bar.
    Add {
        title: TextComponent,
        health: f32,
        color: BossBarColor,
        overlay: BossBarOverlay,
        flags: u8,
    },
    /// Removes a boss bar.
    Remove,
    /// Updates boss bar health (0.0 to 1.0).
    UpdateHealth { health: f32 },
    /// Updates boss bar title.
    UpdateTitle { title: TextComponent },
    /// Updates boss bar color and overlay.
    UpdateStyle {
        color: BossBarColor,
        overlay: BossBarOverlay,
    },
    /// Updates boss bar flags.
    UpdateFlags { flags: u8 },
}

/// Sent to add, remove, or update a boss health bar on the client.
#[derive(ClientPacket, Clone, Debug)]
#[packet_id(Play = C_BOSS_EVENT)]
pub struct CBossEvent {
    pub uuid: Uuid,
    pub action: BossEventAction,
}

impl CBossEvent {
    /// Creates an `Add` boss event.
    #[must_use]
    pub const fn add(
        uuid: Uuid,
        title: TextComponent,
        health: f32,
        color: BossBarColor,
        overlay: BossBarOverlay,
        flags: u8,
    ) -> Self {
        Self {
            uuid,
            action: BossEventAction::Add {
                title,
                health,
                color,
                overlay,
                flags,
            },
        }
    }

    /// Creates a `Remove` boss event.
    #[must_use]
    pub const fn remove(uuid: Uuid) -> Self {
        Self {
            uuid,
            action: BossEventAction::Remove,
        }
    }

    /// Creates an `UpdateHealth` boss event.
    #[must_use]
    pub const fn update_health(uuid: Uuid, health: f32) -> Self {
        Self {
            uuid,
            action: BossEventAction::UpdateHealth { health },
        }
    }

    /// Creates an `UpdateTitle` boss event.
    #[must_use]
    pub const fn update_title(uuid: Uuid, title: TextComponent) -> Self {
        Self {
            uuid,
            action: BossEventAction::UpdateTitle { title },
        }
    }

    /// Creates an `UpdateStyle` boss event.
    #[must_use]
    pub const fn update_style(uuid: Uuid, color: BossBarColor, overlay: BossBarOverlay) -> Self {
        Self {
            uuid,
            action: BossEventAction::UpdateStyle { color, overlay },
        }
    }

    /// Creates an `UpdateFlags` boss event.
    #[must_use]
    pub const fn update_flags(uuid: Uuid, flags: u8) -> Self {
        Self {
            uuid,
            action: BossEventAction::UpdateFlags { flags },
        }
    }
}

impl WriteTo for CBossEvent {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.uuid.write(writer)?;
        match &self.action {
            BossEventAction::Add {
                title,
                health,
                color,
                overlay,
                flags,
            } => {
                VarInt(0).write(writer)?;
                title.write(writer)?;
                health.write(writer)?;
                color.write(writer)?;
                overlay.write(writer)?;
                flags.write(writer)?;
            }
            BossEventAction::Remove => {
                VarInt(1).write(writer)?;
            }
            BossEventAction::UpdateHealth { health } => {
                VarInt(2).write(writer)?;
                health.write(writer)?;
            }
            BossEventAction::UpdateTitle { title } => {
                VarInt(3).write(writer)?;
                title.write(writer)?;
            }
            BossEventAction::UpdateStyle { color, overlay } => {
                VarInt(4).write(writer)?;
                color.write(writer)?;
                overlay.write(writer)?;
            }
            BossEventAction::UpdateFlags { flags } => {
                VarInt(5).write(writer)?;
                flags.write(writer)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boss_event_serializes_add_and_remove() {
        let uuid = Uuid::nil();
        let add_packet = CBossEvent::add(
            uuid,
            TextComponent::from("Ender Dragon"),
            1.0,
            BossBarColor::Pink,
            BossBarOverlay::Progress,
            0,
        );

        let mut buf = Vec::new();
        add_packet.write(&mut buf).unwrap();
        assert!(!buf.is_empty());

        let remove_packet = CBossEvent::remove(uuid);
        let mut remove_buf = Vec::new();
        remove_packet.write(&mut remove_buf).unwrap();
        assert_eq!(remove_buf.len(), 17); // 16 bytes UUID + 1 byte VarInt(1)
    }
}
