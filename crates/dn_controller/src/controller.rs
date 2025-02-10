use crate::{ClientCommand, ClientEvent, ServerCommand, ServerEvent};
use crossbeam_channel::{Receiver, Sender};
use dn_message::ClientBody;
use petgraph::algo::connected_components;
use petgraph::prelude::UnGraphMap;
use rayon::ThreadPool;
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
        group_name: String,
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

pub struct SimulationControllerOptions {
    pub nodes: HashMap<NodeId, Node>,
    pub drone_recv: Receiver<DroneEvent>,
    pub server_recv: Receiver<ServerEvent>,
    pub client_recv: Receiver<ClientEvent>,
    pub topology: Topology,
    pub drone_pool: ThreadPool,
    pub client_pool: ThreadPool,
    pub server_pool: ThreadPool,
}

#[allow(clippy::module_name_repetitions)]
pub struct SimulationController {
    nodes: HashMap<NodeId, Node>,

    drone_recv: Receiver<DroneEvent>,
    client_recv: Receiver<ClientEvent>,
    server_recv: Receiver<ServerEvent>,

    topology: Topology,

    #[allow(unused)]
    drone_pool: ThreadPool,
    #[allow(unused)]
    client_pool: ThreadPool,
    #[allow(unused)]
    server_pool: ThreadPool,
}

impl SimulationController {
    #[must_use]
    pub fn new(opt: SimulationControllerOptions) -> Self {
        Self {
            nodes: opt.nodes,
            drone_recv: opt.drone_recv,
            server_recv: opt.server_recv,
            client_recv: opt.client_recv,
            topology: opt.topology,
            drone_pool: opt.drone_pool,
            client_pool: opt.client_pool,
            server_pool: opt.server_pool,
        }
    }

    #[must_use]
    pub fn get_drone_recv(&self) -> Receiver<DroneEvent> {
        self.drone_recv.clone()
    }

    #[must_use]
    pub fn get_server_recv(&self) -> Receiver<ServerEvent> {
        self.server_recv.clone()
    }

    #[must_use]
    pub fn get_client_recv(&self) -> Receiver<ClientEvent> {
        self.client_recv.clone()
    }

    #[must_use]
    pub fn get_drone_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| match node.node_type {
                NodeType::Drone { .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    #[must_use]
    pub fn get_client_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| match node.node_type {
                NodeType::Client { .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    #[must_use]
    pub fn get_server_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter_map(|(id, node)| match node.node_type {
                NodeType::Server { .. } => Some(*id),
                _ => None,
            })
            .collect()
    }

    fn get_drone_sender(&self, id: NodeId) -> Option<Sender<DroneCommand>> {
        match &self.nodes.get(&id)?.node_type {
            NodeType::Drone { sender, .. } => Some(sender.clone()),
            _ => None,
        }
    }

    fn get_client_sender(&self, id: NodeId) -> Option<Sender<ClientCommand>> {
        match &self.nodes.get(&id)?.node_type {
            NodeType::Client { sender } => Some(sender.clone()),
            _ => None,
        }
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
        // We need to return the OPPOSITE of what petgraph::graphmap::add_edge returns
        match self.topology.add_edge(a, b, ()) {
            None => Some(()),
            Some(()) => None,
        }
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
            self.remove_sender(neighbor, id)?;
        }
        self.nodes.remove(&id);

        self.topology.remove_node(id);
        Some(())
    }

    pub fn set_pdr(&mut self, id: NodeId, new_pdr: f32) -> Option<()> {
        let new_pdr = new_pdr.clamp(0.0, 1.0);
        match &mut self.nodes.get_mut(&id)?.node_type {
            NodeType::Drone { sender, pdr, .. } => {
                sender.send(DroneCommand::SetPacketDropRate(new_pdr)).ok();
                *pdr = new_pdr;
                Some(())
            }
            _ => None,
        }
    }

    #[must_use]
    pub fn get_pdr(&self, drone_id: NodeId) -> Option<f32> {
        match &self.nodes.get(&drone_id)?.node_type {
            NodeType::Drone { pdr, .. } => Some(*pdr),
            _ => None,
        }
    }

    #[must_use]
    pub fn get_group_name(&self, drone_id: NodeId) -> Option<&str> {
        match &self.nodes.get(&drone_id)?.node_type {
            NodeType::Drone { group_name, .. } => Some(group_name),
            _ => None,
        }
    }

    #[must_use]
    pub fn client_send_message(
        &self,
        client_id: NodeId,
        dest: NodeId,
        body: ClientBody,
    ) -> Option<()> {
        let sender = self.get_client_sender(client_id)?;
        sender.send(ClientCommand::SendMessage(body, dest)).ok()
    }

    #[must_use]
    pub fn shortcut(&self, p: Packet) -> Option<()> {
        let dest_id = p.routing_header.hops.last()?;
        let sender = &self.nodes.get(dest_id)?.packet_send;
        sender.send(p).ok()
    }

    #[must_use]
    pub fn get_topology(&self) -> &Topology {
        &self.topology
    }

    #[must_use]
    pub fn is_valid_topology(&self) -> bool {
        if connected_components(&self.topology) != 1 {
            return false;
        }
        for node in self.topology.nodes() {
            let connected_nodes_count = self.topology.neighbors(node).count();
            match self.nodes[&node].node_type {
                NodeType::Drone { .. } => {}
                NodeType::Client { .. } => {
                    if !(1..=2).contains(&connected_nodes_count) {
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
        true
    }

    pub fn topology_crash_check(&mut self, id: NodeId) -> bool {
        let neighbors = self.topology.neighbors(id).collect::<Vec<_>>();

        // remove node and check if it's a valid topology
        self.topology.remove_node(id);
        let valid = self.is_valid_topology();
        // restore the topology
        self.topology.add_node(id);
        for neighbor in &neighbors {
            self.topology.add_edge(id, *neighbor, ());
        }

        valid
    }
}

impl Debug for SimulationController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#?}", self.nodes)
    }
}

impl Drop for SimulationController {
    fn drop(&mut self) {
        for (id, node) in self.nodes.drain() {
            match node.node_type {
                NodeType::Drone { sender, .. } => {
                    sender.send(DroneCommand::Crash).unwrap();
                    // remove all senders
                    for neighbor in self.topology.neighbors(id) {
                        _ = sender.send(DroneCommand::RemoveSender(neighbor));
                    }
                }
                NodeType::Client { sender } => sender.send(ClientCommand::Return).unwrap(),
                NodeType::Server { sender } => sender.send(ServerCommand::Return).unwrap(),
            }
        }
    }
}
