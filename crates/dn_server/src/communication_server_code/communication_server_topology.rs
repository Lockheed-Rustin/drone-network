//! This module manages the network topology for the communication server.
//! It provides methods for adding/removing nodes and edges, retrieving and updating node types,
//! and implementing source routing.
//!
//! The topology is represented as a graph where nodes represent network nodes, and edges represent
//! connections between nodes. The routing algorithm is used to find paths between nodes, and it
//! supports "saved paths" for faster routing.

use petgraph::graphmap::UnGraphMap;
use std::collections::{HashMap, HashSet, VecDeque};
use wg_2024::network::NodeId;
use wg_2024::packet::NodeType;

type Topology = UnGraphMap<NodeId, ()>;

/// A struct that represents the network topology of the communication server
pub struct CommunicationServerNetworkTopology {
    graph: Topology,
    node_types: HashMap<NodeId, NodeType>,
    saved_paths: HashMap<NodeId, Vec<NodeId>>, // target_node_id -> path
}

impl CommunicationServerNetworkTopology {
    /// Creates a new instance of `CommunicationServerNetworkTopology`.
    ///
    /// This function initializes an empty graph, an empty map for node types, and an empty map for
    /// saved paths.
    pub fn new() -> Self {
        Self {
            graph: Topology::new(),
            node_types: HashMap::new(),
            saved_paths: HashMap::new(),
        }
    }

    /// Adds a node to the network topology with the specified node ID and type.
    ///
    /// If the node does not already exist in the graph, it is added. The node type is also stored,
    /// but it is assumed that the type of the node will not change once set.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node to add.
    /// * `node_type` - The type of the node (e.g., client, drone, etc.).
    pub fn add_node(&mut self, node_id: NodeId, node_type: NodeType) {
        if !self.graph.contains_node(node_id) {
            self.graph.add_node(node_id);
        }
        self.node_types
            .entry(node_id)
            .or_insert(node_type);
    }

    /// Adds an edge between two nodes in the network topology.
    ///
    /// If the edge does not already exist, it is added to the graph.
    ///
    /// # Arguments
    /// * `node_a` - The ID of the first node.
    /// * `node_b` - The ID of the second node.
    pub fn add_edge(&mut self, node_a: NodeId, node_b: NodeId) {
        if !self.graph.contains_edge(node_a, node_b) {
            self.graph.add_edge(node_a, node_b, ());
        }
    }

    /// Removes a node from the network topology.
    ///
    /// The node and all of its associated edges are removed from the graph.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node to remove.
    pub fn remove_node(&mut self, node_id: NodeId) {
        self.graph.remove_node(node_id);
    }

    /// Removes an edge between two nodes in the network topology.
    ///
    /// # Arguments
    /// * `node_a` - The ID of the first node.
    /// * `node_b` - The ID of the second node.
    pub fn remove_edge(&mut self, node_a: NodeId, node_b: NodeId) {
        self.graph.remove_edge(node_a, node_b);
    }

    /// Retrieves the type of node from the network topology.
    ///
    /// If the node exists in the `node_types` map, its type is returned.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node whose type is to be retrieved.
    ///
    /// # Returns
    /// * `Option<&NodeType>` - The type of the node if it exists, `None` if the node is not found.
    pub fn get_node_type(&self, node_id: &NodeId) -> Option<&NodeType> {
        self.node_types.get(node_id)
    }

    /// Updates the type of existing node in the network topology.
    ///
    /// The node type is replaced with the new type provided.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node whose type is to be updated.
    /// * `node_type` - The new type to set for the node.
    pub fn update_node_type(&mut self, node_id: NodeId, node_type: NodeType) {
        self.node_types.insert(node_id, node_type);
    }

    /// Attempts to find a route from one node to another using source routing.
    ///
    /// If the destination node is a client, the function first checks if a saved path exists.
    /// If a saved path is available, it is returned. Otherwise, a new route is calculated using a
    /// Breadth-First Search (BFS) algorithm. If the destination node is not a client, `None` is returned.
    ///
    /// # Arguments
    /// * `from` - The ID of the source node.
    /// * `to` - The ID of the target node.
    ///
    /// # Returns
    /// * `Option<Vec<NodeId>>` - The list of nodes representing the route from `from` to `to`,
    ///    or `None` if no route is found.
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

    /// Performs a Breadth-First Search (BFS) to find the shortest path between two nodes.
    ///
    /// This method is used internally to compute the path from `from` to `to`. It also stores the
    /// resulting path in `saved_paths` to allow faster future lookups.
    ///
    /// # Arguments
    /// * `from` - The ID of the source node.
    /// * `to` - The ID of the target node.
    ///
    /// # Returns
    /// * `Vec<NodeId>` - The list of nodes representing the route from `from` to `to`, or an empty
    ///    vector if no route is found.
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

    #[cfg(test)]
    pub fn contains_node(&self, node_id: NodeId) -> bool {
        self.graph.contains_node(node_id)
    }

    #[cfg(test)]
    pub fn contains_edge(&self, node_a: NodeId, node_b: NodeId) -> bool {
        self.graph.contains_edge(node_a, node_b)
    }

    #[cfg(test)]
    pub fn contains_type(&self, node: &NodeId) -> bool {
        self.node_types.contains_key(node)
    }
}

impl Default for CommunicationServerNetworkTopology {
    fn default() -> CommunicationServerNetworkTopology {
        Self::new()
    }
}
