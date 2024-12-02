use crossbeam_channel::Sender;
use dn_message::ClientBody;
use wg_2024::{network::NodeId, packet::Packet};

pub enum ClientCommand {
    AddSender(NodeId, Sender<Packet>),
    SendMessage(ClientBody),
}

pub enum ServerCommand {
    AddSender(NodeId, Sender<Packet>),
}
