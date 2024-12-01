use crossbeam_channel::{Receiver, Sender};
use dn_controller::ServerCommand;
use dn_message::{
    ClientBody, ClientCommunicationBody, ClientContentBody, CommunicationMessage, Message,
    MessageBody, ServerBody,
};
use dn_topology::Topology;
use std::collections::{HashMap, HashSet};
use wg_2024::controller::NodeEvent;
use wg_2024::network::{NodeId};
use wg_2024::packet::Fragment;
use wg_2024::packet::{Packet, PacketType};
use wg_2024::packet::FRAGMENT_DSIZE as MAX_FRAGMENT_SIZE;

pub struct CommunicationServer {
    pub controller_send: Sender<NodeEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
    pub registered_clients: HashSet<NodeId>,
    pub topology: Topology,
    pub assembler: Assembler,
}

impl CommunicationServer {
    pub fn new(
        controller_send: Sender<NodeEvent>,
        controller_recv: Receiver<ServerCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
    ) -> Self {
        Self {
            controller_send,
            controller_recv,
            packet_send,
            packet_recv,
            registered_clients: HashSet::new(),
            topology: Topology::new(),
            assembler: Assembler::new(),
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

    // send packets to the assembler, then pass the message to handle_message
    fn handle_packet(&mut self, packet: Packet) {
        if let Some(message) = self.assembler.handle_packet(packet) {
            self.handle_message(message);
        }
    }

    // given a message finds out what function to call
    fn handle_message(&mut self, message: Message) {
        let routing_header = message.routing_header.clone();
        match message.body.clone() {
            MessageBody::Client(cb) => {
                match cb {
                    ClientBody::ReqServerType => {
                        self.send_server_type(routing_header.hops[0]);
                    }
                    ClientBody::ClientContent(client_content) => {
                        match client_content {
                            ClientContentBody::ReqFilesList => {} // ignored
                            ClientContentBody::ReqFile(_) => {}   // ignored
                            ClientContentBody::ReqClientList => {
                                self.registered_clients_list(routing_header.hops[0]);
                            } // TODO!: why in the ClientContentBody?
                        }
                    }
                    ClientBody::ClientCommunication(comm_body) => match comm_body {
                        ClientCommunicationBody::ReqRegistrationToChat => {
                            self.register_client(routing_header.hops[0]);
                        }
                        ClientCommunicationBody::MessageSend(comm_message) => {
                            self.forward_message(message, comm_message);
                        }
                    },
                }
            }
            MessageBody::Server(_) => {} // ignoring messages received by other servers
        }
    }

    // source routing
    fn source_routing(&self) {
        // update_network_topology and then find a path to send the message
        unimplemented!()
    }

    fn update_network_topology(&mut self) {
        // follow network discovery protocol
        // update the topology of the network
        unimplemented!()
    }

    // possible actions:

    // TODO!: should ignoring wrong messages be replaced by send_error?
    fn send_error(&self, destination: NodeId, error_body: ServerBody) {
        unimplemented!("send error message?");
    }

    fn register_client(&mut self, client_id: NodeId) {
        unimplemented!("Register client");
    }

    fn forward_message(&mut self, message: Message, communication_message: CommunicationMessage) {
        unimplemented!("Message forward");
    }

    fn registered_clients_list(&self, client_id: NodeId) -> Vec<NodeId> {
        unimplemented!("Send list of registered clients");
    }

    fn send_server_type(&self, client_id: NodeId) {
        unimplemented!("Send server type");
    }
}

pub struct Assembler {
    in_progress_messages: HashMap<(NodeId, u64), MessageBuffer>,
}

impl Assembler {
    pub fn new() -> Self {
        Assembler {
            in_progress_messages: HashMap::new(),
        }
    }

    pub fn handle_packet(&mut self, packet: Packet) -> Option<Message> {
        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                let session_id = packet.session_id;
                let buffer = self
                    .in_progress_messages
                    .entry((packet.routing_header.hops[0], session_id))
                    .or_insert_with(|| MessageBuffer::new(fragment.total_n_fragments as usize));

                buffer.add_fragment(fragment);

                if buffer.is_complete() {
                    let message = buffer.to_message();
                    self.in_progress_messages
                        .remove(&(packet.routing_header.hops[0], session_id));
                    Some(message)
                } else {
                    None
                }
            }
            _ => None, // currently ignoring other type of packets
        }
    }

    pub fn serialize_message(&self, message: Message, hops: Vec<NodeId>) -> Vec<Fragment> {
        let message_data = self.serialize_message_data(&message);
        let total_fragments =
            ((message_data.len() + MAX_FRAGMENT_SIZE - 1) / MAX_FRAGMENT_SIZE) as u64;

        let mut fragments = Vec::new();

        for (i, chunk) in message_data.chunks(MAX_FRAGMENT_SIZE).enumerate() {
            let mut data = [0u8; MAX_FRAGMENT_SIZE];
            data[..chunk.len()].copy_from_slice(chunk);
            let fragment = Fragment {
                fragment_index: i as u64,
                total_n_fragments: total_fragments,
                length: chunk.len() as u8,
                data
            };

            fragments.push(fragment);
        }

        fragments
    }

    fn serialize_message_data(&self, message: &Message) -> Vec<u8> {
        unimplemented!()
    }
}

pub struct MessageBuffer {
    fragments: Vec<u8>,
    total_fragments: u64,
    received_indices: HashSet<u64>,
}

impl MessageBuffer {
    pub fn new(total_n_fragments: usize) -> Self {
        MessageBuffer {
            fragments: vec![0; MAX_FRAGMENT_SIZE * total_n_fragments],
            total_fragments: total_n_fragments as u64,
            received_indices: HashSet::new(),
        }
    }

    pub fn add_fragment(&mut self, fragment: Fragment) {
        let start_index = MAX_FRAGMENT_SIZE * fragment.fragment_index as usize;
        let end_index = start_index + fragment.length as usize;

        if !self.received_indices.insert(fragment.fragment_index) {
            return; //Ignoring duplicates: assuming the first packet had the correct data
        }

        self.total_fragments = fragment.total_n_fragments;

        self.fragments[start_index..end_index]
            .copy_from_slice(&fragment.data[..fragment.length as usize]);

    }

    pub fn is_complete(&self) -> bool {
        self.received_indices.len() == self.total_fragments as usize
    }

    pub fn to_message(&self) -> Message {
        // TODO
        unimplemented!()
    }
}
