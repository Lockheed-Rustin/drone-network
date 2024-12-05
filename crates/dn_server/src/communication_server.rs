use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::ServerCommand;
use dn_message::{
    Assembler, ClientBody, ClientCommunicationBody, CommunicationMessage, Message, ServerBody,
    ServerCommunicationBody,
};
use dn_topology::Topology;
use std::collections::{HashMap, HashSet};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Ack, Fragment, Packet, PacketType};

// TODO: TEMP
enum CommunicationServerEvent {}

pub struct CommunicationServer {
    controller_send: Sender<CommunicationServerEvent>,
    controller_recv: Receiver<ServerCommand>,
    packet_send: HashMap<NodeId, Sender<Packet>>,
    packet_recv: Receiver<Packet>,

    id: NodeId,
    session_id_counter: u64,
    registered_clients: HashSet<NodeId>,
    topology: Topology,
    assembler: Assembler,
}

impl CommunicationServer {
    pub fn new(
        controller_send: Sender<CommunicationServerEvent>,
        controller_recv: Receiver<ServerCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
        id: NodeId,
    ) -> Self {
        Self {
            controller_send,
            controller_recv,
            packet_send,
            packet_recv,
            id,
            session_id_counter: 0,
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
                    if let Ok(p) = packet {
                        self.handle_packet(p);
                    } else {
                        break;
                    }
                }
            }
        }
    }

    // send packets (only fragments are considered) to the assembler, then pass the message to "handle_message"
    fn handle_packet(&mut self, packet: Packet) {
        match packet.pack_type {
            PacketType::MsgFragment(f) => {
                let sender_id = packet.routing_header.hops[0];
                self.handle_fragment(f, sender_id, packet.session_id, packet.routing_header);
            }
            PacketType::Nack(_) => {
                todo!()
            }
            PacketType::Ack(_) => {
                todo!()
            }
            PacketType::FloodRequest(_) => {
                todo!()
            }
            PacketType::FloodResponse(_) => {
                todo!()
            }
        }
    }

    fn handle_fragment(
        &mut self,
        f: Fragment,
        sender_id: NodeId,
        session_id: u64,
        routing_header: SourceRoutingHeader,
    ) {
        if let Some(message) = self
            .assembler
            .handle_fragment(f.clone(), sender_id, session_id)
        {
            self.handle_message(message, sender_id);
        }
        self.send_ack(f, routing_header, session_id);
    }

    // given a message finds out what function to call
    fn handle_message(&mut self, message: Message, sender_id: NodeId) {
        match message {
            Message::Client(cb) => {
                match cb {
                    ClientBody::ReqServerType => {
                        self.send_server_type(sender_id);
                    }
                    ClientBody::ClientCommunication(comm_body) => match comm_body {
                        ClientCommunicationBody::ReqRegistrationToChat => {
                            self.register_client(sender_id);
                        }
                        ClientCommunicationBody::MessageSend(comm_message) => {
                            self.forward_message(comm_message);
                        }
                        ClientCommunicationBody::ReqClientList => {
                            self.registered_clients_list(sender_id);
                        }
                    },
                    ClientBody::ClientContent(_) => {} // ignoring messages for the content server
                }
            }
            Message::Server(_) => {} // ignoring messages received by other servers
        }
    }

    // source routing
    fn source_routing(&self, to: NodeId) -> Vec<NodeId> {
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
        // assuming the first node connected to the server exists (TODO: probably to check)
        let next_hop = packet.routing_header.hops[1];
        let sender = self.packet_send.get(&next_hop).unwrap();
        if let Err(e) = sender.send(packet) {
            eprintln!("Error during packet sending to {}: {:?}", next_hop, e);
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

    fn forward_message(&mut self, communication_message: CommunicationMessage) {
        let hops = self.source_routing(communication_message.to);
        let routing_header = SourceRoutingHeader { hop_index: 1, hops };
        let message: Message = Message::Server(ServerBody::ServerCommunication(
            ServerCommunicationBody::MessageReceive(communication_message),
        ));

        let fragments = self.assembler.serialize_message(message);

        for fragment in fragments {
            let packet = Packet {
                pack_type: PacketType::MsgFragment(fragment),
                routing_header: routing_header.clone(),
                session_id: self.session_id_counter,
            };
            self.send_packet(packet);
        }

        self.session_id_counter += 1;
    }

    fn registered_clients_list(&self, client_id: NodeId) -> Vec<NodeId> {
        unimplemented!("Send list of registered clients");
        // use "serialize_message"
    }

    fn send_server_type(&self, client_id: NodeId) {
        unimplemented!("Send server type");
        // use "serialize_message"
    }
}
