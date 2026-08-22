//! Helpers for applying potion contents to entities and world interactions.

use std::sync::Arc;

use steel_registry::TaggedRegistryExt as _;
use steel_registry::data_components::{PotionContents, vanilla_components};
use steel_registry::item_stack::ItemStack;
use steel_registry::mob_effect::MobEffectRef;
use steel_registry::vanilla_entity_type_tags::EntityTypeTag;
use steel_registry::{REGISTRY, vanilla_damage_types, vanilla_mob_effects};

use crate::entity::damage::DamageSource;
use crate::entity::{Entity, LivingEntity, MobEffectInstance};
use crate::world::World;

/// Applies all effects contained within an item stack's `POTION_CONTENTS`
/// component to `target`.
///
/// Duration multiplier is used to scale durations (e.g., 0.5 for splash potions,
/// 0.25 for lingering potions/clouds). `source_entity` is the thrower or damager if applicable.
pub fn apply_potion_effects(
    world: &Arc<World>,
    target: &dyn LivingEntity,
    stack: &ItemStack,
    duration_multiplier: f32,
    source_entity: Option<&dyn Entity>,
) {
    let Some(contents) = stack.get(vanilla_components::POTION_CONTENTS) else {
        return;
    };
    let duration_scale = stack
        .get(vanilla_components::POTION_DURATION_SCALE)
        .copied()
        .unwrap_or(1.0)
        * duration_multiplier;

    apply_potion_contents_effects(world, target, contents, duration_scale, source_entity);
}

/// Applies all effects in `PotionContents` to `target`.
pub fn apply_potion_contents_effects(
    world: &Arc<World>,
    target: &dyn LivingEntity,
    contents: &PotionContents,
    duration_scale: f32,
    source_entity: Option<&dyn Entity>,
) {
    let mut effects_to_apply = Vec::new();

    if let Some(potion_ref) = contents.potion() {
        for effect in potion_ref.value().effects {
            effects_to_apply.push((effect.effect, effect.duration, effect.amplifier));
        }
    }

    for custom_effect in contents.custom_effects() {
        effects_to_apply.push((
            custom_effect.effect(),
            custom_effect.duration(),
            custom_effect.amplifier(),
        ));
    }

    let is_undead = REGISTRY
        .entity_types
        .is_in_tag(target.entity_type(), &EntityTypeTag::INVERTED_HEALING_AND_HARM)
        || REGISTRY
            .entity_types
            .is_in_tag(target.entity_type(), &EntityTypeTag::UNDEAD);

    for (effect_ref, duration, amplifier) in effects_to_apply {
        if is_instant_effect(effect_ref) {
            apply_instant_effect(
                world,
                target,
                effect_ref,
                amplifier,
                duration_scale,
                is_undead,
                source_entity,
            );
        } else {
            let scaled_duration = (duration as f32 * duration_scale) as i32;
            if scaled_duration > 0 {
                let instance = MobEffectInstance::with_duration(effect_ref, scaled_duration, amplifier);
                target.add_mob_effect(instance);
            }
        }
    }
}

/// Returns whether an effect is instant (applied once immediately rather than as a duration status effect).
pub fn is_instant_effect(effect: MobEffectRef) -> bool {
    effect == vanilla_mob_effects::INSTANT_HEALTH
        || effect == vanilla_mob_effects::INSTANT_DAMAGE
        || effect == vanilla_mob_effects::SATURATION
}

fn apply_instant_effect(
    world: &Arc<World>,
    target: &dyn LivingEntity,
    effect: MobEffectRef,
    amplifier: i32,
    scale: f32,
    is_undead: bool,
    source_entity: Option<&dyn Entity>,
) {
    if effect == vanilla_mob_effects::INSTANT_HEALTH {
        if is_undead {
            let damage = ((6 << amplifier) as f32) * scale;
            if damage > 0.0 {
                let mut source = DamageSource::environment(&vanilla_damage_types::INDIRECT_MAGIC);
                if let Some(src) = source_entity {
                    source = source.with_causing_entity(src.id()).with_direct_entity(src.id());
                }
                target.hurt(world, &source, damage);
            }
        } else {
            let heal_amount = ((4 << amplifier) as f32) * scale;
            if heal_amount > 0.0 {
                target.heal(heal_amount);
            }
        }
    } else if effect == vanilla_mob_effects::INSTANT_DAMAGE {
        if is_undead {
            let heal_amount = ((4 << amplifier) as f32) * scale;
            if heal_amount > 0.0 {
                target.heal(heal_amount);
            }
        } else {
            let damage = ((6 << amplifier) as f32) * scale;
            if damage > 0.0 {
                let mut source = DamageSource::environment(&vanilla_damage_types::INDIRECT_MAGIC);
                if let Some(src) = source_entity {
                    source = source.with_causing_entity(src.id()).with_direct_entity(src.id());
                }
                target.hurt(world, &source, damage);
            }
        }
    } else if effect == vanilla_mob_effects::SATURATION {
        if let Some(player) = target.as_player() {
            let food_increase = (amplifier + 1) * 2;
            let saturation_increase = (amplifier + 1) as f32 * 2.0;
            player.food_data.lock().eat(food_increase, saturation_increase);
        }
    }
}
