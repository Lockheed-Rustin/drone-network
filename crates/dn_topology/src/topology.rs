use petgraph::prelude::UnGraphMap;
use std::{cmp, hash};
use wg_2024::{network::NodeId, packet::NodeType};

#[derive(Debug, Clone, Copy)]
pub struct Node {
    pub id: NodeId,
    pub ty: NodeType,
}

impl hash::Hash for Node {
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        state.write_u8(self.id);
    }
}

impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.id.eq(&other.id)
    }
}

impl Eq for Node {}

impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl Ord for Node {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

pub type Topology = UnGraphMap<Node, ()>;
