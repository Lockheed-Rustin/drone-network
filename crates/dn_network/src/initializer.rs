use crossbeam_channel::{unbounded, Receiver, Sender};
use dn_client::Client;
use dn_controller::{
    ClientEvent, Node, NodeType as ControllerNodeType, ServerEvent, SimulationController, Topology,
};
use lockheedrustin_drone::LockheedRustin;
use petgraph::prelude::{DiGraphMap, UnGraphMap};
use std::collections::HashMap;
use wg_2024::{
    config,
    controller::DroneEvent,
    drone::Drone,
    network::NodeId,
    packet::{NodeType, Packet},
};
use dn_server::communication_server_code::communication_server::CommunicationServer;

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

struct InitOption<'a> {
    config: &'a config::Config,
    packets: HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    drone_send: Sender<DroneEvent>,
    server_send: Sender<ServerEvent>,
    client_send: Sender<ClientEvent>,
    nodes: HashMap<NodeId, Node>,
    pool: rayon::ThreadPool,
}

pub fn init_network(config: &config::Config) -> Result<SimulationController, NetworkInitError> {
    let topology = init_topology(config)?;

    let (drone_send, drone_recv) = unbounded();
    let (server_send, server_recv) = unbounded();
    let (client_send, client_recv) = unbounded();

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

    let pool = rayon::ThreadPoolBuilder::new().build().unwrap();

    let mut opt = InitOption {
        config,
        packets,
        drone_send,
        server_send,
        client_send,
        nodes: HashMap::new(),
        pool,
    };
    init_drones(&mut opt);
    init_clients(&mut opt);
    init_servers(&mut opt);

    Ok(SimulationController::new(
        opt.nodes,
        drone_recv,
        server_recv,
        client_recv,
        topology,
        opt.pool,
    ))
}

fn get_packet_send(opt: &mut InitOption, node_ids: &[NodeId]) -> HashMap<NodeId, Sender<Packet>> {
    node_ids
        .iter()
        .cloned()
        .map(|id| (id, opt.packets[&id].0.clone()))
        .collect()
}

fn init_drones(opt: &mut InitOption) {
    for drone in opt.config.drone.iter() {
        // controller
        let (drone_send, controller_recv) = unbounded();
        opt.nodes.insert(
            drone.id,
            Node {
                packet_send: opt.packets[&drone.id].0.clone(),
                node_type: ControllerNodeType::Drone {
                    sender: drone_send,
                    pdr: drone.pdr,
                },
            },
        );
        let controller_send = opt.drone_send.clone();
        // packet
        let packet_recv = opt.packets[&drone.id].1.clone();
        let packet_send = get_packet_send(opt, &drone.connected_node_ids);
        let drone_id = drone.id;
        let drone_pdr = drone.pdr;

        opt.pool.spawn(move || {
            LockheedRustin::new(
                drone_id,
                controller_send,
                controller_recv,
                packet_recv,
                packet_send,
                drone_pdr,
            )
            .run();
        });
    }
}

fn init_clients(opt: &mut InitOption) {
    for client in opt.config.client.iter() {
        // controller
        let (client_send, controller_recv) = unbounded();
        opt.nodes.insert(
            client.id,
            Node {
                packet_send: opt.packets[&client.id].0.clone(),
                node_type: ControllerNodeType::Client {
                    sender: client_send,
                },
            },
        );
        let controller_send = opt.client_send.clone();
        // packet
        let packet_recv = opt.packets[&client.id].1.clone();
        let packet_send = get_packet_send(opt, &client.connected_drone_ids);
        let id = client.id;

        opt.pool.spawn(move || {
            Client::new(
                id,
                controller_send,
                controller_recv,
                packet_send,
                packet_recv,
            )
            .run();
        });
    }
}

fn init_servers(opt: &mut InitOption) {
    for server in opt.config.server.iter() {
        // controller
        let (server_send, controller_recv) = unbounded();
        opt.nodes.insert(
            server.id,
            Node {
                packet_send: opt.packets[&server.id].0.clone(),
                node_type: ControllerNodeType::Server {
                    sender: server_send,
                },
            },
        );
        let controller_send = opt.server_send.clone();
        // packet
        let packet_recv = opt.packets[&server.id].1.clone();
        let packet_send = get_packet_send(opt, &server.connected_drone_ids);
        let id = server.id;

        opt.pool.spawn(move || {
            CommunicationServer::new(
                controller_send,
                controller_recv,
                packet_send,
                packet_recv,
                id,
            )
            .run();
        });
    }
}

// TODO: add this checks inside controller is_valid_topology
fn init_topology(config: &config::Config) -> Result<Topology, NetworkInitError> {
    let mut graph = DiGraphMap::new();
    let mut node_types = HashMap::new();

    for drone in config.drone.iter() {
        if drone.pdr < 0.0 || drone.pdr > 1.0 {
            return Err(NetworkInitError::Pdr);
        }
        graph.add_node(drone.id);
        node_types.insert(drone.id, NodeType::Drone);
    }
    for client in config.client.iter() {
        if !(1..=2).contains(&client.connected_drone_ids.len()) {
            return Err(NetworkInitError::EdgeCount);
        }
        graph.add_node(client.id);
        node_types.insert(client.id, NodeType::Client);
    }
    for server in config.server.iter() {
        if server.connected_drone_ids.len() < 2 {
            return Err(NetworkInitError::EdgeCount);
        }
        graph.add_node(server.id);
        node_types.insert(server.id, NodeType::Server);
    }

    for drone in config.drone.iter() {
        for neighbor_id in drone.connected_node_ids.iter() {
            if drone.id == *neighbor_id {
                return Err(NetworkInitError::SelfLoop);
            }
            let _ = *node_types
                .get(neighbor_id)
                .ok_or(NetworkInitError::NodeId)?;
            graph.add_edge(drone.id, *neighbor_id, ());
        }
    }
    for client in config.client.iter() {
        for neighbor_id in client.connected_drone_ids.iter() {
            if client.id == *neighbor_id {
                return Err(NetworkInitError::SelfLoop);
            }
            let neighbor_type = *node_types
                .get(neighbor_id)
                .ok_or(NetworkInitError::NodeId)?;
            if neighbor_type != NodeType::Drone {
                return Err(NetworkInitError::Edge);
            }
            graph.add_edge(client.id, *neighbor_id, ());
        }
    }
    for server in config.server.iter() {
        for neighbor_id in server.connected_drone_ids.iter() {
            if server.id == *neighbor_id {
                return Err(NetworkInitError::SelfLoop);
            }
            let neighbor_type = *node_types
                .get(neighbor_id)
                .ok_or(NetworkInitError::NodeId)?;
            if neighbor_type != NodeType::Drone {
                return Err(NetworkInitError::Edge);
            }
            graph.add_edge(server.id, *neighbor_id, ());
        }
    }

    let mut topology = UnGraphMap::new();
    for node in graph.nodes() {
        topology.add_node(node);
    }
    for (a, b, _) in graph.all_edges() {
        if !graph.contains_edge(b, a) {
            return Err(NetworkInitError::Directed);
        }
        topology.add_edge(a, b, ());
    }
    Ok(topology)
}
