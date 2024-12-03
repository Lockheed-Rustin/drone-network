use crossbeam_channel::{Receiver, Sender};
use dn_controller::ServerCommand;
use std::collections::HashMap;
use wg_2024::{network::NodeId, packet::Packet};

pub struct Server {
    // TODO: create ServerEvent (2 different enums for the 2 server types?)
    // pub controller_send: Sender<NodeEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
}

impl Server {
    pub fn run(&mut self) {
        loop {
            if let Ok(packet) = self.packet_recv.recv() {
                println!("packet received by server");
            } else {
                break;
            }
        }
    }
}
