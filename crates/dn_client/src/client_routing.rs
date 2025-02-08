use petgraph::prelude::UnGraphMap;
use std::collections::{HashMap, HashSet};

use std::cmp::Ordering;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use wg_2024::network::NodeId;
use wg_2024::packet::NodeType;

//---------- CUSTOM TYPES ----------//
type Path = Vec<NodeId>;
type FloodPath = Vec<(NodeId, NodeType)>;

//---------- QUEUE PRIO TYPE ----------//
#[derive(Copy, Clone, Debug)]
struct QP {
    prio: f64,
}

impl QP {
    pub fn new(prio: f64) -> Self {
        Self { prio }
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

//---------- STRUCT SERVER INFO ----------//
#[derive(Debug, Default)]
pub struct ServerInfo {
    path: Path,
    reachable: bool,
}

//---------- STRUCT DRONE INFO ----------//
#[derive(Default, Debug)]
pub struct DroneInfo {
    packet_traveled: u64,
    packet_dropped: u64,
}

impl DroneInfo {
    /// `rps_factor`: real packet sent factor
    /// Returns the estimated number of packet to send for every packet which has been delivered
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn rps_factor(&self) -> f64 {
        if self.packet_traveled == 0 || self.packet_dropped == 0 {
            1.0
        } else {
            let pdr = (self.packet_dropped as f64) / (self.packet_traveled as f64);

            let mut rps = 0.0;
            for i in 0..=10 {
                rps += pdr.powi(i);
            }

            rps
        }
    }

    pub fn inc_correct_traveled(&mut self) {
        self.packet_traveled += 1;
    }

    pub fn inc_dropped(&mut self) {
        self.packet_traveled += 1;
        self.packet_dropped += 1;
    }
}

//---------- CLIENT'S SOURCE ROUTING ----------//
pub struct ClientRouting {
    client_id: NodeId,
    topology: UnGraphMap<NodeId, ()>,
    servers_info: HashMap<NodeId, ServerInfo>,
    drones_info: HashMap<NodeId, DroneInfo>,
    clients: HashSet<NodeId>,
}

impl ClientRouting {
    #[must_use]
    pub fn new(client_id: NodeId) -> Self {
        let mut topology: UnGraphMap<NodeId, ()> = UnGraphMap::new();
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
    pub fn clear_topology(&mut self) {
        self.topology.clear();
        self.topology.add_node(self.client_id);

        for server_info in self.servers_info.values_mut() {
            server_info.reachable = false;
        }
    }

    pub fn remove_channel_to_neighbor(&mut self, neighbor: NodeId) {
        if self
            .topology
            .remove_edge(self.client_id, neighbor)
            .is_some()
        {
            if self.topology.neighbors(neighbor).next().is_none() {
                self.topology.remove_node(neighbor);
            }

            self.compute_routing_paths();
        }
    }

    pub fn add_channel_to_neighbor(&mut self, neighbor: NodeId) -> Option<Vec<(NodeId, Path)>> {
        if !self.topology.contains_node(neighbor) {
            self.topology.add_node(neighbor);
            self.drones_info.insert(neighbor, DroneInfo::default());
        }

        if self
            .topology
            .add_edge(self.client_id, neighbor, ())
            .is_none()
        {
            //If new edge is added
            self.compute_routing_paths()
        } else {
            //nothing changed: no need to recompute path
            None
        }
    }

    pub fn remove_node(&mut self, node: NodeId) {
        if self.topology.remove_node(node) {
            self.drones_info.remove(&node);

            self.compute_routing_paths();
        }
    }

    pub fn add_path(&mut self, path: &FloodPath) -> Option<Vec<(NodeId, Path)>> {
        //check if path is empty and
        let mut iter = path.iter();

        match &iter.next() {
            None => None,

            Some((mut last, _)) => {
                let mut something_changed = false;
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
                        something_changed = true;
                    }

                    //add new edges to topology
                    if !self.topology.contains_edge(node, last) {
                        self.topology.add_edge(node, last, ());
                        something_changed = true;
                    }
                    last = node;
                }

                //no need to recompute path if nothing has been changed
                if something_changed {
                    self.compute_routing_paths()
                } else {
                    None
                }
            }
        }
    }

    //---------- update info on packet exchanged ----------//
    pub fn correct_send_to(&mut self, server: NodeId) {
        if let Some(server_info) = self.servers_info.get(&server) {
            self.correct_exchanged_with(&server_info.path.clone());
        }
    }

    pub fn correct_exchanged_with(&mut self, path: &Path) {
        for drone in path {
            if let Some(drone_info) = self.drones_info.get_mut(drone) {
                drone_info.inc_correct_traveled();
            }
        }
    }

    pub fn inc_packet_dropped(&mut self, path: &Path) {
        let mut iter = path.iter();

        if let Some(drone) = iter.next() {
            if let Some(drone_info) = self.drones_info.get_mut(drone) {
                drone_info.inc_dropped();
            }
        }

        for drone in iter {
            if let Some(drone_info) = self.drones_info.get_mut(drone) {
                drone_info.inc_correct_traveled();
            }
        }

        self.compute_routing_paths();
    }

    //---------- compute source routing ----------//
    /// Returns path to server as Vec<NodeId>.
    ///
    /// If the destination server doesn't exist in the topology or the server os actually unreachable,
    /// returns an appropriate error
    #[must_use]
    pub fn get_path(&self, destination: NodeId) -> Option<Path> {
        match self.servers_info.get(&destination) {
            Some(server_info) if server_info.reachable => Some(server_info.path.clone()),
            _ => None,
        }
    }

    /// Update the information about path from client to the connected servers
    ///
    /// Return an option to a list of servers which became reachable after updating their routing paths.
    pub fn compute_routing_paths(&mut self) -> Option<Vec<(NodeId, Path)>> {
        if self.servers_info.is_empty() {
            return None; //No server in the topology
        }

        let mut servers_became_reachable: Vec<(NodeId, Path)> = Vec::new();

        //init
        let mut queue: BinaryHeap<(Reverse<QP>, NodeId)> = BinaryHeap::new();
        queue.push((Reverse(QP::new(0.0)), self.client_id));

        let mut distances: HashMap<NodeId, (NodeId, f64)> = HashMap::new(); //node_id -> (pred_id, node_distance)
        distances.insert(self.client_id, (self.client_id, 0.0));
        let mut visited: HashSet<NodeId> = HashSet::new();

        //search the shortest path
        while !queue.is_empty() {
            if let Some((Reverse(qp), node)) = queue.pop() {
                let mut distance = qp.prio;
                if !visited.contains(&node) {
                    visited.insert(node);

                    if !self.servers_info.contains_key(&node) {
                        for neighbor in self.topology.neighbors(node) {
                            //if neighbor it's not visited yet && it's not a client
                            if !visited.contains(&neighbor) && !self.clients.contains(&neighbor) {
                                distance += 1.0;
                                if let Some(drone_info) = self.drones_info.get(&neighbor) {
                                    distance *= drone_info.rps_factor();
                                }

                                queue.push((Reverse(QP::new(distance)), neighbor));

                                distances
                                    .entry(neighbor)
                                    .and_modify(|e| {
                                        if e.1 > distance {
                                            *e = (node, distance);
                                        }
                                    })
                                    .or_insert((node, distance));
                            }
                        }
                    }
                }
            }
        }

        //compute single path for every server
        for (&server, server_info) in &mut self.servers_info {
            if visited.contains(&server) {
                let mut path: Path = Vec::new();
                path.push(server);
                let mut last = server;

                let mut pathable = true;

                while last != self.client_id && pathable {
                    if let Some((pred, _)) = distances.get(&last) {
                        path.push(*pred);
                        last = *pred;
                    } else {
                        pathable = false;
                    }
                }

                if pathable {
                    path.reverse();

                    server_info.path.clone_from(&path);

                    if !server_info.reachable {
                        server_info.reachable = true;
                        servers_became_reachable.push((server, path.clone()));
                    }
                }
            } else {
                server_info.reachable = false;
            }
        }

        if servers_became_reachable.is_empty() {
            None
        } else {
            Some(servers_became_reachable)
        }
    }
}

//---------------------------//
//---------- TESTS ----------//
//---------------------------//
#[cfg(test)]
mod tests {
    use super::*;
    use wg_2024::packet::NodeType::{Client, Drone, Server};

