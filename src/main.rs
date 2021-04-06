mod args;
mod config;
mod util;

use std::collections::VecDeque;

use config::Config;
use futures_util::StreamExt;
use log::{debug, error};
use swayipc::{
    bail,
    reply::{Node, NodeType},
    Connection, Error, EventType, Fallible,
};

fn get_windows<'a>(node: &'a Node, windows: &mut Vec<&'a Node>) {
    if node.node_type == NodeType::FloatingCon || node.node_type == NodeType::Con {
        if let Some(_) = node.name {
            windows.push(node)
        }
    };

    for node in &node.nodes {
        get_windows(node, windows);
    }
    for node in &node.floating_nodes {
        get_windows(node, windows)
    }
}

async fn update_workspace_name(config: &mut Config, workspace: &Node) -> Fallible<()> {
    let mut conn = Connection::new().await?;

    let mut windows = vec![];
    get_windows(workspace, &mut windows);

    let icons: Vec<String> = windows
        .iter()
        .map(|node| config.fetch_icon(&node).to_string())
        .collect();

    let name = match &workspace.name {
        Some(name) => name,
        None => bail!("Could not get name for workspace with id: {}", workspace.id),
    };

    let index = match workspace.num {
        Some(num) => num,
        None => bail!("Could not fetch index for: {}", name),
    };

    let mut icons = icons.join(" ");
    if icons.len() > 0 {
        icons.push_str(" ")
    }

    let new_name = if icons.len() > 0 {
        format!("{}: {}", index, icons)
    } else if let Some(num) = workspace.num {
        format!("{}", num)
    } else {
        error!("Could not fetch workspace num for: {:?}", workspace.name);
        " ".to_string()
    };

    if *name != new_name {
        debug!("rename workspace \"{}\" to \"{}\"", name, new_name);

        conn.run_command(format!("rename workspace \"{}\" to \"{}\"", name, new_name))
            .await?;
    }

    return Ok(());
}

fn get_workspace_with_focus_recurse<'a>(
    parents: &mut VecDeque<&'a Node>,
    node: &'a Node,
) -> Option<&'a Node> {
    if node.focused {
        if node.node_type == NodeType::Workspace {
            return Some(node);
        } else if node.node_type == NodeType::Con || node.node_type == NodeType::FloatingCon {
            for parent in parents.iter() {
                if parent.node_type == NodeType::Workspace {
                    return Some(parent);
                }
            }
        }
    }

    for child in node.nodes.iter().chain(node.floating_nodes.iter()) {
        parents.push_front(child);
        if let Some(n) = get_workspace_with_focus_recurse(parents, child) {
            return Some(n);
        }
        parents.pop_front();
    }

    return None;
}

fn get_workspace_with_focus(tree: &Node) -> Result<&Node, Error> {
    if let Some(workspace) = get_workspace_with_focus_recurse(&mut VecDeque::new(), tree) {
        return Ok(workspace);
    }

    bail!("Could not find a workspace with focus")
}

async fn update_workspace(con: &mut Connection, config: &mut Config) -> Fallible<()> {
    let tree = con.get_tree().await?;
    let workspace = get_workspace_with_focus(&tree)?;
    update_workspace_name(config, workspace).await?;
    Ok(())
}

async fn subscribe_to_window_events(mut config: Config) -> Fallible<()> {
    let mut events = Connection::new()
        .await?
        .subscribe(&[EventType::Workspace, EventType::Window])
        .await?;

    let mut con = Connection::new().await?;

    while let Some(event) = events.next().await {
        if let Ok(_) = event {
            if let Err(e) = update_workspace(&mut con, &mut config).await {
                error!("Could not update workspace name: {}", e);
            }
        }
    }

    return Ok(());
}

#[tokio::main]
async fn main() -> Fallible<()> {
    args::setup();

    let config = Config::new()?;

    subscribe_to_window_events(config).await?;

    Ok(())
}
