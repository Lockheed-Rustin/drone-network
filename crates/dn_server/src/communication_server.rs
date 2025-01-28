use std::collections::{HashMap, HashSet, VecDeque};
use crossbeam_channel::{select, Receiver, Sender};
use dn_message::{Assembler, ClientBody, ClientCommunicationBody, CommunicationMessage, Message, ServerBody, ServerCommunicationBody, ServerType};
use dn_controller::{ServerCommand, ServerEvent};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, Packet, PacketType};
use dn_message::ServerBody::RespServerType;
use dn_message::ServerCommunicationBody::RespClientList;
use crate::communication_server_topology::CommunicationServerNetworkTopology;
use crate::session_manager::SessionManager;

type PendingFragments = HashMap<u64, Fragment>;

// TODO: I could save the paths instead of doing the source routing every time
pub struct CommunicationServer  {
    // channels
    controller_send: Sender<ServerEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,

    pub id: NodeId,
    flood_id_counter: u64,
    session_manager: SessionManager,
    assembler: Assembler,

    pub network_topology: CommunicationServerNetworkTopology,

    registered_clients: HashSet<NodeId>,
}

impl CommunicationServer {
    pub fn new(
        controller_send: Sender<ServerEvent>,
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
            flood_id_counter: 0,
            session_manager: SessionManager::new(),
            registered_clients: HashSet::new(),
            network_topology: CommunicationServerNetworkTopology::new(),
            assembler: Assembler::new(),
        }
    }

    pub fn run(&mut self) {

        self.update_network_topology(); // first discovery of the network

        loop {
            select! {
                recv(self.controller_recv) -> command => {
                    if let Ok(cmd) = command {
                        self.handle_command(cmd);
                    } else {
                        break;
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

    /// Handles incoming commands to modify the server's neighbors.
    ///
    /// This function processes commands sent to the server, allowing the addition or removal
    /// of packet senders. When a sender is added or removed, the network topology is
    /// automatically updated to reflect the changes.
    fn handle_command(&mut self, command: ServerCommand) {
        match command {
            ServerCommand::AddSender(node_id, sender) => {
                self.packet_send.insert(node_id, sender);
                self.update_network_topology(); // when adding a sender the topology needs to be updated
            }
            ServerCommand::RemoveSender(node_id) => {
                self.packet_send.remove(&node_id);
                self.update_network_topology(); // when removing a sender the topology needs to be updated
            }
        }
    }

    /// Processes an incoming packet and performs the corresponding action.
    ///
    /// This function handles packets received by the server, determining the type of packet
    /// and delegating the appropriate action based on its content. The actions may involve
    /// processing message fragments, handling acknowledgments or negative acknowledgments,
    /// or responding to flood requests and responses.
    fn handle_packet(&mut self, packet: Packet) {
        // TODO: check if the packet is for you (we decided to assume that you are the receiver
        // but what if I use the wrong hops-vector later?)
        let sender_id = packet.routing_header.hops[0];
        match packet.pack_type {
            PacketType::MsgFragment(f) => {
                self.handle_fragment(f, sender_id, packet.session_id)
            }
            PacketType::Nack(nack) => self.handle_nack(nack),
            PacketType::Ack(ack) => self.session_manager.handle_ack(ack, &packet.session_id),
            PacketType::FloodRequest(f_req) => self.send_flood_response(f_req),
            PacketType::FloodResponse(f_res) => self.handle_flood_response(f_res),
        }
    }

    /// Processes a message fragment and handles its acknowledgment.
    ///
    /// This function processes an incoming message fragment by attempting to assemble it into a
    /// complete message. If the message is successfully assembled, it delegates the message
    /// handling to the appropriate method. Regardless of the assembly result, it sends an
    /// acknowledgment for the processed fragment.
    fn handle_fragment(
        &mut self,
        f: Fragment,
        sender_id: NodeId,
        session_id: u64,
    ) {
        if let Some(message) = self
            .assembler
            .handle_fragment(f.clone(), sender_id, session_id)
        {
            self.handle_message(message, sender_id);
        }
        self.send_ack(f, sender_id, session_id);
    }

    fn handle_nack(&self, nack: Nack) {
        match nack.nack_type {
            // TODO!
            NackType::ErrorInRouting(_) => { todo!() } // aggiorna la topologia
            NackType::DestinationIsDrone => { todo!() }
            NackType::Dropped => { todo!() }
            NackType::UnexpectedRecipient(_) => { todo!() }
        }
    }

    /// Sends a flood response packet in reply to a flood request.
    ///
    /// This function creates a flood response packet based on the received flood request, updating
    /// the path trace to include the server's ID. It reverses the path trace to determine the
    /// return path and sends the response to the next node in the return path. Additionally, it
    /// notifies the controller about the sent packet.
    ///
    /// # Notes
    /// - The call to `send` on the `packet_send` for the next hop may panic if the channel is closed.
    ///   This should not happen unless there are unexpected issues with the communication channels.
    /// - Similarly, the call to `send` on the `controller_send` channel may panic if the channel is
    ///   unexpectedly closed.
    pub fn send_flood_response(&mut self, mut flood_request: FloodRequest) {
        flood_request.path_trace.push((self.id, NodeType::Server));
        let hops = flood_request
            .path_trace
            .iter()
            .map(|(node_id, _)| *node_id)
            .rev()
            .collect();

        let flood_response_packet = Packet {
            pack_type: PacketType::FloodResponse(FloodResponse {
                flood_id: flood_request.flood_id,
                path_trace: flood_request.path_trace,
            }),
            routing_header: SourceRoutingHeader { hop_index: 1, hops },
            session_id: self.session_manager.get_and_increment_session_id_counter(),
        };

        self.packet_send[&flood_response_packet.routing_header.hops[1]]
            .send(flood_response_packet.clone())
            .expect("Error in send");
        self.controller_send
            .send(ServerEvent::PacketSent(flood_response_packet))
            .expect("Error in controller_send");
    }

    /// Handles a flood response packet by updating the network topology.
    ///
    /// This function processes the received flood response to update the local network topology.
    /// It adds any new nodes and edges to the topology based on the path trace contained in the
    /// response. For each pair of consecutive nodes in the path trace, it checks if the nodes and
    /// their connecting edge are already present in the topology. If not, they are added.
    /// It also saves the type of each node in `topology_nodes_type`.
    pub fn handle_flood_response(&mut self, response: FloodResponse) {

        for &(node_id, node_type) in &response.path_trace {
            self.network_topology.add_node(node_id, node_type);
        }

        for window in response.path_trace.windows(2) {
            let (node_a, _) = window[0];
            let (node_b, _) = window[1];
            self.network_topology.add_edge(node_a, node_b);
        }

    }

    /// Handles incoming messages and executes the appropriate actions based on the message type.
    /// Processes client requests, including server type queries, client registration,
    /// message forwarding, and client list requests. Ignores messages from other servers or for
    /// a content server.
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

    /// This function just send a flood request to update the server network topology
    fn update_network_topology(&mut self) {
        // TODO: maybe to move this function in Topology

        // Univocal flood id
        let flood_id = self.flood_id_counter;
        self.flood_id_counter += 1;

        let flood_request = FloodRequest {
            flood_id,
            initiator_id: self.id,
            path_trace: vec![(self.id, NodeType::Server)],
        };

        let flood_request_packet = Packet {
            pack_type: PacketType::FloodRequest(flood_request),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![],
            },
            session_id: self.session_manager.get_and_increment_session_id_counter(),
        };

        for (_, sender) in self.packet_send.iter() {
            sender
                .send(flood_request_packet.clone())
                .expect("Error in send");
        }

        self.controller_send
            .send(ServerEvent::PacketSent(flood_request_packet))
            .expect("Error in controller_send");
    }

    fn send_ack(
        &mut self,
        fragment: Fragment,
        to: NodeId,
        session_id: u64,
    ) {
        // TODO: to test

        let ack = PacketType::Ack(Ack {
            fragment_index: fragment.fragment_index,
        });
        let hops = self.network_topology.source_routing(self.id, to);

        let packet = Packet {
            pack_type: ack,
            routing_header: SourceRoutingHeader {
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
        // TODO: to test
        let next_hop = packet.routing_header.hops[1];
        let sender = self.packet_send.get(&next_hop).unwrap();
        if let Err(e) = sender.send(packet) {
            eprintln!("Error during packet sending to {}: {:?}", next_hop, e);
        }
    }

    fn send_fragments(&mut self, session_id: u64, fragments: Vec<Fragment>, routing_header: SourceRoutingHeader) {
        self.session_manager.add_session(session_id, fragments.clone());

        for fragment in fragments {
            let packet = Packet {
                pack_type: PacketType::MsgFragment(fragment),
                routing_header: routing_header.clone(),
                session_id,
            };
            self.send_packet(packet);
        }
    }

    fn send_message(&mut self, message: Message, to: NodeId) {
        let hops = self.network_topology.source_routing(self.id, to);
        if !hops.is_empty() {
            let serialized_message = self.assembler.serialize_message(message);
            let routing_header = SourceRoutingHeader { hop_index: 1, hops };
            self.send_fragments(
                self.session_manager.get_and_increment_session_id_counter(),
                serialized_message,
                routing_header,
            );
        }
        // TODO: what if hops is empty
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
        // TODO: to test
        let to = communication_message.to;
        if self.registered_clients.contains(&to) {
            let message: Message = Message::Server(ServerBody::ServerCommunication(
                ServerCommunicationBody::MessageReceive(communication_message),
            ));
            self.send_message(message.clone(), to);
        } else {
            let message: Message = Message::Server(ServerBody::ServerCommunication(
                ServerCommunicationBody::ErrWrongClientId
            ));
            self.send_message(message.clone(), to);
        }
    }

    fn registered_clients_list(&mut self, client_id: NodeId) {
        // TODO: to test
        let client_list: Vec<NodeId> = self.registered_clients.iter().cloned().collect();
        let message = Message::Server(ServerBody::ServerCommunication(
            RespClientList(
                client_list
            )
        ));
        self.send_message(message, client_id);
    }

    fn send_server_type(&mut self, client_id: NodeId) {
        // TODO: to test
        let message = Message::Server(
            RespServerType(ServerType::Communication)
        );
        self.send_message(message, client_id);
    }
}
