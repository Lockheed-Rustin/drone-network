use crate::{ClientCommand, ServerCommand};
use crossbeam_channel::{Receiver, Sender};
use dn_topology::Topology;
use std::collections::HashMap;
use wg_2024::packet::Packet;
use wg_2024::{
    controller::{DroneCommand, NodeEvent},
    network::NodeId,
};

#[derive(Clone)]
pub enum NodeSender {
    Drone(Sender<DroneCommand>, Sender<Packet>),
    Client(Sender<ClientCommand>, Sender<Packet>),
    Server(Sender<ServerCommand>, Sender<Packet>),
}

impl NodeSender {
    fn get_packet_sender(&self) -> Sender<Packet> {
        match self {
            NodeSender::Drone(_, ps) => ps.clone(),
            NodeSender::Client(_, ps) => ps.clone(),
            NodeSender::Server(_, ps) => ps.clone(),
        }
    }

    fn add_sender(&self, id: NodeId, sender: Sender<Packet>) -> Option<()> {
        match self {
            NodeSender::Drone(cs, _) => cs.send(DroneCommand::AddSender(id, sender)).ok(),
            NodeSender::Client(cs, _) => cs.send(ClientCommand::AddSender(id, sender)).ok(),
            NodeSender::Server(cs, _) => cs.send(ServerCommand::AddSender(id, sender)).ok(),
        }
    }
}

pub struct SimulationController {
    node_senders: HashMap<NodeId, NodeSender>,
    node_recv: Receiver<NodeEvent>,

    pub topology: Topology,

    pool: rayon::ThreadPool,
}

impl SimulationController {
    pub fn new(
        node_senders: HashMap<NodeId, NodeSender>,
        node_recv: Receiver<NodeEvent>,
        topology: Topology,
        pool: rayon::ThreadPool,
    ) -> Self {
        Self {
            node_senders,
            node_recv,
            topology,
            pool,
        }
    }

    pub fn crash_drone(&mut self, id: NodeId) -> Option<()> {
        let sender = self.get_drone_sender(id)?.0;
        sender.send(DroneCommand::Crash).ok()?;
        self.node_senders.remove(&id);
        Some(())
    }

    pub fn set_pdr(&self, id: NodeId, pdr: f32) -> Option<()> {
        let pdr = pdr.clamp(0.0, 1.0);
        let sender = self.get_drone_sender(id)?.0;
        sender.send(DroneCommand::SetPacketDropRate(pdr)).ok()
    }

    fn get_sender(&self, id: NodeId) -> Option<NodeSender> {
        self.node_senders.get(&id).cloned()
    }

    fn get_drone_sender(&self, id: NodeId) -> Option<(Sender<DroneCommand>, Sender<Packet>)> {
        match self.get_sender(id)? {
            NodeSender::Drone(dcs, ps) => Some((dcs, ps)),
            _ => None,
        }
    }

    fn get_receiver(&self) -> Receiver<NodeEvent> {
        self.node_recv.clone()
    }

    pub fn add_edge(&self, a: NodeId, b: NodeId) -> Option<()> {
        let a_sender = self.get_sender(a)?;
        let b_sender = self.get_sender(b)?;
        a_sender.add_sender(b, b_sender.get_packet_sender())?;
        b_sender.add_sender(a, a_sender.get_packet_sender())
    }

    pub fn get_drone_ids(&self) -> Vec<NodeId> {
        let mut res = vec![];
        for (id, sender) in self.node_senders.iter() {
            match sender {
                NodeSender::Drone(_, _) => {
                    res.push(*id);
                }
                _ => {}
            }
        }
        res
    }

    pub fn get_client_ids(&self) -> Vec<NodeId> {
        let mut res = vec![];
        for (id, sender) in self.node_senders.iter() {
            match sender {
                NodeSender::Client(_, _) => {
                    res.push(*id);
                }
                _ => {}
            }
        }
        res
    }

    pub fn get_server_ids(&self) -> Vec<NodeId> {
        let mut res = vec![];
        for (id, sender) in self.node_senders.iter() {
            match sender {
                NodeSender::Server(_, _) => {
                    res.push(*id);
                }
                _ => {}
            }
        }
        res
    }
}
