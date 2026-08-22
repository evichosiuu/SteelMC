use steel_macros::{ReadFrom, ServerPacket};

/// Serverbound packet sent when a player modifies a command block minecart's command.
///
/// Packet ID maps to `set_command_minecart` in `packets.json`.
#[derive(ReadFrom, ServerPacket, Clone, Debug)]
pub struct SSetCommandMinecart {
    /// Entity ID of the minecart.
    #[read(as = VarInt)]
    pub entity_id: i32,
    /// The command text.
    #[read(as = Prefixed(VarInt))]
    pub command: String,
    /// Whether output tracking is enabled.
    pub track_output: bool,
}