    //---------- DRONE INFO TEST ----------//
    #[test]
    fn drone_info_test() {
        //---------- init ----------//
        let mut drone_info = DroneInfo::default();

        assert_eq!(drone_info.packet_dropped, 0);
        assert_eq!(drone_info.packet_traveled, 0);

        //---------- functions ----------//
        assert_eq!(drone_info.rps_factor(), 1.0);

        drone_info.inc_correct_traveled();
        assert_eq!(drone_info.rps_factor(), 1.0);

        for _ in 0..8 {
            drone_info.inc_correct_traveled();
        }
        drone_info.inc_dropped();

        assert_eq!(drone_info.packet_traveled, 10);
        assert_eq!(drone_info.packet_dropped, 1);

        assert_eq!(drone_info.rps_factor(), 1.1111111111);
    }

    //---------- SERVER INFO TEST ----------//
    #[test]
    fn server_info_test() {
        //---------- init ----------//
        let server_info = ServerInfo::default();

        assert!(server_info.path.is_empty());
        assert!(!server_info.reachable);
    }

    //---------- CLIENT ROUTING TEST ----------//

    #[test] //---------- INIT & TOPOLOGY MODIFIER ----------//
    fn client_routing_test_part1() {
        /*
        topologia con 4 nodi: 1(Client), 2(Drone), 3(Drone), 4(Server)
        paths: 1-2-4, 1-3-4
        routing path: 1-3-4 -> edge_weight(3,4) = 2.0
        */

        //---------- init ----------//
        let mut client_routing = ClientRouting::new(1);

        assert_eq!(client_routing.client_id, 1);

        assert!(client_routing.drones_info.is_empty());
        assert!(client_routing.servers_info.is_empty());

        assert!(!client_routing.clients.is_empty());
        assert!(client_routing.clients.contains(&1));

        assert_eq!(client_routing.topology.node_count(), 1);
        assert!(client_routing.topology.contains_node(1));

        //---------- add channel to neighbor ----------//
        client_routing.add_channel_to_neighbor(2);
        client_routing.add_channel_to_neighbor(3);

        assert_eq!(client_routing.topology.node_count(), 3);

        assert!(client_routing.topology.contains_node(2));
        assert!(client_routing.topology.contains_edge(1, 2));

        assert!(client_routing.topology.contains_node(3));
        assert!(client_routing.topology.contains_edge(1, 3));

        assert!(client_routing.drones_info.contains_key(&2));
        assert!(client_routing.drones_info.contains_key(&3));

        //---------- add path ----------//
        let path1: FloodPath = vec![(1, Client), (2, Drone), (4, Server)];
        let path2: FloodPath = vec![(1, Client), (3, Drone), (4, Server)];
        client_routing.add_path(&path1);
        client_routing.add_path(&path2);

        assert_eq!(client_routing.topology.node_count(), 4);

        assert!(client_routing.topology.contains_node(4));

        assert!(client_routing.topology.contains_edge(2, 4));

        assert!(client_routing.topology.contains_edge(3, 4));

        assert!(client_routing.servers_info.contains_key(&4));

        //---------- remove channel to neighbor ----------//
        client_routing.remove_channel_to_neighbor(2);

        assert!(!client_routing.topology.contains_edge(1, 2));

        //---------- remove drone ----------//
        client_routing.remove_node(2);

        assert_eq!(client_routing.topology.node_count(), 3);

        assert!(!client_routing.topology.contains_node(2));
        assert!(!client_routing.topology.contains_edge(2, 4));

        //---------- reset topology ----------//
        client_routing.clear_topology();

        assert_eq!(client_routing.topology.node_count(), 1);
        assert!(client_routing.topology.contains_node(1));
    }

