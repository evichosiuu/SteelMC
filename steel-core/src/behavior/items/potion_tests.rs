#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::DVec3;
    use steel_registry::data_components::{PotionContents, vanilla_components};
    use steel_registry::{
        RegistryReference, init_vanilla_registry, vanilla_entities, vanilla_items,
        vanilla_mob_effects, vanilla_potions,
    };

    use crate::behavior::init_behaviors;
    use crate::behavior::potion_utils::apply_potion_effects;
    use crate::entity::entities::mobs::hostile::ZombieEntity;
    use crate::entity::{LivingEntity, next_entity_id};
    use crate::test_support::test_world;

    #[test]
    fn test_drinking_potion_applies_status_effect() {
        init_vanilla_registry();
        init_behaviors();

        let world = test_world();
        let zombie = Arc::new(ZombieEntity::new(
            &vanilla_entities::ZOMBIE,
            next_entity_id(),
            DVec3::ZERO,
            Arc::downgrade(&world),
        ));

        let mut stack = steel_registry::item_stack::ItemStack::new(&vanilla_items::POTION);
        let contents = PotionContents::new(
            Some(RegistryReference::new(&vanilla_potions::SWIFTNESS)),
            None,
            Vec::new(),
            None,
        );
        stack.set(vanilla_components::POTION_CONTENTS, contents);

        apply_potion_effects(&world, zombie.as_ref(), &stack, 1.0, None);

        assert!(zombie.has_mob_effect(vanilla_mob_effects::SPEED));
    }

    #[test]
    fn test_instant_health_heals_living_and_hurts_undead() {
        init_vanilla_registry();
        init_behaviors();

        let world = test_world();
        let zombie = Arc::new(ZombieEntity::new(
            &vanilla_entities::ZOMBIE,
            next_entity_id(),
            DVec3::ZERO,
            Arc::downgrade(&world),
        ));
        let initial_health = zombie.get_health();

        let mut stack = steel_registry::item_stack::ItemStack::new(&vanilla_items::POTION);
        let contents = PotionContents::new(
            Some(RegistryReference::new(&vanilla_potions::HEALING)),
            None,
            Vec::new(),
            None,
        );
        stack.set(vanilla_components::POTION_CONTENTS, contents);

        // Healing hurts undead (zombie)
        apply_potion_effects(&world, zombie.as_ref(), &stack, 1.0, None);
        assert!(zombie.get_health() < initial_health);
    }
}
