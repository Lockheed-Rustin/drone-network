use crate::fair_drones::{fair_drones, fair_drones_adapter, DroneOptions, FairDrones};
use crossbeam_channel::{unbounded, Receiver, Sender};
use dn_client::Client;
use dn_controller::{
    ClientEvent, Node, NodeType as ControllerNodeType, ServerEvent, SimulationController,
    SimulationControllerOptions, Topology,
};
use dn_server::content_server::ContentServer;
use dn_server::{communication_server::CommunicationServer, content_server::ContentServerOptions};
use petgraph::prelude::{DiGraphMap, UnGraphMap};
use rayon::{
    iter::{IntoParallelIterator, ParallelIterator},
    ThreadPoolBuilder,
};
use std::collections::HashMap;
use wg_2024::{
    config::Config,
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

pub fn init_network(config: &Config) -> Result<SimulationController, NetworkInitError> {
    init_network_with_fair_drones(config, fair_drones())
}

pub fn init_network_with_drone<D: Drone + 'static>(
    config: &Config,
    group_name: String,
) -> Result<SimulationController, NetworkInitError> {
    init_network_with_fair_drones(config, fair_drones_adapter::<D>(group_name))
}

fn init_network_with_fair_drones(
    config: &Config,
    drones: FairDrones,
) -> Result<SimulationController, NetworkInitError> {
    let topology = init_topology(config)?;

    let mut nodes = HashMap::new();

    let (drone_send, drone_recv) = unbounded();
    let (server_send, server_recv) = unbounded();
    let (client_send, client_recv) = unbounded();

    let mut packets = HashMap::new();
    for drone in &config.drone {
        packets.insert(drone.id, unbounded());
    }
    for client in &config.client {
        packets.insert(client.id, unbounded());
    }
    for server in &config.server {
        packets.insert(server.id, unbounded());
    }

    let drone_pool = ThreadPoolBuilder::new().build().unwrap();
    let client_pool = ThreadPoolBuilder::new().build().unwrap();
    let server_pool = ThreadPoolBuilder::new().build().unwrap();

    let drones = drone_options(config, &mut nodes, &packets, &drone_send, &drones);
    let clients = client_options(config, &mut nodes, &packets, &client_send);
    let servers = server_options(config, &mut nodes, &packets, &server_send);

    drone_pool.spawn(|| {
        drones.into_par_iter().for_each(|mut drone| drone.run());
    });
    client_pool.spawn(|| {
        clients.into_par_iter().for_each(|mut client| client.run());
    });
    server_pool.spawn(|| {
        servers.into_par_iter().for_each(|server| match server {
            Server::ContentServer(mut server) => server.run(),
            Server::CommunicationServer(mut server) => server.run(),
        });
    });

    Ok(SimulationController::new(SimulationControllerOptions {
        nodes,
        drone_recv,
        server_recv,
        client_recv,
        topology,
        drone_pool,
        client_pool,
        server_pool,
    }))
}

fn get_packet_send(
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    node_ids: &[NodeId],
) -> HashMap<NodeId, Sender<Packet>> {
    node_ids
        .iter()
        .copied()
        .map(|id| (id, packets[&id].0.clone()))
        .collect()
}

fn drone_options(
    config: &Config,
    nodes: &mut HashMap<NodeId, Node>,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: &Sender<DroneEvent>,
    drones: &FairDrones,
) -> Vec<Box<dyn Drone>> {
    config
        .drone
        .iter()
        .enumerate()
        .map(|(i, drone)| {
            // controller
            let (drone_send, controller_recv) = unbounded();
            nodes.insert(
                drone.id,
                Node {
                    packet_send: packets[&drone.id].0.clone(),
                    node_type: ControllerNodeType::Drone {
                        sender: drone_send,
                        pdr: drone.pdr,
                        group_name: drones.get(i).group_name().to_string(),
                    },
                },
            );
            let controller_send = controller_send.clone();
            // packet
            let packet_recv = packets[&drone.id].1.clone();
            let packet_send = get_packet_send(packets, &drone.connected_node_ids);
            let id = drone.id;
            let pdr = drone.pdr;

            drones.get(i).drone(DroneOptions {
                id,
                controller_send,
                controller_recv,
                packet_recv,
                packet_send,
                pdr,
            })
        })
        .collect()
}

