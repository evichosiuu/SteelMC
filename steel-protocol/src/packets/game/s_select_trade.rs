use std::io::Cursor;

use steel_macros::ServerPacket;
use steel_utils::{
    codec::VarInt,
    serial::{ReadFrom, WriteTo},
};

#[derive(ServerPacket, Clone, Debug, PartialEq)]
#[packet_id(Play = S_SELECT_TRADE)]
pub struct SSelectTrade {
    pub selected_slot: i32,
}

impl ReadFrom for SSelectTrade {
    fn read(data: &mut Cursor<&[u8]>) -> std::io::Result<Self> {
        Ok(Self {
            selected_slot: VarInt::read(data)?.0,
        })
    }
}

impl WriteTo for SSelectTrade {
    fn write(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
        VarInt(self.selected_slot).write(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_trade_roundtrip() {
        let packet = SSelectTrade { selected_slot: 3 };
        let mut buf = Vec::new();
        packet.write(&mut buf).unwrap();
        let decoded = SSelectTrade::read(&mut Cursor::new(buf.as_slice())).unwrap();
        assert_eq!(packet, decoded);
    }
}
