use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::ServerCommand;
use dn_message::{
    Assembler, ClientBody, ClientCommunicationBody, CommunicationMessage, Message, MessageBody,
    ServerBody,
};
use dn_topology::Topology;
use std::collections::{HashMap, HashSet};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Ack, Fragment, Packet, PacketType};

// TODO: temporaneo
enum CommunicationServerEvent {}

pub struct CommunicationServer {
    pub controller_send: Sender<CommunicationServerEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,

    pub registered_clients: HashSet<NodeId>,
    pub topology: Topology,
    pub assembler: Assembler,
}

impl CommunicationServer {
    pub fn new(
        controller_send: Sender<CommunicationServerEvent>,
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
            select! {
                recv(self.controller_recv) -> command => {
                    if let Ok(cmd) = command {
                        todo!(); // TODO!
                    }
                },
                recv(self.packet_recv) -> packet => {
                    // TODO: handling of flood requests, and nack needed
                    if let Ok(p) = packet {
                        if let PacketType::MsgFragment(f) = p.pack_type.clone() {
                            self.send_ack(f, p.routing_header.clone(), p.session_id);
                        }
                        self.handle_packet(p);
                    } else {
                        break;
                    }
                }
            }
        }
    }

    // send packets (only fragments are considered) to the assembler, then pass the message to handle_message
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
                    ClientBody::ClientCommunication(comm_body) => match comm_body {
                        ClientCommunicationBody::ReqRegistrationToChat => {
                            self.register_client(routing_header.hops[0]);
                        }
                        ClientCommunicationBody::MessageSend(comm_message) => {
                            self.forward_message(message, comm_message);
                        }
                        ClientCommunicationBody::ReqClientList => {
                            self.registered_clients_list(routing_header.hops[0]);
                        }
                    },
                    ClientBody::ClientContent(client_content) => {} // ignoring messages for the content erver
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

    // TODO!: should ignoring wrong messages be replaced by send_error?
    fn send_error(&self, destination: NodeId, error_body: ServerBody) {
        unimplemented!("send error message?");
    }

    fn send_ack(
        &self,
        fragment: Fragment,
        mut sender_routing_header: SourceRoutingHeader,
        session_id: u64,
    ) {
        let ack = PacketType::Ack(Ack {
            fragment_index: fragment.fragment_index,
        });
        let hops = sender_routing_header
            .hops
            .iter()
            .cloned()
            .rev()
            .collect::<Vec<NodeId>>();

        let packet = Packet {
            pack_type: ack,
            routing_header: SourceRoutingHeader {
                // TODO: just reverse the path or calculate a new path?
                hop_index: 1,
                hops,
            },
            session_id,
        };

        self.send_packet(packet);
    }

    fn send_packet(&self, packet: Packet) {
        // assuming hop index already set at 1
        // assuming the first node connected to the server exists
        let next_hop = packet.routing_header.hops[1];
        let sender = self.packet_send.get(&next_hop).unwrap();
        if let Err(e) = sender.send(packet) {
            eprintln!(
                "Errore durante l'invio del pacchetto a {}: {:?}",
                next_hop, e
            );
        }
    }

    // possible actions:

    fn register_client(&mut self, client_id: NodeId) {
        if self.registered_clients.contains(&client_id) {
            // already registered
            // TODO: send an error or ignoring?
        }

        self.registered_clients.insert(client_id);

        // TODO: send confirmation message or not?
        // in that case use assembler.serialize_message
    }

    fn forward_message(&mut self, message: Message, communication_message: CommunicationMessage) {
        unimplemented!("Message forward");
        // sue assembler.serialize_message
    }

    fn registered_clients_list(&self, client_id: NodeId) -> Vec<NodeId> {
        unimplemented!("Send list of registered clients");
        // sue assembler.serialize_message
    }

    fn send_server_type(&self, client_id: NodeId) {
        unimplemented!("Send server type");
        // sue assembler.serialize_message
    }
}
