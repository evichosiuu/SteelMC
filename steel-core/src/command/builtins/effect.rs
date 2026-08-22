//! Vanilla status effect management command.

use std::borrow::Cow;

use steel_registry::mob_effect::MobEffectRef;
use steel_utils::{Identifier, translations};
use text_components::{TextComponent, translation::TranslatedMessage};

use super::super::{
    brigadier::{ArgumentType, CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use crate::entity::{INFINITE_EFFECT_DURATION, MobEffectInstance};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("effect"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("effect")
        .then(
            literal("clear")
                .executes(clear_self_all)
                .then(
                    argument("targets", SteelArgumentType::entities())
                        .executes(clear_targets_all)
                        .then(
                            argument("effect", SteelArgumentType::mob_effect())
                                .executes(clear_targets_effect),
                        ),
                ),
        )
        .then(
            literal("give").then(
                argument("targets", SteelArgumentType::entities()).then(
                    argument("effect", SteelArgumentType::mob_effect())
                        .executes(give_default)
                        .then(
                            argument("seconds", ArgumentType::integer(0, 1_000_000))
                                .executes(give_with_seconds)
                                .then(
                                    argument("amplifier", ArgumentType::integer(0, 255))
                                        .executes(give_with_seconds_amplifier)
                                        .then(
                                            argument("hideParticles", ArgumentType::bool())
                                                .executes(give_with_seconds_amplifier_hide),
                                        ),
                                ),
                        )
                        .then(
                            literal("infinite")
                                .executes(give_infinite)
                                .then(
                                    argument("amplifier", ArgumentType::integer(0, 255))
                                        .executes(give_infinite_amplifier)
                                        .then(
                                            argument("hideParticles", ArgumentType::bool())
                                                .executes(give_infinite_amplifier_hide),
                                        ),
                                ),
                        ),
                ),
            ),
        )
}

fn clear_self_all(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let Some(sender_entity) = context.source().entity() else {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::ARGUMENT_ENTITY_NOTFOUND_ENTITY,
        )));
    };

    let targets = [sender_entity.clone()];
    clear_all_effects(context, &targets)
}

fn clear_targets_all(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.entities("targets")?;
    clear_all_effects(context, &targets)
}

fn clear_all_effects(
    context: &SteelCommandContext<CommandSource>,
    targets: &[crate::entity::SharedEntity],
) -> Result<i32, CommandSyntaxError> {
    let mut success_count = 0usize;
    for target in targets {
        if let Some(living) = target.as_living_entity() {
            if living.clear_mob_effects() {
                success_count += 1;
            }
        }
    }

    if success_count == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_FAILED,
        )));
    }

    let message = if let [target] = targets {
        translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_SUCCESS_SINGLE
            .message([TextComponent::plain(target.plain_text_name())])
            .component()
    } else {
        translations::COMMANDS_EFFECT_CLEAR_EVERYTHING_SUCCESS_MULTIPLE
            .message([TextComponent::plain(success_count.to_string())])
            .component()
    };
    context.source().send_success(&message, true);

    i32::try_from(success_count).map_err(|_| {
        CommandSyntaxError::dynamic("Cleared entity count exceeds command result range")
    })
}

