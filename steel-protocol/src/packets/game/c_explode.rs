use std::io::{Result, Write};

use glam::DVec3;
use steel_macros::ClientPacket;
use steel_registry::packets::play::C_EXPLODE;
use steel_registry::particle_type::ParticleData;
use steel_registry::sound_event::{SoundEventHolder, SoundEventRef};
use steel_utils::codec::LpVec3;
use steel_utils::serial::WriteTo;

/// Sent when an explosion occurs to trigger visual effects, sounds, and optional player knockback.
#[derive(ClientPacket, Clone, Debug)]
#[packet_id(Play = C_EXPLODE)]
pub struct CExplode {
    /// The explosion center position.
    pub center: DVec3,
    /// Optional knockback velocity applied to the recipient player.
    pub knockback: Option<DVec3>,
    /// The particle effect spawned at the explosion position.
    pub particle: ParticleData,
    /// The sound event holder (`Holder<SoundEvent>`) played for the explosion.
    pub sound: SoundEventHolder,
}

impl WriteTo for CExplode {
    fn write(&self, writer: &mut impl Write) -> Result<()> {
        self.center.write(writer)?;
        self.knockback.map(LpVec3).write(writer)?;
        self.particle.write(writer)?;
        self.sound.write(writer)
    }
}

impl CExplode {
    /// Creates a new explosion packet with a registry sound event reference.
    #[must_use]
    pub fn new(
        center: DVec3,
        knockback: Option<DVec3>,
        particle: ParticleData,
        sound: SoundEventRef,
    ) -> Self {
        Self {
            center,
            knockback,
            particle,
            sound: SoundEventHolder::registry(sound),
        }
    }

    /// Creates a new explosion packet with a sound event holder.
    #[must_use]
    pub fn with_sound_holder(
        center: DVec3,
        knockback: Option<DVec3>,
        particle: ParticleData,
        sound: SoundEventHolder,
    ) -> Self {
        Self {
            center,
            knockback,
            particle,
            sound,
        }
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use steel_registry::{
        init_vanilla_registry, particle_type::ParticleData, sound_events, vanilla_particle_types,
    };
    use steel_utils::serial::WriteTo;

    use super::CExplode;

    #[test]
    fn explode_packet_encodes_correctly() {
        init_vanilla_registry();

        let center = DVec3::new(10.5, 64.0, -15.5);
        let knockback = Some(DVec3::new(0.1, 0.4, -0.2));
        let particle = ParticleData::simple(&vanilla_particle_types::EXPLOSION);
        let sound = &sound_events::ENTITY_GENERIC_EXPLODE;

        let packet = CExplode::new(center, knockback, particle, sound);

        let mut buf = Vec::new();
        packet.write(&mut buf).unwrap();

        assert!(!buf.is_empty());
    }

    #[test]
    fn explode_packet_encodes_knockback_as_optional_lpvec3() {
        init_vanilla_registry();

        let center = DVec3::new(0.0, 64.0, 0.0);
        let particle = ParticleData::simple(&vanilla_particle_types::EXPLOSION);
        let sound = &sound_events::ENTITY_GENERIC_EXPLODE;

        let packet_none = CExplode::new(center, None, particle.clone(), sound);
        let mut buf_none = Vec::new();
        packet_none.write(&mut buf_none).unwrap();

        let packet_some = CExplode::new(center, Some(DVec3::new(0.1, 0.4, -0.2)), particle, sound);
        let mut buf_some = Vec::new();
        packet_some.write(&mut buf_some).unwrap();

        // Position `center` is 3 * 8 = 24 bytes.
        // `None` knockback writes 1 byte (`0x00`).
        // `Some` knockback writes 1 byte (`0x01`) plus 6 bytes for the non-zero `LpVec3`.
        assert_eq!(buf_none[24], 0x00);
        assert_eq!(buf_some[24], 0x01);
        assert_eq!(buf_some.len(), buf_none.len() + 6);
    }
}
