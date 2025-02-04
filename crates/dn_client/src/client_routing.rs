use std::collections::{HashMap, HashSet};
use petgraph::data::Build;
use petgraph::prelude::UnGraphMap;

use std::collections::BinaryHeap;
use std::cmp::Reverse;
use std::cmp::Ordering;
use wg_2024::network::{NodeId};
use wg_2024::packet::NodeType;

/// TODO: check fn x calcolo path minimo


//---------- CUSTOM TYPES ----------//
type Path = Vec<NodeId>;
type FloodPath = Vec<(NodeId, NodeType)>;
static K: f64 = 0.8;


//---------- QUEUE PRIO TYPE ----------//
#[derive(Copy, Clone, Debug)]

struct QP {
    prio: f64,
}

impl QP {
    pub fn new(prio: f64) -> Self {
        Self {
            prio,
        }
    }
}

impl PartialEq for QP {
    fn eq(&self, other: &Self) -> bool {
        self.prio == other.prio
    }
}

impl Eq for QP {}

impl PartialOrd for QP {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QP {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.prio.is_nan() && other.prio.is_nan() {
            Ordering::Equal
        } else if self.prio.is_nan() {
            Ordering::Greater
        } else if other.prio.is_nan() {
            Ordering::Less
        } else {
            self.prio.total_cmp(&other.prio)
        }
    }
}




//---------- ERROR'S ENUMS ----------//
pub enum SourceRoutingError {
    DestinationNotFound,
    UnreachableDestination,
}



//---------- STRUCT SERVER INFO ----------//
pub struct ServerInfo {
    path: Path,
    reachable: bool,
    packet_exchanged: u64,
    path_weight: f64,
}

impl Default for ServerInfo {
    fn default() -> Self {
        Self {
            path: Vec::new(),
            reachable: false,
            packet_exchanged: 0, //since last update of paths to servers
            path_weight: 1.0,
        }
    }
}

impl ServerInfo {

    pub fn inc_packet_exchanged(&mut self) {
        self.packet_exchanged += 1;
    }
}



//---------- STRUCT DRONE INFO ----------//
#[derive(Default)]

pub struct DroneInfo {
    packet_received: u64,
    packet_dropped: u64,
}

impl DroneInfo {

    pub fn get_pdr(&self) -> f64 {
        if self.packet_received == 0 {0.0}
        else {(self.packet_dropped as f64)/(self.packet_received as f64)}
    }

    pub fn inc_corret_send(&mut self) {
        self.packet_received += 1;
    }

    pub fn inc_dropped(&mut self) {
        self.packet_received += 1;
        self.packet_dropped += 1;
    }
}



//---------- CLIENT'S SOURCE ROUTING ----------//
pub struct ClientRouting {
    client_id: NodeId,
    topology: UnGraphMap<NodeId, f64>,
    servers_info: HashMap<NodeId, ServerInfo>,
    drones_info: HashMap<NodeId, DroneInfo>,
    clients: HashSet<NodeId>,
}

impl ClientRouting {
    pub fn new(client_id: NodeId) -> Self {
        let mut topology: UnGraphMap::<NodeId, f64> =  UnGraphMap::new();
        topology.add_node(client_id);

        let mut clients: HashSet<NodeId> = HashSet::new();
        clients.insert(client_id);

        Self {
            client_id,
            topology,
            servers_info: HashMap::new(),
            drones_info: HashMap::new(),
            clients,
        }
    }


    //---------- topology modifier ----------//
    pub fn reset_topology(&mut self) {
        self.topology.clear();
        self.topology.add_node(self.client_id);

        for (_, server_info) in self.servers_info.iter_mut() {
            server_info.reachable = false;
        }

    }

    pub fn remove_channel_to_neighbor(&mut self, neighbor: NodeId)  {
        if self.topology.remove_edge(self.client_id, neighbor).is_some() {
            self.compute_routing_table();

            for (_, server_info) in self.servers_info.iter_mut() {
                if server_info.path.len() >= 2 && server_info.path.contains(&neighbor) {
                    server_info.reachable = false;
                }
            }
        }
    }

    pub fn add_channel_to_neighbor(&mut self, neighbor: NodeId)  {
        if self.topology.add_edge(self.client_id, neighbor, 1.0).is_none() {
            self.compute_routing_table();

            for (_, server_info) in self.servers_info.iter_mut() {
                if server_info.path.len() >= 2 && server_info.path.contains(&neighbor) {
                    server_info.reachable = false;
                }
            }
        }
    }

    pub fn remove_drone(&mut self, drone: NodeId)  {
        if self.topology.remove_node(drone) {
            self.compute_routing_table();

            for (_, server_info) in self.servers_info.iter_mut() {
                if server_info.path.contains(&drone) {
                    server_info.reachable = false;
                }
            }
        }
    }

    pub fn add_path(&mut self, path: FloodPath) -> Option<Vec<NodeId>>  {
        //check if path is empty and
        let mut iter = path.iter();
        let mut last = match iter.next() {
            Some(&(node, _)) => node, //first node this case (client itself)
            None => return None, //Case empty path
        };

        for &(node, node_type) in iter {
            //add new nodes to topology
            if !self.topology.contains_node(node) {
                self.topology.add_node(node);
                match &node_type {
                    NodeType::Drone => {
                        self.drones_info.entry(node).or_default();
                    }
                    NodeType::Server => {
                        self.servers_info.entry(node).or_default();
                    }
                    NodeType::Client => {
                        self.clients.insert(node);
                    }
                }
            }

            //add new edges to topology
            if !self.topology.contains_edge(node, last) {
                self.topology.add_edge(node, last, 1.0);
            }
            last = node;
        }

        self.compute_routing_table()
    }

