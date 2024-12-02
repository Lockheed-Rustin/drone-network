use crossbeam_channel::{Receiver, Sender};
use dn_controller::ServerCommand;
use std::collections::HashMap;
use wg_2024::{controller::NodeEvent, network::NodeId, packet::Packet};

pub struct Server {
    pub controller_send: Sender<NodeEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
}

impl Server {
    pub fn run(&mut self) {}
}