    #[test] //---------- UPDATE INFO PACKED EXCHANGED ----------//
    fn client_routing_test_part2() {
        /*
        topologia con 6 nodi: 1(Client), 2(Drone), 3(Drone), 4(Drone), 5(Drone), 6(Server)
        paths: 1-2-3-6, 1-4-5-6
        */

        let mut client_routing = ClientRouting::new(1);
        let path1: FloodPath = vec![(1, Client), (2, Drone), (3, Drone), (6, Server)];
        let path2: FloodPath = vec![(1, Client), (4, Drone), (5, Drone), (6, Server)];
        client_routing.add_path(&path1);
        client_routing.add_path(&path2);

        let good_path = vec![1, 2, 3, 6];
        if let Some(server_info) = client_routing.servers_info.get_mut(&6) {
            server_info.path = good_path;
        }
        let dropped_path = vec![5, 4, 1];

        //---------- correct exchanged ----------//
        client_routing.correct_send_to(6);

        assert_eq!(
            client_routing.drones_info.get(&2).unwrap().packet_traveled,
            1
        );
        assert_eq!(
            client_routing.drones_info.get(&2).unwrap().packet_dropped,
            0
        );

        assert_eq!(
            client_routing.drones_info.get(&3).unwrap().packet_traveled,
            1
        );
        assert_eq!(
            client_routing.drones_info.get(&3).unwrap().packet_dropped,
            0
        );

        //---------- packet dropped ----------//
        client_routing.inc_packet_dropped(&dropped_path);

        assert_eq!(
            client_routing.drones_info.get(&5).unwrap().packet_traveled,
            1
        );
        assert_eq!(
            client_routing.drones_info.get(&5).unwrap().packet_dropped,
            1
        );

        assert_eq!(
            client_routing.drones_info.get(&4).unwrap().packet_traveled,
            1
        );
        assert_eq!(
            client_routing.drones_info.get(&4).unwrap().packet_dropped,
            0
        );
    }