fn client_options(
    config: &Config,
    nodes: &mut HashMap<NodeId, Node>,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: &Sender<ClientEvent>,
) -> Vec<Client> {
    config
        .client
        .iter()
        .map(|client| {
            // controller
            let (client_send, controller_recv) = unbounded();
            nodes.insert(
                client.id,
                Node {
                    packet_send: packets[&client.id].0.clone(),
                    node_type: ControllerNodeType::Client {
                        sender: client_send,
                    },
                },
            );
            let controller_send = controller_send.clone();
            // packet
            let packet_recv = packets[&client.id].1.clone();
            let packet_send = get_packet_send(packets, &client.connected_drone_ids);
            let id = client.id;

            Client::new(
                id,
                controller_send,
                controller_recv,
                packet_send,
                packet_recv,
            )
        })
        .collect()
}

enum Server {
    ContentServer(ContentServer),
    CommunicationServer(CommunicationServer),
}

fn server_options(
    config: &Config,
    nodes: &mut HashMap<NodeId, Node>,
    packets: &HashMap<NodeId, (Sender<Packet>, Receiver<Packet>)>,
    controller_send: &Sender<ServerEvent>,
) -> Vec<Server> {
    config
        .server
        .iter()
        .enumerate()
        .map(|(i, server)| {
            // controller
            let (server_send, controller_recv) = unbounded();
            nodes.insert(
                server.id,
                Node {
                    packet_send: packets[&server.id].0.clone(),
                    node_type: ControllerNodeType::Server {
                        sender: server_send,
                    },
                },
            );
            let controller_send = controller_send.clone();
            // packet
            let packet_recv = packets[&server.id].1.clone();
            let packet_send = get_packet_send(packets, &server.connected_drone_ids);
            let id = server.id;

            if i % 2 == 0 {
                Server::CommunicationServer(CommunicationServer::new(
                    controller_send,
                    controller_recv,
                    packet_send,
                    packet_recv,
                    id,
                ))
            } else {
                Server::ContentServer(ContentServer::new(ContentServerOptions {
                    id,
                    controller_send,
                    controller_recv,
                    packet_recv,
                    packet_send,
                }))
            }
        })
        .collect()
}

fn init_topology(config: &Config) -> Result<Topology, NetworkInitError> {
    let mut graph = DiGraphMap::new();
    let mut node_types = HashMap::new();

    for drone in &config.drone {
        if drone.pdr < 0.0 || drone.pdr > 1.0 {
            return Err(NetworkInitError::Pdr);
        }
        graph.add_node(drone.id);
        node_types.insert(drone.id, NodeType::Drone);
    }
    for client in &config.client {
        if !(1..=2).contains(&client.connected_drone_ids.len()) {
            return Err(NetworkInitError::EdgeCount);
        }
        graph.add_node(client.id);
        node_types.insert(client.id, NodeType::Client);
    }
    for server in &config.server {
        if server.connected_drone_ids.len() < 2 {
            return Err(NetworkInitError::EdgeCount);
        }
        graph.add_node(server.id);
        node_types.insert(server.id, NodeType::Server);
    }

    for drone in &config.drone {
        for neighbor_id in &drone.connected_node_ids {
            if drone.id == *neighbor_id {
                return Err(NetworkInitError::SelfLoop);
            }
            let _ = *node_types
                .get(neighbor_id)
                .ok_or(NetworkInitError::NodeId)?;
            graph.add_edge(drone.id, *neighbor_id, ());
        }
    }
    for client in &config.client {
        for neighbor_id in &client.connected_drone_ids {
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
        for neighbor_id in &server.connected_drone_ids {
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
    for (a, b, ()) in graph.all_edges() {
        if !graph.contains_edge(b, a) {
            return Err(NetworkInitError::Directed);
        }
        topology.add_edge(a, b, ());
    }
    Ok(topology)
}
