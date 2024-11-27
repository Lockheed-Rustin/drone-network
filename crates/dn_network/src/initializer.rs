use crossbeam_channel::{unbounded, Receiver, Sender};
use dn_client::Client;
use dn_controller::{ClientCommand, SimulationController};
use dn_server::Server;
use drone::LockheedRustin;
use std::{
    collections::HashMap,
    fs,
    thread::{self, JoinHandle},
};
use wg_2024::{
    config,
    controller::{DroneCommand, NodeEvent},
    drone::{Drone, DroneOptions},
    network::NodeId,
    packet::Packet,
};

fn parse_config(file: &str) -> config::Config {
    let file_str = fs::read_to_string(file).unwrap();
    toml::from_str(&file_str).unwrap()
}

pub fn init_network(file: &str) -> SimulationController {
    let config = parse_config(file);
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

    let drones_send = init_drones(config.drone, &packets, node_send.clone(), &mut handles);
    let clients_send = init_clients(config.client, &packets, node_send.clone(), &mut handles);
    let server_ids = init_servers(config.server, &packets, node_send.clone(), &mut handles);

    SimulationController {
        drones_send,
        clients_send,
        server_ids,
        node_recv,
        handles,
    }
}

fn init_drones(
    drones: Vec<config::Drone>,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: Sender<NodeEvent>,
    handles: &mut Vec<JoinHandle<()>>,
) -> HashMap<NodeId, Sender<DroneCommand>> {
    let mut drones_send = HashMap::new();
    for drone in drones.into_iter() {
        // controller
        let (drone_send, controller_recv) = unbounded();
        drones_send.insert(drone.id, drone_send);
        let controller_send = controller_send.clone();
        // packet
        let packet_recv = packets[&drone.id].1.clone();
        let packet_send = drone
            .connected_node_ids
            .into_iter()
            .map(|id| (id, packets[&id].0.clone()))
            .collect();

        handles.push(thread::spawn(move || {
            let mut drone = LockheedRustin::new(DroneOptions {
                id: drone.id,
                controller_send,
                controller_recv,
                packet_send,
                packet_recv,
                pdr: drone.pdr,
            });
            drone.run();
        }));
    }
    drones_send
}

fn init_clients(
    clients: Vec<config::Client>,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: Sender<NodeEvent>,
    handles: &mut Vec<JoinHandle<()>>,
) -> HashMap<NodeId, Sender<ClientCommand>> {
    let mut clients_send = HashMap::new();
    for client in clients.into_iter() {
        // controller
        let (client_send, controller_recv) = unbounded();
        clients_send.insert(client.id, client_send);
        let controller_send = controller_send.clone();
        // packet
        let packet_recv = packets[&client.id].1.clone();
        let packet_send = client
            .connected_drone_ids
            .into_iter()
            .map(|id| (id, packets[&id].0.clone()))
            .collect();

        handles.push(thread::spawn(move || {
            let mut client = Client {
                controller_send,
                controller_recv,
                packet_send,
                packet_recv,
            };
            client.run();
        }));
    }
    clients_send
}

fn init_servers(
    servers: Vec<config::Server>,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: Sender<NodeEvent>,
    handles: &mut Vec<JoinHandle<()>>,
) -> Vec<NodeId> {
    let mut server_ids = Vec::new();
    for server in servers.into_iter() {
        // controller
        server_ids.push(server.id);
        let controller_send = controller_send.clone();
        // packet
        let packet_recv = packets[&server.id].1.clone();
        let packet_send = server
            .connected_drone_ids
            .into_iter()
            .map(|id| (id, packets[&id].0.clone()))
            .collect();

        handles.push(thread::spawn(move || {
            let mut server = Server {
                controller_send,
                packet_send,
                packet_recv,
            };
            server.run();
        }));
    }
    server_ids
}
