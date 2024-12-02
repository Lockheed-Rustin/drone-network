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
    pub node_senders: HashMap<NodeId, NodeSender>,
    pub node_recv: Receiver<NodeEvent>,

    pub topology: Topology,

    pub pool: rayon::ThreadPool,
}

impl SimulationController {
    pub fn crash_drone(&self, id: NodeId) -> Option<()> {
        let sender = self.get_drone_sender(id)?.0;
        sender.send(DroneCommand::Crash).ok()
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

    pub fn add_edge(&self, a: NodeId, b: NodeId) -> Option<()> {
        let a_sender = self.get_sender(a)?;
        let b_sender = self.get_sender(b)?;
        a_sender.add_sender(b, b_sender.get_packet_sender())?;
        b_sender.add_sender(a, a_sender.get_packet_sender())
    }
}
