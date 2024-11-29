use crossbeam_channel::{unbounded, Receiver, Sender};
use dn_client::Client;
use dn_controller::{ClientCommand, SimulationController};
use dn_server::Server;
use drone::LockheedRustin;
use std::{
    collections::{HashMap, HashSet},
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
pub enum NetworkInitErr {
    GraphMustBeBidirectional,
    InvalidPdr,
    InvalidNeighbor,
    InvalidNodeId,
    NodeConnectedToSelf,
}

pub fn init_network(config: &config::Config) -> Result<SimulationController, NetworkInitErr> {
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
        handles,
    })
}

fn is_drone(nt: NodeType) -> bool {
    if let NodeType::Drone = nt {
        true
    } else {
        false
    }
}

pub fn check_valid(config: &config::Config) -> Result<(), NetworkInitErr> {
    let mut graph = HashMap::new();
    for drone in config.drone.iter() {
        if drone.pdr < 0.0 || drone.pdr > 1.0 {
            return Err(NetworkInitErr::InvalidPdr);
        }
        let neighbors = drone
            .connected_node_ids
            .iter()
            .cloned()
            .collect::<HashSet<_>>();
        graph.insert(drone.id, (NodeType::Drone, neighbors));
    }
    for client in config.client.iter() {
        let neighbors = client.connected_drone_ids.iter().cloned().collect();
        graph.insert(client.id, (NodeType::Client, neighbors));
    }
    for server in config.server.iter() {
        let neighbors = server.connected_drone_ids.iter().cloned().collect();
        graph.insert(server.id, (NodeType::Server, neighbors));
    }
    for (node_id, (node_type, neighbors)) in graph.iter() {
        for n_node_id in neighbors.iter() {
            match graph.get(n_node_id) {
                Some((n_node_type, n_neighbors)) => {
                    if node_id == n_node_id {
                        return Err(NetworkInitErr::NodeConnectedToSelf);
                    } else if !is_drone(node_type.clone()) && !is_drone(n_node_type.clone()) {
                        return Err(NetworkInitErr::InvalidNeighbor);
                    } else if n_neighbors.contains(node_id) {
                        return Err(NetworkInitErr::GraphMustBeBidirectional);
                    }
                }
                None => return Err(NetworkInitErr::InvalidNodeId),
            };
        }
    }
    Ok(())
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