fn clear_targets_effect(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.entities("targets")?;
    let effect = context.mob_effect("effect")?;

    let mut success_count = 0usize;
    for target in &targets {
        if let Some(living) = target.as_living_entity() {
            if living.remove_mob_effect(effect) {
                success_count += 1;
            }
        }
    }

    if success_count == 0 {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_FAILED,
        )));
    }

    let message = if let [target] = targets.as_slice() {
        translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_SUCCESS_SINGLE
            .message([effect_display_name(effect), TextComponent::plain(target.plain_text_name())])
            .component()
    } else {
        translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_SUCCESS_MULTIPLE
            .message([effect_display_name(effect), TextComponent::plain(success_count.to_string())])
            .component()
    };
    context.source().send_success(&message, true);

    i32::try_from(success_count).map_err(|_| {
        CommandSyntaxError::dynamic("Cleared entity count exceeds command result range")
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DurationOption {
    Seconds(i32),
    Infinite,
}

fn give_default(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    give_effect(context, DurationOption::Seconds(30), 0, false)
}

fn give_with_seconds(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let seconds = context.integer("seconds")?;
    give_effect(context, DurationOption::Seconds(seconds), 0, false)
}

fn give_with_seconds_amplifier(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let seconds = context.integer("seconds")?;
    let amplifier = context.integer("amplifier")?;
    give_effect(context, DurationOption::Seconds(seconds), amplifier, false)
}

fn give_with_seconds_amplifier_hide(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let seconds = context.integer("seconds")?;
    let amplifier = context.integer("amplifier")?;
    let hide_particles = context.boolean("hideParticles")?;
    give_effect(context, DurationOption::Seconds(seconds), amplifier, hide_particles)
}

fn give_infinite(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    give_effect(context, DurationOption::Infinite, 0, false)
}

fn give_infinite_amplifier(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let amplifier = context.integer("amplifier")?;
    give_effect(context, DurationOption::Infinite, amplifier, false)
}

fn give_infinite_amplifier_hide(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let amplifier = context.integer("amplifier")?;
    let hide_particles = context.boolean("hideParticles")?;
    give_effect(context, DurationOption::Infinite, amplifier, hide_particles)
}

fn give_effect(
    context: &SteelCommandContext<CommandSource>,
    duration: DurationOption,
    amplifier: i32,
    hide_particles: bool,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.entities("targets")?;
    let effect = context.mob_effect("effect")?;

    match duration {
        DurationOption::Seconds(0) => {
            let mut success_count = 0usize;
            for target in &targets {
                if let Some(living) = target.as_living_entity() {
                    if living.remove_mob_effect(effect) {
                        success_count += 1;
                    }
                }
            }

            if success_count == 0 {
                return Err(CommandSyntaxError::dynamic(TextComponent::from(
                    &translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_FAILED,
                )));
            }

            let message = if let [target] = targets.as_slice() {
                translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_SUCCESS_SINGLE
                    .message([
                        effect_display_name(effect),
                        TextComponent::plain(target.plain_text_name()),
                    ])
                    .component()
            } else {
                translations::COMMANDS_EFFECT_CLEAR_SPECIFIC_SUCCESS_MULTIPLE
                    .message([
                        effect_display_name(effect),
                        TextComponent::plain(success_count.to_string()),
                    ])
                    .component()
            };
            context.source().send_success(&message, true);

            i32::try_from(success_count).map_err(|_| {
                CommandSyntaxError::dynamic("Cleared entity count exceeds command result range")
            })
        }
        _ => {
            let duration_ticks = match duration {
                DurationOption::Seconds(s) => s.saturating_mul(20),
                DurationOption::Infinite => INFINITE_EFFECT_DURATION,
            };

            let visible = !hide_particles;
            let show_icon = !hide_particles;

            let mut success_count = 0usize;
            for target in &targets {
                if let Some(living) = target.as_living_entity() {
                    let instance = MobEffectInstance::with_duration(effect, duration_ticks, amplifier)
                        .with_visible(visible)
                        .with_show_icon(show_icon);
                    if living.add_mob_effect(instance) {
                        success_count += 1;
                    }
                }
            }

            if success_count == 0 {
                let message = TextComponent::from(&translations::COMMANDS_EFFECT_GIVE_FAILED);
                return Err(CommandSyntaxError::dynamic(message));
            }

            let message = if let [target] = targets.as_slice() {
                translations::COMMANDS_EFFECT_GIVE_SUCCESS_SINGLE
                    .message([
                        effect_display_name(effect),
                        TextComponent::plain(target.plain_text_name()),
                    ])
                    .component()
            } else {
                translations::COMMANDS_EFFECT_GIVE_SUCCESS_MULTIPLE
                    .message([
                        effect_display_name(effect),
                        TextComponent::plain(success_count.to_string()),
                    ])
                    .component()
            };
            context.source().send_success(&message, true);

            i32::try_from(success_count).map_err(|_| {
                CommandSyntaxError::dynamic("Applied entity count exceeds command result range")
            })
        }
    }
}

fn effect_display_name(effect: MobEffectRef) -> TextComponent {
    TextComponent::translated(TranslatedMessage {
        key: Cow::Owned(format!(
            "effect.{}.{}",
            effect.key.namespace, effect.key.path
        )),
        args: None,
        fallback: None,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;

    use glam::DVec3;
    use steel_registry::{
        entity_type::EntityTypeRef, init_vanilla_registry, vanilla_entities, vanilla_mob_effects,
    };
    use steel_utils::locks::SyncMutex;

    use super::super::create_dispatcher;
    use crate::{
        command::{
            brigadier::{ArgumentType, CommandDispatcher, NodeId},
            execution::{CommandSource, SteelArgumentType, SteelCommandRuntime},
        },
        entity::{
            Entity, EntityBase, INFINITE_EFFECT_DURATION, LivingEntity, LivingEntityBase,
            MobEffectInstance,
        },
    };

    type Dispatcher = CommandDispatcher<CommandSource, SteelCommandRuntime>;

    fn child(dispatcher: &Dispatcher, parent: NodeId, name: &str) -> NodeId {
        let Some(children) = dispatcher.children(parent) else {
            panic!("parent node should exist");
        };
        let Some(child) = children.iter().copied().find(|child| {
            dispatcher
                .node(*child)
                .is_some_and(|node| node.name() == name)
        }) else {
            panic!("child {name} should exist");
        };
        child
    }

    #[test]
    fn effect_graph_matches_vanilla_structure() {
        init_vanilla_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let effect = child(&dispatcher, dispatcher.root(), "effect");

        let clear = child(&dispatcher, effect, "clear");
        assert!(matches!(
            dispatcher.node(clear),
            Some(node) if node.is_executable()
        ));

        let clear_targets = child(&dispatcher, clear, "targets");
        assert_eq!(
            dispatcher
                .node(clear_targets)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::entities())
        );
        assert!(matches!(
            dispatcher.node(clear_targets),
            Some(node) if node.is_executable()
        ));

        let clear_effect = child(&dispatcher, clear_targets, "effect");
        assert_eq!(
            dispatcher
                .node(clear_effect)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::mob_effect())
        );
        assert!(matches!(
            dispatcher.node(clear_effect),
            Some(node) if node.is_executable()
        ));

        let give = child(&dispatcher, effect, "give");
        let give_targets = child(&dispatcher, give, "targets");
        assert_eq!(
            dispatcher
                .node(give_targets)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::entities())
        );

        let give_effect = child(&dispatcher, give_targets, "effect");
        assert_eq!(
            dispatcher
                .node(give_effect)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::mob_effect())
        );
        assert!(matches!(
            dispatcher.node(give_effect),
            Some(node) if node.is_executable()
        ));

        let seconds = child(&dispatcher, give_effect, "seconds");
        assert_eq!(
            dispatcher
                .node(seconds)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::from(ArgumentType::integer(0, 1_000_000)))
        );
        assert!(matches!(
            dispatcher.node(seconds),
            Some(node) if node.is_executable()
        ));

        let sec_amp = child(&dispatcher, seconds, "amplifier");
        assert_eq!(
            dispatcher
                .node(sec_amp)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::from(ArgumentType::integer(0, 255)))
        );
        assert!(matches!(
            dispatcher.node(sec_amp),
            Some(node) if node.is_executable()
        ));

        let sec_amp_hide = child(&dispatcher, sec_amp, "hideParticles");
        assert_eq!(
            dispatcher
                .node(sec_amp_hide)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::from(ArgumentType::Bool))
        );
        assert!(matches!(
            dispatcher.node(sec_amp_hide),
            Some(node) if node.is_executable()
        ));

        let infinite = child(&dispatcher, give_effect, "infinite");
        assert!(matches!(
            dispatcher.node(infinite),
            Some(node) if node.is_executable()
        ));

        let inf_amp = child(&dispatcher, infinite, "amplifier");
        assert_eq!(
            dispatcher
                .node(inf_amp)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::from(ArgumentType::integer(0, 255)))
        );
        assert!(matches!(
            dispatcher.node(inf_amp),
            Some(node) if node.is_executable()
        ));

        let inf_amp_hide = child(&dispatcher, inf_amp, "hideParticles");
        assert_eq!(
            dispatcher
                .node(inf_amp_hide)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::from(ArgumentType::Bool))
        );
        assert!(matches!(
            dispatcher.node(inf_amp_hide),
            Some(node) if node.is_executable()
        ));
    }

    #[test]
    fn living_entity_effect_give_and_clear_operations() {
        init_vanilla_registry();
        let target = TestLivingEntity::new(&vanilla_entities::ZOMBIE);

        // Give default speed (30s = 600 ticks, amplifier 0)
        let instance = MobEffectInstance::with_duration(vanilla_mob_effects::SPEED, 600, 0);
        assert!(target.add_mob_effect(instance));

        let active = target.mob_effect(vanilla_mob_effects::SPEED);
        assert!(active.is_some());
        let active = active.unwrap();
        assert_eq!(active.duration(), 600);
        assert_eq!(active.amplifier(), 0);
        assert!(active.is_visible());

        // Clear specific effect
        assert!(target.remove_mob_effect(vanilla_mob_effects::SPEED));
        assert!(target.mob_effect(vanilla_mob_effects::SPEED).is_none());

        // Give infinite strength with amplifier 2 and hide particles
        let instance =
            MobEffectInstance::with_duration(vanilla_mob_effects::STRENGTH, INFINITE_EFFECT_DURATION, 2)
                .with_visible(false)
                .with_show_icon(false);
        assert!(target.add_mob_effect(instance));

        let active = target.mob_effect(vanilla_mob_effects::STRENGTH);
        assert!(active.is_some());
        let active = active.unwrap();
        assert_eq!(active.duration(), INFINITE_EFFECT_DURATION);
        assert_eq!(active.amplifier(), 2);
        assert!(!active.is_visible());

        // Clear all effects
        assert!(target.clear_mob_effects());
        assert!(target.active_mob_effects().is_empty());
    }

    struct TestLivingEntity {
        base: EntityBase,
        living_base: LivingEntityBase,
        health: SyncMutex<f32>,
        entity_type: EntityTypeRef,
    }

    impl TestLivingEntity {
        fn new(entity_type: EntityTypeRef) -> Self {
            Self {
                base: EntityBase::new(1, DVec3::ZERO, entity_type.dimensions, Weak::new()),
                living_base: LivingEntityBase::new(entity_type),
                health: SyncMutex::new(20.0),
                entity_type,
            }
        }
    }

    crate::entity::impl_test_downcast_type!(TestLivingEntity);

    impl Entity for TestLivingEntity {
        fn base(&self) -> &EntityBase {
            &self.base
        }

        fn entity_type(&self) -> EntityTypeRef {
            self.entity_type
        }
    }

    impl LivingEntity for TestLivingEntity {
        fn living_base(&self) -> &LivingEntityBase {
            &self.living_base
        }

        fn get_health(&self) -> f32 {
            *self.health.lock()
        }

        fn set_health(&self, health: f32) {
            *self.health.lock() = health;
        }

        fn get_absorption_amount(&self) -> f32 {
            0.0
        }

        fn set_absorption_amount(&self, _amount: f32) {}
    }
}
