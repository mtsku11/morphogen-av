use serde::{Deserialize, Serialize};

use crate::{Node, NodeId};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeGraph {
    pub nodes: Vec<Node>,
    pub routes: Vec<ModulationRoute>,
}

impl NodeGraph {
    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| &node.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModulationRoute {
    pub from_node: NodeId,
    pub from_output: String,
    pub to_node: NodeId,
    pub to_parameter: String,
    pub amount: f32,
}
