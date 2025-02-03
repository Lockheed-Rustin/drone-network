use petgraph::graphmap::UnGraphMap;
use std::collections::{HashMap, HashSet, VecDeque};
use wg_2024::network::NodeId;
use wg_2024::packet::NodeType;

type Topology = UnGraphMap<NodeId, ()>;

pub struct CommunicationServerNetworkTopology {
    graph: Topology,
    node_types: HashMap<NodeId, NodeType>,
    saved_paths: HashMap<NodeId, Vec<NodeId>>, // target_node_id -> path
}

impl CommunicationServerNetworkTopology {
    pub fn new() -> Self {
        Self {
            graph: Topology::new(),
            node_types: HashMap::new(),
            saved_paths: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node_id: NodeId, node_type: NodeType) {
        if !self.graph.contains_node(node_id) {
            self.graph.add_node(node_id);
        }
        self.node_types
            .entry(node_id) // I assume the node type does not change
            .or_insert(node_type);
    }

    pub fn add_edge(&mut self, node_a: NodeId, node_b: NodeId) {
        if !self.graph.contains_edge(node_a, node_b) {
            self.graph.add_edge(node_a, node_b, ());
        }
    }

    pub fn remove_node(&mut self, node_id: NodeId) {
        self.graph.remove_node(node_id);
    }

    pub fn remove_edge(&mut self, node_a: NodeId, node_b: NodeId) {
        self.graph.remove_edge(node_a, node_b);
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

    pub fn update_node_type(&mut self, node_id: NodeId, node_type: NodeType) {
        self.node_types.insert(node_id, node_type);
    }

    pub fn contains_type(&self, node: &NodeId) -> bool {
        self.node_types.contains_key(node)
    }

    pub fn source_routing(&mut self, from: NodeId, to: NodeId) -> Option<Vec<NodeId>> {
        let destination_type = self.get_node_type(&to);
        if let Some(NodeType::Client) = destination_type {
            if self.saved_paths.contains_key(&to) {
                self.saved_paths.get(&to).cloned()
            } else {
                Some(self.bfs(from, to))
            }
        } else {
            None
        }
    }

    fn bfs(&mut self, from: NodeId, to: NodeId) -> Vec<NodeId> {
        // todo!: currently using a simple BFS
        // it could use "astar" with pdr as weight
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
                self.saved_paths.insert(to, route.clone());
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

impl Default for CommunicationServerNetworkTopology {
    fn default() -> CommunicationServerNetworkTopology {
        Self::new()
    }
}
