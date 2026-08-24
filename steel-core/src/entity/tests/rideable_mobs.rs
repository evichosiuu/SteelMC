use std::io::Cursor;

use simdnbt::borrow::read_compound as read_borrowed_compound;

use super::*;
use crate::entity::entities::{
    CamelEntity, CamelHuskEntity, DonkeyEntity, HappyGhastEntity, HorseEntity, LlamaEntity,
    MuleEntity, SkeletonHorseEntity, StriderEntity, TraderLlamaEntity, ZombieHorseEntity,
};
use crate::inventory::equipment::EquipmentSlot;

fn roundtrip_nbt(nbt: &NbtCompound) -> simdnbt::borrow::NbtCompound<'static, 'static> {
    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let leaked_bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
    let borrowed = read_borrowed_compound(&mut Cursor::new(leaked_bytes)).expect("test nbt reborrow");
    let leaked_borrowed = Box::leak(Box::new(borrowed));
    simdnbt::borrow::NbtCompound::from(&*leaked_borrowed)
}

#[test]
fn horse_taming_feeding_and_equipment() {
    init_vanilla_registry();

    let horse = HorseEntity::new(&vanilla_entities::HORSE, 1, DVec3::ZERO, Weak::new());
    assert!(!horse.is_tamed());
    assert!(!horse.is_saddled());
    assert_eq!(horse.temper(), 0);

    // Feeding increases temper
    horse.modify_temper(10);
    assert_eq!(horse.temper(), 10);

    // Taming
    horse.set_tamed(true);
    let owner = Uuid::new_v4();
    horse.set_owner_uuid(Some(owner));
    assert!(horse.is_tamed());
    assert_eq!(horse.owner_uuid(), Some(owner));

    // Equipment slots
    assert!(horse.can_use_slot(EquipmentSlot::Saddle));
    assert!(horse.can_use_slot(EquipmentSlot::Body));

    horse.set_saddled(true);
    assert!(horse.is_saddled());

    // NBT saving & loading
    let mut nbt = NbtCompound::new();
    horse.save_additional(&mut nbt);

    let horse_loaded = HorseEntity::new(&vanilla_entities::HORSE, 2, DVec3::ZERO, Weak::new());
    horse_loaded.load_additional(roundtrip_nbt(&nbt));

    assert!(horse_loaded.is_tamed());
    assert!(horse_loaded.is_saddled());
    assert_eq!(horse_loaded.temper(), 10);
    assert_eq!(horse_loaded.owner_uuid(), Some(owner));
}

#[test]
fn donkey_and_mule_chests_and_saddles() {
    init_vanilla_registry();

    let donkey = DonkeyEntity::new(&vanilla_entities::DONKEY, 1, DVec3::ZERO, Weak::new());
    assert!(!donkey.has_chest());
    assert!(!donkey.is_saddled());

    donkey.set_tamed(true);
    donkey.set_has_chest(true);
    donkey.set_saddled(true);
    assert!(donkey.has_chest());
    assert!(donkey.is_saddled());

    let mut donkey_nbt = NbtCompound::new();
    donkey.save_additional(&mut donkey_nbt);

    let donkey_loaded = DonkeyEntity::new(&vanilla_entities::DONKEY, 2, DVec3::ZERO, Weak::new());
    donkey_loaded.load_additional(roundtrip_nbt(&donkey_nbt));
    assert!(donkey_loaded.has_chest());
    assert!(donkey_loaded.is_saddled());

    let mule = MuleEntity::new(&vanilla_entities::MULE, 3, DVec3::ZERO, Weak::new());
    assert!(!mule.has_chest());
    mule.set_has_chest(true);
    mule.set_saddled(true);
    assert!(mule.has_chest());
    assert!(mule.is_saddled());
}

#[test]
fn skeleton_horse_and_zombie_horse_riding() {
    init_vanilla_registry();

    let skel_horse =
        SkeletonHorseEntity::new(&vanilla_entities::SKELETON_HORSE, 1, DVec3::ZERO, Weak::new());
    assert!(!skel_horse.is_saddled());
    assert!(!skel_horse.is_trap());

    skel_horse.set_saddled(true);
    skel_horse.set_trap(true);
    assert!(skel_horse.is_saddled());
    assert!(skel_horse.is_trap());

    let mut skel_nbt = NbtCompound::new();
    skel_horse.save_additional(&mut skel_nbt);

    let skel_loaded =
        SkeletonHorseEntity::new(&vanilla_entities::SKELETON_HORSE, 2, DVec3::ZERO, Weak::new());
    skel_loaded.load_additional(roundtrip_nbt(&skel_nbt));
    assert!(skel_loaded.is_saddled());
    assert!(skel_loaded.is_trap());

    let zombie_horse =
        ZombieHorseEntity::new(&vanilla_entities::ZOMBIE_HORSE, 3, DVec3::ZERO, Weak::new());
    assert!(!zombie_horse.is_saddled());
    zombie_horse.set_saddled(true);
    assert!(zombie_horse.is_saddled());
}

