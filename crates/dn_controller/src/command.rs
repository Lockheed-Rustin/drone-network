use crossbeam_channel::Sender;
use dn_message::{ClientBody, Message, ServerBody};
use wg_2024::{network::NodeId, packet::Packet};

pub enum ClientCommand {
    AddSender(NodeId, Sender<Packet>),
    SendMessage(ClientBody),
    SendFragment,
    SendFloodRequest,
}

pub enum ServerCommand {
    AddSender(NodeId, Sender<Packet>),
}

pub enum ServerEvent {
    PacketReceived(Packet),
    MessageAssembled(ClientBody),
    MessageFragmented(ServerBody),
    PacketSent(Packet),
}

pub enum ClientEvent {
    PacketReceived(Packet),
    MessageAssembled(ServerBody),
    MessageFragmented(ClientBody),
    PacketSent(Packet),
}