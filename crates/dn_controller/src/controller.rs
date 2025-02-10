use crate::{ClientCommand, ClientEvent, ServerCommand, ServerEvent};
use core::result;
use crossbeam_channel::{Receiver, SendError, Sender};
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

pub enum Error {
    /// if the id is not present in the topology
    Missing,
    /// crossbeam's `SendError`
    SendError,
    /// you are trying to call a function that's intended
    /// only for one type of node on another type of node
    /// e.g. calling `set_pdr` on a client
    InvalidNode,
    /// this error can only be returned when you try to
    /// crash a drone that would alter the topology
    /// in such a way that's not allowed by the protocol
    InvalidTopology,
}

impl<T> From<SendError<T>> for Error {
    fn from(_: SendError<T>) -> Self {
        Self::SendError
    }
}

pub type Result<T> = result::Result<T, Error>;

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
    /// # Errors
    /// see `Error`
    fn add_sender(&self, id: NodeId, ps: Sender<Packet>) -> Result<()> {
        match self {
            NodeType::Drone { sender, .. } => Ok(sender.send(DroneCommand::AddSender(id, ps))?),
            NodeType::Client { sender } => Ok(sender.send(ClientCommand::AddSender(id, ps))?),
            NodeType::Server { sender } => Ok(sender.send(ServerCommand::AddSender(id, ps))?),
        }
    }

    /// # Errors
    /// see `Error`
    fn remove_sender(&self, id: NodeId) -> Result<()> {
        match self {
            NodeType::Drone { sender, .. } => Ok(sender.send(DroneCommand::RemoveSender(id))?),
            NodeType::Client { sender } => Ok(sender.send(ClientCommand::RemoveSender(id))?),
            NodeType::Server { sender } => Ok(sender.send(ServerCommand::RemoveSender(id))?),
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

    /// # Errors
    /// see `Error`
    fn get_drone_sender(&self, id: NodeId) -> Result<Sender<DroneCommand>> {
        match &self.nodes.get(&id).ok_or(Error::Missing)?.node_type {
            NodeType::Drone { sender, .. } => Ok(sender.clone()),
            _ => Err(Error::InvalidNode),
        }
    }

    /// # Errors
    /// see `Error`
    fn get_client_sender(&self, id: NodeId) -> Result<Sender<ClientCommand>> {
        match &self.nodes.get(&id).ok_or(Error::Missing)?.node_type {
            NodeType::Client { sender } => Ok(sender.clone()),
            _ => Err(Error::InvalidNode),
        }
    }

    /// # Errors
    /// see `Error`
    fn add_sender(&self, a: NodeId, b: NodeId) -> Result<()> {
        let a_node = self.nodes.get(&a).ok_or(Error::Missing)?;
        let b_node = self.nodes.get(&b).ok_or(Error::Missing)?;
        a_node.node_type.add_sender(b, b_node.packet_send.clone())
    }

    /// # Errors
    /// see `Error`
    pub fn add_edge(&mut self, a: NodeId, b: NodeId) -> Result<()> {
        self.add_sender(a, b)?;
        self.add_sender(b, a)?;
        self.topology.add_edge(a, b, ());
        Ok(())
    }

    /// # Errors
    /// see `Error`
    fn remove_sender(&self, a: NodeId, b: NodeId) -> Result<()> {
        let a_node = self.nodes.get(&a).ok_or(Error::Missing)?;
        a_node.node_type.remove_sender(b)
    }

    /// # Errors
    /// see `Error`
    pub fn remove_edge(&mut self, a: NodeId, b: NodeId) -> Result<()> {
        self.remove_sender(a, b)?;
        self.remove_sender(b, a)?;

        self.topology.remove_edge(a, b);
        Ok(())
    }

    /// # Errors
    /// see `Error`
    pub fn crash_drone(&mut self, id: NodeId) -> Result<()> {
        let sender = self.get_drone_sender(id)?;
        if !self.topology_crash_check(id) {
            return Err(Error::InvalidTopology);
        }

        sender.send(DroneCommand::Crash)?;
        // remove all senders
        for neighbor in self.topology.neighbors(id) {
            self.remove_sender(neighbor, id)?;
        }
        self.nodes.remove(&id);

        self.topology.remove_node(id);
        Ok(())
    }

    /// # Errors
    /// see `Error`
    pub fn set_pdr(&mut self, id: NodeId, new_pdr: f32) -> Result<()> {
        let new_pdr = new_pdr.clamp(0.0, 1.0);
        Ok(self
            .get_drone_sender(id)?
            .send(DroneCommand::SetPacketDropRate(new_pdr))?)
    }

    /// # Errors
    /// see `Error`
    pub fn get_pdr(&self, drone_id: NodeId) -> Result<f32> {
        match &self.nodes.get(&drone_id).ok_or(Error::Missing)?.node_type {
            NodeType::Drone { pdr, .. } => Ok(*pdr),
            _ => Err(Error::InvalidNode),
        }
    }

    /// # Errors
    /// see `Error`
    pub fn get_group_name(&self, drone_id: NodeId) -> Result<&str> {
        match &self.nodes.get(&drone_id).ok_or(Error::Missing)?.node_type {
            NodeType::Drone { group_name, .. } => Ok(group_name),
            _ => Err(Error::InvalidNode),
        }
    }

    /// # Errors
    /// see `Error`
    pub fn client_send_message(
        &self,
        client_id: NodeId,
        dest: NodeId,
        body: ClientBody,
    ) -> Result<()> {
        let sender = self.get_client_sender(client_id)?;
        Ok(sender.send(ClientCommand::SendMessage(body, dest))?)
    }

    /// # Panics
    /// if `hops.len()` == 0
    ///
    /// # Errors
    /// see `Error`
    pub fn shortcut(&self, p: Packet) -> Result<()> {
        let dest_id = p.routing_header.hops.last().unwrap();
        let sender = &self.nodes.get(dest_id).ok_or(Error::Missing)?.packet_send;
        Ok(sender.send(p)?)
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
