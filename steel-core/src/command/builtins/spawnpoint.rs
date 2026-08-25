//! Vanilla spawnpoint command.

use std::sync::Arc;

use steel_utils::{BlockPos, Identifier, translations};
use text_components::TextComponent;

use super::super::{
    brigadier::{CommandNodeBuilder, CommandSyntaxError},
    execution::{
        CommandSource, SteelArgumentType, SteelCommandContext, SteelCommandRuntime, argument,
        literal,
    },
    registration::CommandRegistration,
};
use crate::{
    entity::Entity,
    level_data::RespawnData,
    player::{Player, PlayerRespawnConfig},
    world::World,
};

pub(super) fn registration() -> CommandRegistration<CommandSource> {
    CommandRegistration::new(Identifier::vanilla_static("spawnpoint"), |_| command())
}

fn command() -> CommandNodeBuilder<CommandSource, SteelCommandRuntime> {
    literal("spawnpoint")
        .executes(set_spawn_source)
        .then(
            argument("targets", SteelArgumentType::players())
                .executes(set_spawn_targets_default)
                .then(
                    argument("pos", SteelArgumentType::block_pos())
                        .executes(set_spawn_targets_pos)
                        .then(
                            argument("angle", SteelArgumentType::angle())
                                .executes(set_spawn_targets_pos_angle),
                        ),
                ),
        )
}

fn set_spawn_source(context: &SteelCommandContext<CommandSource>) -> Result<i32, CommandSyntaxError> {
    let Some(player) = context.source().player() else {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::PERMISSIONS_REQUIRES_PLAYER,
        )));
    };
    let pos = BlockPos::from(player.position());
    let yaw = player.rotation().0;
    set_spawn_point(context, std::slice::from_ref(player), pos, yaw)
}

fn set_spawn_targets_default(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.players("targets")?;
    let pos = BlockPos::from(context.source().position());
    let yaw = context.source().rotation().0;
    set_spawn_point(context, &targets, pos, yaw)
}

fn set_spawn_targets_pos(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.players("targets")?;
    let pos = spawnable_position(context)?;
    set_spawn_point(context, &targets, pos, 0.0)
}

fn set_spawn_targets_pos_angle(
    context: &SteelCommandContext<CommandSource>,
) -> Result<i32, CommandSyntaxError> {
    let targets = context.players("targets")?;
    let pos = spawnable_position(context)?;
    let yaw = context.angle("angle")?;
    set_spawn_point(context, &targets, pos, yaw)
}

fn spawnable_position(
    context: &SteelCommandContext<CommandSource>,
) -> Result<BlockPos, CommandSyntaxError> {
    let coordinates = context.coordinates("pos")?;
    let position = coordinates.block_pos(context.source());
    if !World::is_in_spawnable_bounds(position) {
        return Err(CommandSyntaxError::dynamic(TextComponent::from(
            &translations::ARGUMENT_POS_OUTOFBOUNDS,
        )));
    }
    Ok(position)
}

fn set_spawn_point(
    context: &SteelCommandContext<CommandSource>,
    targets: &[Arc<Player>],
    pos: BlockPos,
    yaw: f32,
) -> Result<i32, CommandSyntaxError> {
    let source = context.source();
    let world_key = source.world().key.clone();

    for target in targets {
        target.set_respawn_position(
            Some(PlayerRespawnConfig::new(
                RespawnData::of(world_key.clone(), pos, yaw, 0.0),
                true,
            )),
            false,
        );
    }

    let result = i32::try_from(targets.len()).map_err(|_| {
        CommandSyntaxError::dynamic("Target player count exceeds the command result range")
    })?;

    if let [target] = targets {
        let message = translations::COMMANDS_SPAWNPOINT_SUCCESS_SINGLE_NEW
            .message([
                pos.x().to_string(),
                pos.y().to_string(),
                pos.z().to_string(),
                yaw.to_string(),
                "0".to_string(),
                world_key.to_string(),
                target.plain_text_name(),
            ])
            .component();
        source.send_success(&message, true);
    } else {
        let message = translations::COMMANDS_SPAWNPOINT_SUCCESS_MULTIPLE_NEW
            .message([
                pos.x().to_string(),
                pos.y().to_string(),
                pos.z().to_string(),
                yaw.to_string(),
                "0".to_string(),
                world_key.to_string(),
                targets.len().to_string(),
            ])
            .component();
        source.send_success(&message, true);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use steel_registry::init_vanilla_registry;

    use super::super::create_dispatcher;
    use crate::command::{
        brigadier::{CommandDispatcher, NodeId},
        execution::{CommandSource, SteelArgumentType, SteelCommandRuntime},
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
            panic!("child `{name}` should exist");
        };
        child
    }

    #[test]
    fn spawnpoint_graph_uses_expected_arguments() {
        init_vanilla_registry();
        let Ok(dispatcher) = create_dispatcher() else {
            panic!("built-in commands should register");
        };
        let root = child(&dispatcher, dispatcher.root(), "spawnpoint");
        let Some(root_node) = dispatcher.node(root) else {
            panic!("spawnpoint root should exist");
        };
        assert!(root_node.is_executable());

        let targets = child(&dispatcher, root, "targets");
        assert_eq!(
            dispatcher
                .node(targets)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::players())
        );
        let Some(targets_node) = dispatcher.node(targets) else {
            panic!("spawnpoint targets should exist");
        };
        assert!(targets_node.is_executable());

        let pos = child(&dispatcher, targets, "pos");
        assert_eq!(
            dispatcher.node(pos).and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::block_pos())
        );
        let Some(pos_node) = dispatcher.node(pos) else {
            panic!("spawnpoint pos should exist");
        };
        assert!(pos_node.is_executable());

        let angle = child(&dispatcher, pos, "angle");
        assert_eq!(
            dispatcher
                .node(angle)
                .and_then(|node| node.argument_type()),
            Some(&SteelArgumentType::angle())
        );
        let Some(angle_node) = dispatcher.node(angle) else {
            panic!("spawnpoint angle should exist");
        };
        assert!(angle_node.is_executable());
    }
}
