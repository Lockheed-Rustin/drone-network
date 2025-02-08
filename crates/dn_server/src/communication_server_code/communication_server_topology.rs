//! # `CommunicationServer`'s Network Topology
//! This module manages the network topology for the communication server.
//! It provides methods for adding/removing nodes and edges, retrieving and updating node types,
//! and implementing source routing.
//!
//! The topology is represented as a graph where nodes represent network nodes, and edges represent
//! connections between nodes. The routing algorithm is used to find paths between nodes, and it
//! supports "saved paths" for faster routing.

use petgraph::graphmap::UnGraphMap;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use wg_2024::network::NodeId;
use wg_2024::packet::NodeType;

type Topology = UnGraphMap<NodeId, ()>;

/// A struct that represents the network topology of the communication server
pub struct CommunicationServerNetworkTopology {
    graph: Topology,
    saved_paths: HashMap<NodeId, Vec<NodeId>>, // client_node_id -> path
    node_types: HashMap<NodeId, NodeType>,
    node_costs: HashMap<NodeId, u32>,
    lambda: f64,
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
            node_costs: HashMap::new(),
            lambda: 0.4, // 0.2 slow changes, 0.8 rapid adapting
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
        self.node_types.entry(node_id).or_insert(node_type);
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
        self.node_types.remove(&node_id);
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
    pub fn get_node_type(&self, node_id: NodeId) -> Option<&NodeType> {
        self.node_types.get(&node_id)
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

    /// Retrieves the cost associated with a given node.
    ///
    /// This function returns the stored cost of the specified `node_id`, which represents
    /// the estimated packet drop rate (PDR) in percentage (1-100). If no cost is found,
    /// it returns `None`.
    ///
    /// # Arguments
    /// * `node_id` - A reference to the ID of the node whose cost is requested.
    ///
    /// # Returns
    /// * `Option<u32>` - The cost value if it exists, otherwise `None`.
    pub fn get_node_cost(&self, node_id: NodeId) -> Option<u32> {
        self.node_costs.get(&node_id).copied()
    }

    /// Updates the cost of a given node.
    ///
    /// This function modifies the cost value associated with `node_id`, updating
    /// the node's weight in the routing algorithm.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node whose cost should be updated.
    /// * `cost` - The new cost value to assign.
    pub fn update_node_cost(&mut self, node_id: NodeId, cost: u32) {
        self.node_costs.insert(node_id, cost);
    }

    /// Updates the estimated packet drop rate (PDR) for a node based on NACK reception.
    ///
    /// This function adapts the node's estimated PDR using an exponential moving average (EMA).
    /// If a NACK is received (`nack_received` is `true`), it assumes a packet loss (PDR = 1.0).
    /// Otherwise, it assumes successful delivery (PDR = 0.0). The `lambda` parameter controls
    /// how much recent events influence the updated estimate.
    ///
    /// The computed PDR is converted into a cost metric (1-100) for routing purposes,
    /// where higher values indicate higher packet loss probability.
    ///
    /// # Arguments
    /// * `node_id` - The ID of the node whose PDR should be updated.
    /// * `nack_received` - A boolean indicating whether a NACK was received (`true` for loss,
    ///                    `false` for success).
    pub fn update_estimated_pdr(&mut self, node_id: NodeId, nack_received: bool) {
        let lambda = self.lambda;
        let old_pdr: f64 = f64::from(self.get_node_cost(node_id).unwrap_or(1)) / 100.0;
        let new_pdr = if nack_received { 1.0 } else { 0.0 };
        let updated_pdr = (1.0 - lambda) * old_pdr + lambda * new_pdr;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cost = (updated_pdr * 100.0).max(1.0).floor() as u32;
        self.update_node_cost(node_id, cost);
    }

    /// Retrieves a saved path for a given node.
    ///
    /// This function returns the previously computed path to `node_id`, if available.
    /// If no path is stored, it returns an empty vector.
    ///
    /// # Arguments
    /// * `node_id` - A reference to the ID of the destination node.
    ///
    /// # Returns
    /// * `Vec<NodeId>` - The saved path to the node, or an empty vector if no path is found.
    pub fn get_saved_path(&self, node_id: NodeId) -> Vec<NodeId> {
        let path = self.saved_paths.get(&node_id);
        match path {
            None => vec![],
            Some(v) => v.clone(),
        }
    }

    /// Saves a routing path for a given node.
    ///
    /// This function stores a path associated with a specific node ID in the `saved_paths` map.
    /// The saved path can later be used for routing or network optimization.
    ///
    /// # Arguments
    /// * `node_id` - The unique identifier of the node for which the path is being saved.
    /// * `path` - A vector of `NodeId` representing the sequence of nodes in the saved path.
    pub fn save_path(&mut self, node_id: NodeId, path: Vec<NodeId>) {
        self.saved_paths.insert(node_id, path);
    }

    /// Removes a saved path for a given node.
    ///
    /// This function deletes the stored path associated with the specified `node_id`,
    /// ensuring that outdated or invalid paths are not used in future routing decisions.
    ///
    /// # Arguments
    /// * `node_id` - A reference to the ID of the node whose path should be removed.
    pub fn remove_path(&mut self, node_id: NodeId) {
        self.saved_paths.remove(&node_id);
    }

    /// Attempts to find a route from one node to another using source routing.
    ///
    /// If the destination node is a client, the function first checks if a saved path exists.
    /// If a saved path is available, it is returned. Otherwise, a new route is calculated using a
    /// Dijkstra algorithm. If the destination node is not a client, `None` is returned.
    ///
    /// # Arguments
    /// * `from` - The ID of the source node.
    /// * `to` - The ID of the target node.
    ///
    /// # Returns
    /// * `Option<Vec<NodeId>>` - The list of nodes representing the route from `from` to `to`,
    ///    or `None` if `destination_type` was not Client. It returns an empty vec if the node `to`
    ///    is not known yet.
    pub fn source_routing(&mut self, from: NodeId, to: NodeId) -> Option<Vec<NodeId>> {
        let destination_type = self.get_node_type(to);
        if let Some(nt) = destination_type {
            match nt {
                NodeType::Client => {
                    if self.saved_paths.contains_key(&to) {
                        self.saved_paths.get(&to).cloned()
                    } else {
                        Some(self.dijkstra(from, to))
                    }
                }
                _ => None,
            }
        } else {
            Some(vec![])
        }
    }

    /// Finds the shortest path (min cost) between two nodes using Dijkstra's Algorithm.
    ///
    /// This function considers the "cost" of each node when finding the best path.
    ///
    /// # Arguments
    /// * `from` - The starting node.
    /// * `to` - The destination node.
    ///
    /// # Returns
    /// * `Vec<NodeId>` - The optimal path with the lowest cost, or an empty vector if no path exists.
    fn dijkstra(&mut self, from: NodeId, to: NodeId) -> Vec<NodeId> {
        let mut distances: HashMap<NodeId, u32> = HashMap::new();
        let mut parent_map: HashMap<NodeId, NodeId> = HashMap::new();
        let mut priority_queue = BinaryHeap::new();

        distances.insert(from, 0);
        priority_queue.push(State {
            cost: 0,
            node: from,
        });

        while let Some(State { cost, node }) = priority_queue.pop() {
            if node == to {
                // Path found, reconstruct the route
                let mut route = Vec::new();
                let mut current = to;
                while let Some(&parent) = parent_map.get(&current) {
                    route.push(current);
                    current = parent;
                }
                route.push(from);
                route.reverse();
                self.save_path(to, route.clone());
                return route;
            }

            // Explore neighbors
            for neighbor in self.graph.neighbors(node) {
                if neighbor != to && neighbor != from {
                    if let Some(node_type) = self.node_types.get(&neighbor) {
                        if *node_type != NodeType::Drone {
                            continue; // I avoid passing through nodes that are not drones
                        }
                    }
                }

                let node_cost = *self.node_costs.get(&neighbor).unwrap_or(&1);
                let new_cost = cost + node_cost;

                if new_cost < *distances.get(&neighbor).unwrap_or(&u32::MAX) {
                    distances.insert(neighbor, new_cost);
                    parent_map.insert(neighbor, node);
                    priority_queue.push(State {
                        cost: new_cost,
                        node: neighbor,
                    });
                }
            }
        }

        vec![] // No path found
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

/// A struct used for ordering nodes in the priority queue for Dijkstra.
#[derive(Copy, Clone, Eq, PartialEq)]
struct State {
    cost: u32,
    node: NodeId,
}

// Implement ordering so BinaryHeap acts as a min-heap
impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.cmp(&self.cost)
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;
    use std::collections::BinaryHeap;

    #[test]
    fn test_source_routing() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;

        let route = server.network_topology.source_routing(server.id, 1);

        assert_eq!(route, None);

        let route = server.network_topology.source_routing(server.id, 3);

        assert_eq!(route, None);

        let route = server
            .network_topology
            .source_routing(server.id, 6)
            .expect("Error in routing");

        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 6);

        let route = server
            .network_topology
            .source_routing(server.id, 4)
            .expect("Error in routing");
        // should avoid passing through 5 because it's a client
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 7);
        assert_eq!(route[3], 4);

