use crossbeam_channel::{Receiver, Sender};
use dn_message::{
    ClientBody, ClientCommunicationBody, CommunicationMessage, Message, MessageBody, ServerBody,
};
use std::collections::{HashMap, HashSet};
use wg_2024::controller::NodeEvent;
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::Fragment;
use wg_2024::packet::{Packet, PacketType};

pub struct CommunicationServer {
    pub controller_send: Sender<NodeEvent>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
    pub registered_clients: HashSet<NodeId>,
    // topology: // TODO!: how do we save the topology of the network?
}

impl CommunicationServer {
    pub fn new(
        controller_send: Sender<NodeEvent>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
    ) -> Self {
        Self {
            controller_send,
            packet_send,
            packet_recv,
            registered_clients: HashSet::new(),
        }
    }

    pub fn run(&mut self) {
        loop {
            if let Ok(packet) = self.packet_recv.recv() {
                self.handle_packet(packet);
            } else {
                break;
            }
        }
    }

    // deal with the logic of receiving packets and sending them to the assembler,
    // then pass the message to handle_message
    fn handle_packet(&mut self, packet: Packet) {
        unimplemented!()
    }

    // given a message finds out what function to call
    fn handle_message(&mut self, message: Message) {
        unimplemented!()
    }

    // source routing
    fn source_routing(&self) {
        // update_network_topology and then find a path to send the message
        unimplemented!()
    }

    fn update_network_topology(&mut self){
        // follow network discovery protocol
        // update the topology of the network
        unimplemented!()
    }

    // possible actions:

    fn send_error(&self, destination: NodeId, error_body: ServerBody) {
        unimplemented!("send error message?");
    }

    fn register_client(&mut self, client_id: NodeId) {
        unimplemented!("Register client");
    }

    fn forward_message(&mut self, sender: NodeId, message: CommunicationMessage) {
        unimplemented!("Message forward");
    }

    fn registered_client_list(&self) -> Vec<NodeId> {
        unimplemented!("Send list of registered clients");
    }
}

// The code below is just an example of what an Assembler might look like

const MAX_FRAGMENT_SIZE: usize = 80; // size defined in the protocol

pub struct TemporaryAssembler {
    in_progress_messages: HashMap<u64, MessageBuffer>,
}

impl TemporaryAssembler {
    pub fn new() -> Self {
        TemporaryAssembler {
            in_progress_messages: HashMap::new(),
        }
    }

    pub fn handle_packet(&mut self, packet: Packet, sender: NodeId) -> Option<Message> {
        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                let session_id = packet.session_id;
                let buffer = self
                    .in_progress_messages
                    .entry(session_id)
                    .or_insert_with(MessageBuffer::new);

                buffer.add_fragment(fragment);

                if buffer.is_complete() {
                    let message = buffer.to_message(sender);
                    self.in_progress_messages.remove(&session_id);
                    Some(message)
                } else {
                    None
                }
            }
            _ => None, // currently ignoring other type of packets
        }
    }

    pub fn serialize_message(&self, message: Message) -> Vec<Packet> {
        let message_data = self.serialize_message_data(&message);
        let total_fragments = ((message_data.len() + MAX_FRAGMENT_SIZE - 1) / MAX_FRAGMENT_SIZE) as u64;

        let mut packets = Vec::new();

        for (i, chunk) in message_data.chunks(MAX_FRAGMENT_SIZE).enumerate() {
            let fragment = Fragment {
                fragment_index: i as u64,
                total_n_fragments: total_fragments,
                length: chunk.len() as u8,
                data: chunk.try_into().unwrap_or_else(|_| [0; MAX_FRAGMENT_SIZE]), //
            };

            let packet = Packet {
                pack_type: PacketType::MsgFragment(fragment),
                routing_header: unimplemented!(),   // TODO!: Set routing header
                session_id: message.session_id,
            };

            packets.push(packet);
        }

        packets
    }

    fn serialize_message_data(&self, message: &Message) -> Vec<u8> {
        unimplemented!()
    }
}

// NB: different from the solution proposed in the protocol:
// To reassemble fragments into a single packet, a client or server uses the fragment header as follows:
// The client or server receives a fragment.
// It first checks the session_id in the header.
// If it has not received a fragment with the same session_id, then it creates a vector
// (Vec<u8> with capacity of total_n_fragments * 80) where to copy the data of the fragments;
// It would then copy length elements of the data array at the correct offset in the vector.
// Note: if there are more than one fragment, length must be 80 for all fragments except for the last.
// If the client or server has already received a fragment with the same session_id, then it just
// needs to copy the data of the fragment in the vector.
// Once that the client or server has received all fragments
// (that is, fragment_index 0 to total_n_fragments - 1), then it has reassembled the whole fragment.
// Therefore, the packet is now a message that can be delivered.
pub struct MessageBuffer {
    fragments: HashMap<u64, Fragment>, // u64 is the index of the fragment
    total_fragments: u64,
}

impl MessageBuffer {
    pub fn new() -> Self {
        MessageBuffer {
            fragments: HashMap::new(),
            total_fragments: 0,
        }
    }

    pub fn add_fragment(&mut self, fragment: Fragment) {
        let total_fragments = self.total_fragments;
        self.fragments.insert(fragment.fragment_index, fragment);
        self.total_fragments = self.total_fragments.max(total_fragments); // maybe we don't need to do this every time
    }

    pub fn is_complete(&self) -> bool {
        self.fragments.len() == self.total_fragments as usize
    }

    pub fn to_message(&self, sender: NodeId) -> Message {
        unimplemented!()
    }
}
