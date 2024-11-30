use crossbeam_channel::{unbounded, Receiver, Sender};
use dn_client::Client;
use dn_controller::{ClientCommand, SimulationController};
use dn_server::Server;
use dn_topology::{Node, Topology};
use drone::LockheedRustin;
use petgraph::prelude::UnGraphMap;
use std::{
    collections::HashMap,
    thread::{self, JoinHandle},
};
use wg_2024::{
    config,
    controller::{DroneCommand, NodeEvent},
    drone::{Drone, DroneOptions},
    network::NodeId,
    packet::{NodeType, Packet},
};

#[derive(Clone, Debug)]
pub enum NetworkInitError {
    /// If a client or server is connected to a non drone.
    Edge,
    /// If a node is connected to a node id that is not present in the nodes.
    NodeId,
    /// If a node is connected to self.
    SelfLoop,
    /// Drone pdr not in range.
    Pdr,
    /// If client is connected to less than one drone or more than two.
    ///
    /// If server is connected to less than two drones.
    EdgeCount,
    /// If the graph is not bidirectional.
    Directed,
}

pub fn init_network(config: &config::Config) -> Result<SimulationController, NetworkInitError> {
    let topology = build_topology(config)?;

    let (node_send, node_recv) = unbounded();
    let mut handles = Vec::new();

    let mut packets = HashMap::new();
    for drone in config.drone.iter() {
        packets.insert(drone.id, unbounded());
    }
    for client in config.client.iter() {
        packets.insert(client.id, unbounded());
    }
    for server in config.server.iter() {
        packets.insert(server.id, unbounded());
    }

    let drones_send = init_drones(config, &packets, node_send.clone(), &mut handles);
    let clients_send = init_clients(config, &packets, node_send.clone(), &mut handles);
    let server_ids = init_servers(config, &packets, node_send.clone(), &mut handles);

    Ok(SimulationController {
        drones_send,
        clients_send,
        server_ids,
        node_recv,
        topology,
        handles,
    })
}

pub fn build_topology(config: &config::Config) -> Result<Topology, NetworkInitError> {
    let mut graph = UnGraphMap::new();
    let mut nodes = HashMap::new();

    for drone in config.drone.iter() {
        if drone.pdr < 0.0 || drone.pdr > 1.0 {
            return Err(NetworkInitError::Pdr);
        }
        let node = Node {
            id: drone.id,
            ty: NodeType::Drone,
        };
        graph.add_node(node);
        nodes.insert(drone.id, node);
    }
    for client in config.client.iter() {
        if client.connected_drone_ids.len() < 1 || client.connected_drone_ids.len() > 2 {
            return Err(NetworkInitError::EdgeCount);
        }
        let node = Node {
            id: client.id,
            ty: NodeType::Drone,
        };
        graph.add_node(node);
        nodes.insert(client.id, node);
    }
    for server in config.server.iter() {
        if server.connected_drone_ids.len() < 2 {
            return Err(NetworkInitError::EdgeCount);
        }
        let node = Node {
            id: server.id,
            ty: NodeType::Server,
        };
        graph.add_node(node);
        nodes.insert(server.id, node);
    }

    for drone in config.drone.iter() {
        let node = nodes[&drone.id];
        for neighbor_id in drone.connected_node_ids.iter() {
            if drone.id == *neighbor_id {
                return Err(NetworkInitError::SelfLoop);
            }
            match nodes.get(neighbor_id) {
                Some(&neighbor) => graph.add_edge(node, neighbor, ()),
                None => return Err(NetworkInitError::NodeId),
            };
        }
    }
    for client in config.client.iter() {
        let node = nodes[&client.id];
        for neighbor_id in client.connected_drone_ids.iter() {
            if client.id == *neighbor_id {
                return Err(NetworkInitError::SelfLoop);
            }
            match nodes.get(neighbor_id) {
                Some(&neighbor) => {
                    if neighbor.ty != NodeType::Drone {
                        return Err(NetworkInitError::Edge);
                    } else {
                        graph.add_edge(node, neighbor, ());
                    }
                }
                None => return Err(NetworkInitError::NodeId),
            };
        }
    }
    for server in config.server.iter() {
        let node = nodes[&server.id];
        for neighbor_id in server.connected_drone_ids.iter() {
            if server.id == *neighbor_id {
                return Err(NetworkInitError::SelfLoop);
            }
            match nodes.get(neighbor_id) {
                Some(&neighbor) => {
                    if neighbor.ty != NodeType::Drone {
                        return Err(NetworkInitError::Edge);
                    } else {
                        graph.add_edge(node, neighbor, ());
                    }
                }
                None => return Err(NetworkInitError::NodeId),
            };
        }
    }

    if graph.is_directed() {
        Err(NetworkInitError::Directed)
    } else {
        Ok(graph.into())
    }
}

fn init_drones(
    config: &config::Config,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: Sender<NodeEvent>,
    handles: &mut Vec<JoinHandle<()>>,
) -> HashMap<NodeId, Sender<DroneCommand>> {
    let mut drones_send = HashMap::new();
    for drone in config.drone.iter() {
        // controller
        let (drone_send, controller_recv) = unbounded();
        drones_send.insert(drone.id, drone_send);
        let controller_send = controller_send.clone();
        // packet
        let packet_recv = packets[&drone.id].1.clone();
        let packet_send = drone
            .connected_node_ids
            .iter()
            .cloned()
            .map(|id| (id, packets[&id].0.clone()))
            .collect();

        let opt = DroneOptions {
            id: drone.id,
            controller_send,
            controller_recv,
            packet_send,
            packet_recv,
            pdr: drone.pdr,
        };
        handles.push(thread::spawn(move || {
            LockheedRustin::new(opt).run();
        }));
    }
    drones_send
}

fn init_clients(
    config: &config::Config,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: Sender<NodeEvent>,
    handles: &mut Vec<JoinHandle<()>>,
) -> HashMap<NodeId, Sender<ClientCommand>> {
    let mut clients_send = HashMap::new();
    for client in config.client.iter() {
        // controller
        let (client_send, controller_recv) = unbounded();
        clients_send.insert(client.id, client_send);
        let controller_send = controller_send.clone();
        // packet
        let packet_recv = packets[&client.id].1.clone();
        let packet_send = client
            .connected_drone_ids
            .iter()
            .cloned()
            .map(|id| (id, packets[&id].0.clone()))
            .collect();

        handles.push(thread::spawn(move || {
            Client {
                controller_send,
                controller_recv,
                packet_send,
                packet_recv,
            }
            .run();
        }));
    }
    clients_send
}

fn init_servers(
    config: &config::Config,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: Sender<NodeEvent>,
    handles: &mut Vec<JoinHandle<()>>,
) -> Vec<NodeId> {
    let mut server_ids = Vec::new();
    for server in config.server.iter() {
        // controller
        server_ids.push(server.id);
        let controller_send = controller_send.clone();
        // packet
        let packet_recv = packets[&server.id].1.clone();
        let packet_send = server
            .connected_drone_ids
            .iter()
            .cloned()
            .map(|id| (id, packets[&id].0.clone()))
            .collect();

        handles.push(thread::spawn(move || {
            Server {
                controller_send,
                packet_send,
                packet_recv,
            }
            .run();
        }));
    }
    server_ids
}
