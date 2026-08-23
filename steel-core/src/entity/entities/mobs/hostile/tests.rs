#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::{init_vanilla_registry, vanilla_entities, vanilla_items};
    use steel_utils::types::Difficulty;
    use steel_utils::Downcast;

    use crate::entity::entities::mobs::bosses::WitherBossEntity;
    use crate::entity::entities::mobs::hostile::{
        BlazeEntity, SkeletonEntity, WardenEntity, ZombieEntity,
    };
    use crate::entity::entities::mobs::neutral::{BeeEntity, WolfEntity};
    use crate::entity::entities::mobs::passive::AllayEntity;
    use crate::entity::{Entity, EntitySpawnReason, LivingEntity, Mob};
    use crate::inventory::equipment::EquipmentSlot;
    use crate::test_support::fresh_test_world;

    #[test]
    fn zombie_entity_registers_goals_and_downcast() {
        init_vanilla_registry();
        let zombie = ZombieEntity::new(&vanilla_entities::ZOMBIE, 1, DVec3::ZERO, Weak::new());

        let dyn_entity: &dyn Entity = &zombie;
        assert!(dyn_entity.downcast_ref::<ZombieEntity>().is_some());

        let selector = zombie.mob_base().goal_selector().lock();
        assert_eq!(selector.available_goal_count(), 5);
        drop(selector);

        let target_selector = zombie.mob_base().target_selector().lock();
        assert_eq!(target_selector.available_goal_count(), 2);
    }

    #[test]
    fn skeleton_entity_registers_goals_and_downcast() {
        init_vanilla_registry();
        let skeleton = SkeletonEntity::new(&vanilla_entities::SKELETON, 2, DVec3::ZERO, Weak::new());

        let dyn_entity: &dyn Entity = &skeleton;
        assert!(dyn_entity.downcast_ref::<SkeletonEntity>().is_some());

        let selector = skeleton.mob_base().goal_selector().lock();
        assert_eq!(selector.available_goal_count(), 6);
        drop(selector);

        let target_selector = skeleton.mob_base().target_selector().lock();
        assert_eq!(target_selector.available_goal_count(), 2);
    }

    #[test]
    fn zombie_and_skeleton_health_clamping() {
        init_vanilla_registry();
        let zombie = ZombieEntity::new(&vanilla_entities::ZOMBIE, 1, DVec3::ZERO, Weak::new());
        let skeleton = SkeletonEntity::new(&vanilla_entities::SKELETON, 2, DVec3::ZERO, Weak::new());

        zombie.set_health(100.0);
        assert_eq!(zombie.get_health(), zombie.get_max_health());

        zombie.set_health(-10.0);
        assert_eq!(zombie.get_health(), 0.0);

        skeleton.set_health(100.0);
        assert_eq!(skeleton.get_health(), skeleton.get_max_health());

        skeleton.set_health(-10.0);
        assert_eq!(skeleton.get_health(), 0.0);
    }

    #[test]
    fn skeleton_finalize_spawn_equips_bow() {
        init_vanilla_registry();
        let world = fresh_test_world("skeleton_spawn_peaceful");
        let skeleton = SkeletonEntity::new(&vanilla_entities::SKELETON, 10, DVec3::ZERO, std::sync::Arc::downgrade(&world));

        let _ = skeleton.finalize_spawn(&world, EntitySpawnReason::Natural, None);

        let mut held_item = None;
        skeleton.with_equipment_slot(EquipmentSlot::MainHand, &mut |stack| {
            held_item = Some(stack.clone());
        });

        let item = held_item.expect("skeleton should hold an item after finalize_spawn");
        assert!(item.is(&vanilla_items::BOW));
    }

    #[test]
    fn skeleton_finalize_spawn_can_enchant_bow() {
        init_vanilla_registry();
        let world = fresh_test_world("skeleton_spawn_hard");
        world.set_difficulty(Difficulty::Hard);

        let mut found_enchanted = false;
        for i in 0..100 {
            let skeleton = SkeletonEntity::new(
                &vanilla_entities::SKELETON,
                100 + i,
                DVec3::ZERO,
                std::sync::Arc::downgrade(&world),
            );
            let _ = skeleton.finalize_spawn(&world, EntitySpawnReason::Natural, None);

            skeleton.with_equipment_slot(EquipmentSlot::MainHand, &mut |stack| {
                if stack.get_enchantments().is_some_and(|e| !e.is_empty()) {
                    found_enchanted = true;
                }
            });

            if found_enchanted {
                break;
            }
        }

        assert!(found_enchanted, "skeleton finalize_spawn on hard difficulty should have a chance to produce an enchanted bow");
    }

    #[test]
    fn test_new_mobs_downcasting_and_ai_goals() {
        init_vanilla_registry();

        let blaze = BlazeEntity::new(&vanilla_entities::BLAZE, 10, DVec3::ZERO, Weak::new());
        assert!((&blaze as &dyn Entity).downcast_ref::<BlazeEntity>().is_some());
        assert!(blaze.mob_base().goal_selector().lock().available_goal_count() > 0);

        let wither = WitherBossEntity::new(&vanilla_entities::WITHER, 11, DVec3::ZERO, Weak::new());
        assert!((&wither as &dyn Entity).downcast_ref::<WitherBossEntity>().is_some());

        let wolf = WolfEntity::new(&vanilla_entities::WOLF, 12, DVec3::ZERO, Weak::new());
        assert!((&wolf as &dyn Entity).downcast_ref::<WolfEntity>().is_some());

        let bee = BeeEntity::new(&vanilla_entities::BEE, 13, DVec3::ZERO, Weak::new());
        assert!((&bee as &dyn Entity).downcast_ref::<BeeEntity>().is_some());

        let allay = AllayEntity::new(&vanilla_entities::ALLAY, 14, DVec3::ZERO, Weak::new());
        assert!((&allay as &dyn Entity).downcast_ref::<AllayEntity>().is_some());

        let warden = WardenEntity::new(&vanilla_entities::WARDEN, 15, DVec3::ZERO, Weak::new());
        assert!((&warden as &dyn Entity).downcast_ref::<WardenEntity>().is_some());
    }
}
