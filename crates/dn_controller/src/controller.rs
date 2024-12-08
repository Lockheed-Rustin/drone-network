use crate::{ClientCommand, ClientEvent, ServerCommand, ServerEvent};
use crossbeam_channel::{Receiver, Sender};
use petgraph::algo::connected_components;
use petgraph::prelude::UnGraphMap;
use std::collections::HashMap;
use std::fmt::Debug;
use wg_2024::packet::Packet;
use wg_2024::{
    controller::{DroneCommand, DroneEvent},
    network::NodeId,
};

pub type Topology = UnGraphMap<NodeId, ()>;

#[derive(Debug, Clone)]
pub struct Node {
    pub packet_send: Sender<Packet>,
    pub node_type: NodeType,
}

#[derive(Debug, Clone)]
pub enum NodeType {
    Drone {
        sender: Sender<DroneCommand>,
        pdr: f32,
    },
    Client {
        sender: Sender<ClientCommand>,
    },
    Server {
        sender: Sender<ServerCommand>,
    },
}

impl NodeType {
    fn add_sender(&self, id: NodeId, ps: Sender<Packet>) -> Option<()> {
        match self {
            NodeType::Drone { sender, .. } => sender.send(DroneCommand::AddSender(id, ps)).ok(),
            NodeType::Client { sender } => sender.send(ClientCommand::AddSender(id, ps)).ok(),
            NodeType::Server { sender } => sender.send(ServerCommand::AddSender(id, ps)).ok(),
        }
    }
    fn remove_sender(&self, id: NodeId) -> Option<()> {
        match self {
            NodeType::Drone { sender, .. } => sender.send(DroneCommand::RemoveSender(id)).ok(),
            NodeType::Client { sender } => sender.send(ClientCommand::RemoveSender(id)).ok(),
            NodeType::Server { sender } => sender.send(ServerCommand::RemoveSender(id)).ok(),
        }
    }
}

pub struct SimulationController {
    nodes: HashMap<NodeId, Node>,

    drone_recv: Receiver<DroneEvent>,
    client_recv: Receiver<ClientEvent>,
    server_recv: Receiver<ServerEvent>,

    topology: Topology,

    _pool: rayon::ThreadPool,
}

impl SimulationController {
    pub fn new(
        nodes: HashMap<NodeId, Node>,
        drone_recv: Receiver<DroneEvent>,
        server_recv: Receiver<ServerEvent>,
        client_recv: Receiver<ClientEvent>,
        topology: Topology,
        pool: rayon::ThreadPool,
    ) -> Self {
        Self {
            nodes,
            drone_recv,
            server_recv,
            client_recv,
            topology,
            _pool: pool,
        }
    }

    pub fn get_drone_recv(&self) -> Receiver<DroneEvent> {
        self.drone_recv.clone()
    }

    pub fn get_server_recv(&self) -> Receiver<ServerEvent> {
        self.server_recv.clone()
    }

    pub fn get_client_recv(&self) -> Receiver<ClientEvent> {
        self.client_recv.clone()
    }

