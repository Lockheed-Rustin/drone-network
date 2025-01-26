use crossbeam_channel::Sender;
use dn_message::{ClientBody, ServerBody};
use wg_2024::{network::NodeId, packet::Packet};

pub enum ClientCommand {
    AddSender(NodeId, Sender<Packet>),
    SendMessage(ClientBody, NodeId),
    SendFragment,
    SendFloodRequest,
    RemoveSender(NodeId),
}

pub enum ServerCommand {
    AddSender(NodeId, Sender<Packet>),
    RemoveSender(NodeId),
}

pub enum ServerEvent {
    // receiver NodeId. Required because it's not present in FloodRequest
    PacketReceived(Packet, NodeId),
    MessageAssembled(ClientBody),
    MessageFragmented(ServerBody),
    PacketSent(Packet),
}

pub enum ClientEvent {
    // receiver NodeId. Required because it's not present in FloodRequest
    PacketReceived(Packet, NodeId),
    MessageAssembled(ServerBody),
    MessageFragmented(ClientBody),
    PacketSent(Packet),
}