#[test]
fn llama_and_trader_llama_taming_and_carpets() {
    init_vanilla_registry();

    let llama = LlamaEntity::new(&vanilla_entities::LLAMA, 1, DVec3::ZERO, Weak::new());
    assert!(!llama.is_tamed());
    assert!(!llama.has_chest());

    llama.set_tamed(true);
    llama.set_has_chest(true);
    llama.set_strength(4);
    llama.set_variant(2);

    assert!(llama.is_tamed());
    assert!(llama.has_chest());
    assert_eq!(llama.strength(), 4);
    assert_eq!(llama.variant(), 2);
    assert!(llama.can_use_slot(EquipmentSlot::Body));

    let mut llama_nbt = NbtCompound::new();
    llama.save_additional(&mut llama_nbt);

    let llama_loaded = LlamaEntity::new(&vanilla_entities::LLAMA, 2, DVec3::ZERO, Weak::new());
    llama_loaded.load_additional(roundtrip_nbt(&llama_nbt));
    assert!(llama_loaded.is_tamed());
    assert!(llama_loaded.has_chest());
    assert_eq!(llama_loaded.strength(), 4);
    assert_eq!(llama_loaded.variant(), 2);

    let trader_llama =
        TraderLlamaEntity::new(&vanilla_entities::TRADER_LLAMA, 3, DVec3::ZERO, Weak::new());
    trader_llama.set_strength(5);
    assert_eq!(trader_llama.strength(), 5);
}

#[test]
fn strider_saddle_and_boost_steering() {
    init_vanilla_registry();

    let strider = StriderEntity::new(&vanilla_entities::STRIDER, 1, DVec3::ZERO, Weak::new());
    assert!(!strider.is_saddled());
    assert!(!strider.is_shivering());

    strider.set_saddled(true);
    strider.set_shivering(true);
    assert!(strider.is_saddled());
    assert!(strider.is_shivering());

    assert!(strider.can_use_slot(EquipmentSlot::Saddle));

    let mut nbt = NbtCompound::new();
    strider.save_additional(&mut nbt);

    let strider_loaded =
        StriderEntity::new(&vanilla_entities::STRIDER, 2, DVec3::ZERO, Weak::new());
    strider_loaded.load_additional(roundtrip_nbt(&nbt));
    assert!(strider_loaded.is_saddled());
}

#[test]
fn camel_and_camel_husk_riding_and_dashing() {
    init_vanilla_registry();

    let camel = CamelEntity::new(&vanilla_entities::CAMEL, 1, DVec3::ZERO, Weak::new());
    assert!(!camel.is_saddled());
    assert!(!camel.is_dashing());

    camel.set_saddled(true);
    camel.set_dashing(true);
    camel.set_last_pose_change_tick(1234);

    assert!(camel.is_saddled());
    assert!(camel.is_dashing());
    assert_eq!(camel.last_pose_change_tick(), 1234);

    let mut nbt = NbtCompound::new();
    camel.save_additional(&mut nbt);

    let camel_loaded = CamelEntity::new(&vanilla_entities::CAMEL, 2, DVec3::ZERO, Weak::new());
    camel_loaded.load_additional(roundtrip_nbt(&nbt));
    assert!(camel_loaded.is_saddled());
    assert_eq!(camel_loaded.last_pose_change_tick(), 1234);

    let camel_husk =
        CamelHuskEntity::new(&vanilla_entities::CAMEL_HUSK, 3, DVec3::ZERO, Weak::new());
    assert!(!camel_husk.is_saddled());
    camel_husk.set_saddled(true);
    assert!(camel_husk.is_saddled());
}

#[test]
fn happy_ghast_riding_and_harness() {
    init_vanilla_registry();

    let ghast = HappyGhastEntity::new(&vanilla_entities::HAPPY_GHAST, 1, DVec3::ZERO, Weak::new());
    assert!(!ghast.is_harnessed());
    assert!(!ghast.stays_still());
    assert!(!ghast.is_leash_holder());

    ghast.set_harnessed(true);
    ghast.set_stays_still(true);
    ghast.set_is_leash_holder(true);

    assert!(ghast.is_harnessed());
    assert!(ghast.stays_still());
    assert!(ghast.is_leash_holder());
    assert!(ghast.can_use_slot(EquipmentSlot::Saddle));
    assert!(ghast.can_use_slot(EquipmentSlot::Body));

    let mut nbt = NbtCompound::new();
    ghast.save_additional(&mut nbt);

    let ghast_loaded =
        HappyGhastEntity::new(&vanilla_entities::HAPPY_GHAST, 2, DVec3::ZERO, Weak::new());
    ghast_loaded.load_additional(roundtrip_nbt(&nbt));
    assert!(ghast_loaded.is_harnessed());
    assert!(ghast_loaded.stays_still());
    assert!(ghast_loaded.is_leash_holder());
}