        let helper = TestServerHelper::new();
        let mut server = helper.server;
        server
            .network_topology
            .update_node_type(7, NodeType::Server);
        let route = server
            .network_topology
            .source_routing(server.id, 4)
            .expect("Error in routing");
        // should avoid passing through 5 because it's a client, but also through 7 because it's a
        // server. So the path is empty
        assert!(route.is_empty());

        server.network_topology.update_node_type(5, NodeType::Drone);
        server.network_topology.update_node_type(7, NodeType::Drone);
        // should pass through 5 now because it's a drone and the path is shorter than the one passing
        // through 7
        let route = server
            .network_topology
            .source_routing(server.id, 4)
            .expect("Error in routing");
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 5);
        assert_eq!(route[2], 4);

        // simulating an impossible topology
        // Server_1 <---> Drone_3 <---> Client_6 <---> Drone_69 <---> target-Client_70
        server.network_topology.add_node(69, NodeType::Drone);
        server.network_topology.add_node(70, NodeType::Client);
        server.network_topology.add_edge(6, 69);
        server.network_topology.add_edge(70, 69);
        let route = server
            .network_topology
            .source_routing(server.id, 70)
            .expect("Error in routing");
        assert!(route.is_empty());

        let helper = TestServerHelper::new();
        let mut server = helper.server;

        let route = server
            .network_topology
            .source_routing(server.id, 4)
            .expect("Error in routing");

        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 7);
        assert_eq!(route[3], 4);

        server.network_topology.update_node_type(5, NodeType::Drone);
        let route = server
            .network_topology
            .source_routing(server.id, 4)
            .expect("Error in routing");

        // even if the path through 5 is shorter, it will use the one through 7 because is saved
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 7);
        assert_eq!(route[3], 4);
    }

    #[test]
    fn test_source_routing_empty_vec_cases() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;

        server.network_topology.remove_node(6);
        let route = server
            .network_topology
            .source_routing(server.id, 6)
            .expect("Error in routing");
        assert_eq!(route, vec![]);
    }

    #[test]
    fn test_min_priority_queue() {
        let mut priority_queue = BinaryHeap::new();
        priority_queue.push(State { cost: 10, node: 1 });
        priority_queue.push(State { cost: 5, node: 2 });
        priority_queue.push(State { cost: 15, node: 3 });
        let min_state = priority_queue.pop();
        assert_eq!(min_state.unwrap().cost, 5);
        assert_eq!(min_state.unwrap().node, 2);
    }

    #[test]
    fn test_dijkstra() {
        let init_topology = |topology: &mut CommunicationServerNetworkTopology| {
            topology.add_node(1, NodeType::Server);
            topology.add_node(3, NodeType::Drone);
            topology.add_node(7, NodeType::Drone);
            topology.add_node(5, NodeType::Drone);
            topology.add_node(4, NodeType::Client);

            topology.add_edge(3, 7);
            topology.add_edge(7, 4);
            topology.add_edge(4, 5);
            topology.add_edge(1, 5);
            topology.add_edge(3, 1);
        };
        // 1)
        let mut topology = CommunicationServerNetworkTopology::new();
        init_topology(&mut topology);
        topology.update_node_cost(5, 10);

        let route = topology.dijkstra(1, 4);
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 7);
        assert_eq!(route[3], 4);

        // 2)
        let mut topology = CommunicationServerNetworkTopology::new();
        init_topology(&mut topology);
        topology.update_node_cost(5, 10);
        topology.update_node_cost(3, 10);

        let route = topology.dijkstra(1, 4);
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 5);
        assert_eq!(route[2], 4);
    }

    #[test]
    fn test_update_pdr() {
        let mut t = CommunicationServerNetworkTopology::new();
        t.update_estimated_pdr(5, true);
        let cost = t.node_costs.get(&5).cloned().unwrap();
        assert_eq!(cost, ((0.006 + 0.4) * 100.0) as u32); // 40

        t.update_estimated_pdr(5, true);
        let cost = t.node_costs.get(&5).cloned().unwrap();
        assert_eq!(cost, ((0.24 + 0.4) * 100.0) as u32); // 64

        t.update_estimated_pdr(5, true);
        let cost = t.node_costs.get(&5).cloned().unwrap();
        assert_eq!(cost, ((0.384 + 0.4) * 100.0) as u32); // 78

        t.update_estimated_pdr(5, false);
        let cost = t.node_costs.get(&5).cloned().unwrap();
        assert_eq!(cost, ((0.468 + 0.0) * 100.0) as u32); // 46

        t.update_estimated_pdr(5, false);
        let cost = t.node_costs.get(&5).cloned().unwrap();
        assert_eq!(cost, ((0.276 + 0.0) * 100.0) as u32); // 27
    }
}