    #[test] //---------- COMPUTE ROUTING ----------//
    fn client_routing_test_part3() {
        /*
        topologia con 8 nodi: 1(Client), 2(Drone), 3(Drone), 4(Drone), 5(Drone), 6(Server), 7(Server), 8(IsolatedServer)
        paths: 1-2-3-6, 1-4-5-6, 1-2-3-7, 1-4-5-7;      path to became server 8 reachable 1-4-5-8
        drones: pkt_traveled -> 100;    pkt_dropped -> 2(5), 3(10), 4(15), 5(20)
        servers: pkt_exchanged: -> 6(160), 7(40), 8(100)
        */

        let mut client_routing = ClientRouting::new(1);
        let path1: FloodPath = vec![(1, Client), (2, Drone), (3, Drone), (6, Server)];
        let path2: FloodPath = vec![(1, Client), (4, Drone), (5, Drone), (6, Server)];
        let path3: FloodPath = vec![(1, Client), (2, Drone), (3, Drone), (7, Server)];
        let path4: FloodPath = vec![(1, Client), (4, Drone), (5, Drone), (7, Server)];
        client_routing.add_path(&path1);
        client_routing.add_path(&path2);
        client_routing.add_path(&path3);
        client_routing.add_path(&path4);

        //add fake server to test case unreachable
        client_routing.topology.add_node(8);
        client_routing.servers_info.insert(8, ServerInfo::default());

        let mut drone_info;
        drone_info = client_routing.drones_info.get_mut(&2).unwrap();
        drone_info.packet_traveled = 100;
        drone_info.packet_dropped = 5;
        drone_info = client_routing.drones_info.get_mut(&3).unwrap();
        drone_info.packet_traveled = 100;
        drone_info.packet_dropped = 10;
        drone_info = client_routing.drones_info.get_mut(&4).unwrap();
        drone_info.packet_traveled = 100;
        drone_info.packet_dropped = 15;
        drone_info = client_routing.drones_info.get_mut(&5).unwrap();
        drone_info.packet_traveled = 100;
        drone_info.packet_dropped = 20;

        //---------- check pre-test ----------//
        assert!(client_routing.topology.contains_node(8));
        assert_eq!(client_routing.topology.node_count(), 8);

        assert!(client_routing.servers_info.contains_key(&8));
        assert_eq!(client_routing.servers_info.len(), 3);

        //---------- compute routing paths ----------//
        client_routing.compute_routing_paths();
        let mut server_info;

        server_info = client_routing.servers_info.get(&6).unwrap();
        assert!(server_info.reachable);
        assert_eq!(server_info.path, vec![1, 2, 3, 6]);

        server_info = client_routing.servers_info.get(&7).unwrap();
        assert!(server_info.reachable);
        assert_eq!(server_info.path, vec![1, 2, 3, 7]);

        server_info = client_routing.servers_info.get(&8).unwrap();
        assert!(!server_info.reachable);

        //---------- get path ----------//
        assert_eq!(client_routing.get_path(6).unwrap(), vec![1, 2, 3, 6]);
        assert_eq!(client_routing.get_path(7).unwrap(), vec![1, 2, 3, 7]);
        assert!(client_routing.get_path(8).is_none()); //server unreachable
        assert!(client_routing.get_path(9).is_none()); //server doesn't exist

        //---------- test became reachable ----------//
        let path5: FloodPath = vec![(1, Client), (4, Drone), (5, Drone), (8, Server)];
        let servers_became_reachable = client_routing.add_path(&path5).unwrap();

        assert_eq!(servers_became_reachable[0].0, 8);
        assert_eq!(servers_became_reachable[0].1, vec![1, 4, 5, 8]);
    }
}
