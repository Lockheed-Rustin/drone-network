use crossbeam_channel::{unbounded, Receiver, Sender};
use dn_client::Client;
use dn_controller::{ClientEvent, NodeSender, ServerEvent, SimulationController};
use dn_server::Server;
use dn_topology::{Node, Topology};
use drone::LockheedRustin;
use petgraph::prelude::UnGraphMap;
use std::collections::HashMap;
use wg_2024::{
    config,
    controller::DroneEvent,
    drone::Drone,
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

struct InitOption<'a> {
    config: &'a config::Config,
    packets: HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    drone_send: Sender<DroneEvent>,
    server_send: Sender<ServerEvent>,
    client_send: Sender<ClientEvent>,
    node_senders: HashMap<NodeId, NodeSender>,
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
        node_senders: HashMap::new(),
        pool,
    };
    init_drones(&mut opt);
    init_clients(&mut opt);
    init_servers(&mut opt);

    Ok(SimulationController::new(
        opt.node_senders,
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
        opt.node_senders.insert(
            drone.id,
            NodeSender::Drone(drone_send, opt.packets[&drone.id].0.clone()),
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
        opt.node_senders.insert(
            client.id,
            NodeSender::Client(client_send, opt.packets[&client.id].0.clone()),
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
        opt.node_senders.insert(
            server.id,
            NodeSender::Server(server_send, opt.packets[&server.id].0.clone()),
        );
        let controller_send = opt.server_send.clone();
        // packet
        let packet_recv = opt.packets[&server.id].1.clone();
        let packet_send = get_packet_send(opt, &server.connected_drone_ids);
        let id = server.id;

        opt.pool.spawn(move || {
            Server::new(
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

pub fn init_topology(config: &config::Config) -> Result<Topology, NetworkInitError> {
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
