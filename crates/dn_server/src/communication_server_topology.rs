use std::collections::{HashMap, HashSet, VecDeque};
use petgraph::graphmap::UnGraphMap;
use wg_2024::network::NodeId;
use wg_2024::packet::NodeType;

type Topology = UnGraphMap<NodeId, ()>;

pub struct CommunicationServerNetworkTopology {
    pub graph: Topology,
    pub node_types: HashMap<NodeId, NodeType>,
}

impl CommunicationServerNetworkTopology {
    pub fn new() -> Self {
        Self {
            graph: Topology::new(),
            node_types: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node_id: NodeId, node_type: NodeType) {
        if !self.graph.contains_node(node_id) {
            self.graph.add_node(node_id);
        }
        self.node_types.entry(node_id) // I assume the node type does not change
            .or_insert(node_type);
    }

    pub fn add_edge(&mut self, node_a: NodeId, node_b: NodeId) {
        if !self.graph.contains_edge(node_a, node_b) {
            self.graph.add_edge(node_a, node_b, ());
        }
    }

    pub fn contains_node(&self, node_id: NodeId) -> bool {
        self.graph.contains_node(node_id)
    }

    pub fn contains_edge(&self, node_a: NodeId, node_b: NodeId) -> bool {
        self.graph.contains_edge(node_a, node_b)
    }

    pub fn get_node_type(&self, node_id: &NodeId) -> Option<&NodeType> {
        self.node_types.get(node_id)
    }

    pub fn source_routing(&self, from: NodeId, to: NodeId) -> Vec<NodeId> {

        // todo!: currently using a simple BFS
        let mut visited = HashSet::new();
        let mut parent_map = HashMap::new();
        let mut queue = VecDeque::new();

        queue.push_back(from);
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            if current == to {
                let mut route = Vec::new();
                let mut node = current;
                while let Some(&parent) = parent_map.get(&node) {
                    route.push(node);
                    node = parent;
                }
                route.push(from);
                route.reverse();
                return route;
            }

            for neighbor in self.graph.neighbors(current) {
                if visited.insert(neighbor) {
                    if neighbor != to && neighbor != from {
                        if let Some(node_type) = self.node_types.get(&neighbor) {
                            if *node_type != NodeType::Drone {
                                continue; // I avoid passing through nodes that are not drones
                            }
                        }
                    }

                    parent_map.insert(neighbor, current);
                    queue.push_back(neighbor);
                }
            }
        }

        vec![]

    }
}
