use crossbeam_channel::Sender;
use dn_message::Message;
use wg_2024::network::NodeId;
use wg_2024::packet::Packet;

pub enum Event {
    // receiver NodeId. Required because it's not present in FloodRequest
    PacketReceived(Packet, NodeId),
    MessageAssembled(Message),
    MessageFragmented(Message),
    PacketSent(Packet),
}

pub enum Command {
    AddSender(NodeId, Sender<Packet>),
    RemoveSender(NodeId),
    SendMessage(Message, NodeId),
}
