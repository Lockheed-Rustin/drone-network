use crossbeam_channel::{Receiver, Sender};
use dn_controller::ServerEvent;
use std::collections::HashMap;
use wg_2024::network::NodeId;
use wg_2024::packet::Packet;

pub struct Router<S, R> {
    controller_send: Sender<ServerEvent>,
    packet_send: HashMap<NodeId, Sender<Packet>>,
    packet_recv: Receiver<Packet>,

    message_recv: Sender<R>,
    message_send: Receiver<S>,
}

impl<S, R> Router<S, R> {
    fn run(&mut self) {
        while let Ok(msg) = self.message_send.recv() {
            self.handle_message(msg);
        }
    }
    fn handle_message(msg: S) {}
}
