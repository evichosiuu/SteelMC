use std::io::Cursor;
use std::sync::Weak;

use glam::DVec3;
use simdnbt::borrow::read_compound as read_borrowed_compound;
use simdnbt::owned::NbtCompound;
use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_villager_professions};

use super::*;

#[test]
fn villager_initial_state() {
    init_vanilla_registry();

    let villager = VillagerEntity::new(&vanilla_entities::VILLAGER, 1, DVec3::ZERO, Weak::new());

    assert_eq!(villager.villager_level(), 1);
    assert_eq!(villager.villager_xp(), 0);
    assert_eq!(
        villager.profession().unwrap().key,
        vanilla_villager_professions::NONE.key
    );
}

#[test]
fn villager_level_progression() {
    init_vanilla_registry();

    let villager = VillagerEntity::new(&vanilla_entities::VILLAGER, 1, DVec3::ZERO, Weak::new());

    if let Some(farmer) = REGISTRY
        .villager_professions
        .by_key(&vanilla_villager_professions::FARMER.key)
    {
        villager.set_profession(farmer);
    }

    assert_eq!(villager.villager_level(), 1);

    // Adding 10 XP levels up to level 2
    villager.add_villager_xp(10);
    assert_eq!(villager.villager_level(), 2);
    assert_eq!(villager.villager_xp(), 10);
    assert!(!villager.offers.lock().is_empty());
}

#[test]
fn villager_nbt_roundtrip() {
    init_vanilla_registry();

    let villager = VillagerEntity::new(&vanilla_entities::VILLAGER, 1, DVec3::ZERO, Weak::new());
    if let Some(farmer) = REGISTRY
        .villager_professions
        .by_key(&vanilla_villager_professions::FARMER.key)
    {
        villager.set_profession(farmer);
    }
    villager.add_villager_xp(15);
    villager.set_villager_level(2);

    let mut nbt = NbtCompound::new();
    villager.save_additional(&mut nbt);

    let mut bytes = Vec::new();
    nbt.write(&mut bytes);
    let tag = read_borrowed_compound(&mut Cursor::new(bytes.as_slice())).unwrap();

    let reloaded = VillagerEntity::new(&vanilla_entities::VILLAGER, 2, DVec3::ZERO, Weak::new());
    reloaded.load_additional((&tag).into());

    assert_eq!(reloaded.villager_level(), 2);
    assert_eq!(reloaded.villager_xp(), 15);
    assert_eq!(
        reloaded.profession().unwrap().key,
        vanilla_villager_professions::FARMER.key
    );
}
