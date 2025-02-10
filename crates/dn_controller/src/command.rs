use crossbeam_channel::Sender;
use dn_message::{ClientBody, ServerBody};
use wg_2024::{network::NodeId, packet::Packet};

#[allow(clippy::module_name_repetitions)]
pub enum ClientCommand {
    AddSender(NodeId, Sender<Packet>),
    SendMessage(ClientBody, NodeId),
    RemoveSender(NodeId),
    Return,
}

#[allow(clippy::module_name_repetitions)]
pub enum ServerCommand {
    AddSender(NodeId, Sender<Packet>),
    RemoveSender(NodeId),
    Return,
}

pub enum ServerEvent {
    // receiver NodeId. Required because it's not present in FloodRequest
    PacketReceived(Packet, NodeId),
    PacketSent(Packet),
    MessageAssembled {
        body: ClientBody,
        from: NodeId,
        to: NodeId,
    },
    MessageFragmented {
        body: ServerBody,
        from: NodeId,
        to: NodeId,
    },
}

pub enum ClientEvent {
    // receiver NodeId. Required because it's not present in FloodRequest
    PacketReceived(Packet, NodeId),
    PacketSent(Packet),
    MessageAssembled {
        body: ServerBody,
        from: NodeId,
        to: NodeId,
    },
    MessageFragmented {
        body: ClientBody,
        from: NodeId,
        to: NodeId,
    },
}