    fn reset_weights(&mut self) {
        for (_, _, edge_weight) in self.topology.all_edges_mut() {
            *edge_weight = 1.0;
        }
    }

    /// Add given weight to given path, except the edge between client and the first node.
    ///
    /// Return if it isn't at least 2 nodes, cause there can be a path.
    ///
    /// Don't increment the weight of edge between client itself and the first node,
    /// since this should generate some unwanted case where the chosen path doesn't improve the
    /// topology's performance, but it makes them worse.
    fn add_weight_to_path(&mut self, path: &Path, weight: f64) {
        let mut iter = path.iter();

        if iter.next().is_none() {
            return; //path empty
        };

        let mut last = match iter.next() {
            Some(&node) => node,
            None => return, //Case path = [client]
        };

        for &node in iter {
            if let Some(edge_weight) = self.topology.edge_weight(node, last) {
                self.topology.update_edge(node, last, edge_weight+weight);
            };
            last = node;
        }
    }


    //---------- compute source routing ----------//

    /// Returns path to server as Vec<NodeId>.
    ///
    /// If the destination server doesn't exist in the topology or the server os actually unreachable,
    /// returns an appropriate error
    pub fn get_path(&self, destination: NodeId) -> Result<Path, SourceRoutingError> {
        match self.servers_info.get(&destination) {
            Some(server_info) => {
                if server_info.reachable {
                    Ok(server_info.path.clone())
                }
                else {
                    Err(SourceRoutingError::UnreachableDestination)
                }
            }
            None => {
                Err(SourceRoutingError::DestinationNotFound)
            }
        }
    }

    /// Update the information about path from client to the connected servers
    ///
    /// Return an option to a list of servers which became reachable after updating their routing paths.
    fn compute_routing_table(&mut self) -> Option<Vec<NodeId>> {

        let mut servers_became_reachable: Vec<NodeId> = Vec::new();

        if self.servers_info.is_empty(){
            return None; //No server in the topology
        }

        self.reset_weights();

        let mut total_packet = 0;
        for (_, info) in self.servers_info.iter_mut() {
            total_packet += info.packet_exchanged;
        }
        let mean = (total_packet as f64) / (self.servers_info.len() as f64);

        //ord servers: heaviest "path" first
        let mut ord_vec: BinaryHeap<(QP, NodeId)> = BinaryHeap::new();
        for (server, info) in self.servers_info.iter_mut() {
            info.path_weight = ((info.packet_exchanged as f64 / mean) + (K * info.path_weight)) / (1.0+K);
            ord_vec.push((QP::new(info.path_weight), *server));
            info.packet_exchanged = 0;
        }

        //compute single paths
        for (_, server) in ord_vec {
            if let Some(path) = self.compute_path_to_server(server) {
                let mut weight = 0.0;
                if let Some(server_info) = self.servers_info.get_mut(&server) {
                    weight = server_info.path_weight;

                    server_info.path = path.clone();

                    if !server_info.reachable {
                        server_info.reachable = true;
                        servers_became_reachable.push(server);
                    }
                }

                if weight != 0.0 {
                    self.add_weight_to_path(&path, weight);
                }
            }
            else if let Some(server_info) = self.servers_info.get_mut(&server) {
                server_info.reachable = false;
            }
        }

        if servers_became_reachable.is_empty() {
            None
        }
        else {
            Some(servers_became_reachable)
        }
    }

    /// Returns the path to destination with the respective weight if exists, None otherwise.
    fn compute_path_to_server(&self, destination: NodeId) -> Option<Path> {
        let mut queue: BinaryHeap<(Reverse<QP>, NodeId)> = BinaryHeap::new();
        queue.push((Reverse(QP::new(0.0)), self.client_id));

        let mut distances: HashMap<NodeId, (NodeId, f64)> = HashMap::new(); //node_id -> (pred_id, node_distance)
        let mut visited: HashSet<NodeId> = HashSet::new();

        while !visited.contains(&destination) && !queue.is_empty() {
            if let Some(&(Reverse(qp), node)) = queue.peek() {
                let distance = qp.prio;
                if !visited.contains(&node) {
                    visited.insert(node);

                    if node != destination {
                        for neighbor in self.topology.neighbors(node) {
                            //if neighbor it's not visited yet && it's not a client && it's not a server or it's the destination
                            if !visited.contains(&neighbor)
                                &&!self.clients.contains(&neighbor)
                                && (!self.servers_info.contains_key(&neighbor) || neighbor == destination)
                            {
                                if let Some(edge_weight) = self.topology.edge_weight(node, neighbor) {
                                    if let Some(drone_info) = self.drones_info.get(&neighbor) {
                                        let total_distance = (distance + edge_weight) * (1.0 + drone_info.get_pdr());
                                        queue.push((Reverse(QP::new(total_distance)), neighbor));

                                        if let Some(&(_, pred_weight)) = distances.get(&neighbor) {
                                            if pred_weight > total_distance {
                                                distances.insert(neighbor, (node, total_distance));
                                            }
                                        }
                                        else {
                                            distances.insert(neighbor, (node, total_distance));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if visited.contains(&destination) {
            let mut path: Path = Vec::new();
            path.push(destination);
            let mut last = destination;

            while last != self.client_id {
                if let Some(&(pred, _)) = distances.get(&last) {
                    path.push(pred);
                    last = pred;
                }
            }
            path.reverse();

            Some(path)
        }
        else {
            None
        }
    }
}