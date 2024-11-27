use crossbeam_channel::{Receiver, Sender};
use dn_controller::ClientCommand;
use std::collections::HashMap;
use wg_2024::{controller::NodeEvent, network::NodeId, packet::Packet};

pub struct Client {
    pub controller_send: Sender<NodeEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
}

impl Client {
    pub fn run(&mut self) {}
}