    pub fn get_drone_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| match node.node_type {
                NodeType::Drone { .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    pub fn get_client_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| match node.node_type {
                NodeType::Client { .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    pub fn get_server_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| match node.node_type {
                NodeType::Server { .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    pub fn get_drone_sender(&self, id: NodeId) -> Option<Sender<DroneCommand>> {
        match &self.nodes.get(&id)?.node_type {
            NodeType::Drone { sender, .. } => Some(sender.clone()),
            _ => None,
        }
    }

    pub fn get_client_sender(&self, id: NodeId) -> Option<Sender<ClientCommand>> {
        match &self.nodes.get(&id)?.node_type {
            NodeType::Client { sender } => Some(sender.clone()),
            _ => None,
        }
    }

    pub fn get_server_sender(&self, id: NodeId) -> Option<Sender<ServerCommand>> {
        match &self.nodes.get(&id)?.node_type {
            NodeType::Server { sender } => Some(sender.clone()),
            _ => None,
        }
    }

    pub fn get_packet_sender(&self, id: NodeId) -> Option<Sender<Packet>> {
        Some(self.nodes.get(&id)?.packet_send.clone())

    }

    fn add_sender(&self, a: NodeId, b: NodeId) -> Option<()> {
        let a_node = self.nodes.get(&a)?;
        let b_node = self.nodes.get(&b)?;
        a_node.node_type.add_sender(b, b_node.packet_send.clone())?;
        Some(())
    }

    pub fn add_edge(&mut self, a: NodeId, b: NodeId) -> Option<()> {
        self.add_sender(a, b)?;
        self.add_sender(b, a)?;

        self.topology.add_edge(a, b, ())?;
        Some(())
    }

    fn remove_sender(&self, a: NodeId, b: NodeId) -> Option<()> {
        let a_node = self.nodes.get(&a)?;
        a_node.node_type.remove_sender(b)?;
        Some(())
    }

    pub fn remove_edge(&mut self, a: NodeId, b: NodeId) -> Option<()> {
        self.remove_sender(a, b);
        self.remove_sender(b, a);

        self.topology.remove_edge(a, b)?;
        Some(())
    }

    pub fn crash_drone(&mut self, id: NodeId) -> Option<()> {
        let sender = self.get_drone_sender(id)?;
        if !self.topology_crash_check(id) {
            return None;
        }

        sender.send(DroneCommand::Crash).ok()?;
        // remove all senders
        for neighbor in self.topology.neighbors(id) {
            self.remove_sender(neighbor, id)?
        }
        self.nodes.remove(&id);

        self.topology.remove_node(id);
        Some(())
    }

    pub fn set_pdr(&mut self, id: NodeId, new_pdr: f32) -> Option<()> {
        let new_pdr = new_pdr.clamp(0.0, 1.0);
        match &mut self.nodes.get_mut(&id)?.node_type {
            NodeType::Drone { sender, pdr } => {
                sender.send(DroneCommand::SetPacketDropRate(new_pdr)).ok();
                *pdr = new_pdr;
                Some(())
            }
            _ => return None,
        }
    }

    pub fn get_pdr(&self, drone_id: NodeId) -> Option<f32> {
        match &self.nodes.get(&drone_id)?.node_type {
            NodeType::Drone { pdr, .. } => {
                Some(*pdr)
            }
            _ =>  None,
        }
    }

    // TODO: remove this after the fair
    pub fn send_fragment_fair(&self, id: NodeId) -> Option<()> {
        let sender = self.get_client_sender(id)?;
        sender.send(ClientCommand::SendFragment).ok()?;
        Some(())
    }

    // TODO: remove this after the fair
    pub fn send_flood_request_fair(&self, id: NodeId) -> Option<()> {
        let sender = self.get_client_sender(id)?;
        sender.send(ClientCommand::SendFloodRequest).ok()?;
        Some(())
    }

    pub fn is_valid_topology(&self) -> bool {
        if connected_components(&self.topology) != 1 {
            return false;
        }
        for node in self.topology.nodes() {
            let connected_nodes_count = self.topology.neighbors(node).count();
            match self.nodes[&node].node_type {
                NodeType::Drone { .. } => {}
                NodeType::Client { .. } => {
                    if connected_nodes_count < 1 || connected_nodes_count > 2 {
                        return false;
                    }
                }
                NodeType::Server { .. } => {
                    if connected_nodes_count < 2 {
                        return false;
                    }
                }
            };
        }
        return true;
    }

    pub fn topology_crash_check(&mut self, id: NodeId) -> bool {
        let neighbors = self.topology.neighbors(id).collect::<Vec<_>>();

        // remove node and check if it's a valid topology
        self.topology.remove_node(id);
        let valid = self.is_valid_topology();
        // restore the topology
        self.topology.add_node(id);
        for neighbor in neighbors.iter() {
            self.topology.add_edge(id, *neighbor, ());
        }

        return valid;
    }
}

impl Debug for SimulationController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{:#?}", self.topology)
    }
}
