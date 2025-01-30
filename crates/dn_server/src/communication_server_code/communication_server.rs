use crate::communication_server_code::communication_server_topology::CommunicationServerNetworkTopology;
use crate::communication_server_code::session_manager::SessionManager;
use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::{ServerCommand, ServerEvent};
use dn_message::ServerBody::RespServerType;
use dn_message::ServerCommunicationBody::RespClientList;
use dn_message::{
    Assembler, ClientBody, ClientCommunicationBody, CommunicationMessage, Message, ServerBody,
    ServerCommunicationBody, ServerType,
};
use std::collections::{HashMap, HashSet};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{
    Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, Packet, PacketType,
};

// TODO: I could save the paths instead of doing the source routing every time
// TODO: I should check if I send to the SC all the info he wants (PacketReceived, MessageAssembled, MessageFragmented, PacketSent)
// TODO: check that the destination of a path is not a drone
// TODO: remove all the println
// TODO: various types of message like error unsupported request type
pub struct CommunicationServer {
    // channels
    controller_send: Sender<ServerEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,

    pub id: NodeId,
    flood_id_counter: u64,
    pub session_manager: SessionManager,
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
            PacketType::MsgFragment(f) => self.handle_fragment(f, sender_id, packet.session_id),
            PacketType::Nack(nack) => {
                self.handle_nack(nack, packet.session_id, packet.routing_header)
            }
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
    fn handle_fragment(&mut self, f: Fragment, sender_id: NodeId, session_id: u64) {
        if let Some(message) = self
            .assembler
            .handle_fragment(f.clone(), sender_id, session_id)
        {
            self.handle_message(message, sender_id);
        }
        self.send_ack(f, sender_id, session_id);
    }

    fn handle_nack(
        &mut self,
        nack: Nack,
        session_id: u64,
        source_routing_header: SourceRoutingHeader,
    ) {
        // TODO: to test
        // If i received a Nack something went wrong with a msg-fragment packet
        match nack.nack_type {
            NackType::ErrorInRouting(error_node) => self.error_in_routing(
                source_routing_header.hops[0],
                error_node,
                nack.fragment_index,
                session_id,
            ),
            NackType::DestinationIsDrone => {
                self.network_topology
                    .update_node_type(source_routing_header.hops[0], NodeType::Drone);
            }
            NackType::Dropped => {
                // TODO: I could update the infos about the pdr of that drone and use it for my sr-protocol
                self.recover_fragment(session_id, nack.fragment_index)
            }
            NackType::UnexpectedRecipient(_) => {
                // TODO: is the unexpected recipient nodeId useless?
                self.unexpected_recipient(nack.fragment_index, session_id)
            }
        }
    }

    fn unexpected_recipient(&mut self, fragment_index: u64, session_id: u64) {
        // TODO: to test
        // In which cases could I receive this Nack?
        self.update_network_topology();
        self.recover_fragment(session_id, fragment_index);
    }

    fn error_in_routing(
        &mut self,
        last_node: NodeId,
        error_node: NodeId,
        fragment_index: u64,
        session_id: u64,
    ) {
        // update the topology
        self.network_topology.remove_edge(last_node, error_node);
        self.update_network_topology(); // TODO: is this needed?
                                        // resend the fragment
        self.recover_fragment(session_id, fragment_index);
    }

    /// Attempts to recover a message fragment associated with a session.
    ///
    /// If the fragment is successfully recovered, it is encapsulated in a packet and sent through the network.
    /// The packet uses source routing to determine the path to its destination.
    ///
    /// # Panic
    /// This function panics if the specified fragment does not exist in the session manager.
    /// This indicates that an invalid session ID or fragment index was provided.
    fn recover_fragment(&mut self, session_id: u64, fragment_index: u64) {
        if let Some((fragment, dest)) = self
            .session_manager
            .recover_fragment(session_id, fragment_index)
        {
            let hops = self.network_topology.source_routing(self.id, dest);
            let packet = Packet {
                routing_header: SourceRoutingHeader { hop_index: 1, hops },
                session_id,
                pack_type: PacketType::MsgFragment(fragment),
            };
            self.send_packet(packet);
        } else {
            panic!(
                "tried to recover a fragment that is not in the session_manager.\n\
                    session_id: {}\n\
                    fragment_index: {}\n",
                session_id, fragment_index
            );
        }
    }

    /// Sends a flood response packet in reply to a flood request.
    ///
    /// This function creates a flood response packet based on the received flood request, updating
    /// the path trace to include the server's ID. It reverses the path trace to determine the
    /// return path and sends the response to the next node in the return path. Additionally, it
    /// notifies the controller about the packet sent.
    ///
    /// # Notes
    /// - The call to `send` on the `packet_send` for the next hop may panic if the channel is closed.
    ///   This should not happen unless there are unexpected issues with the communication channels.
    /// - Similarly, the call to `send` on the `controller_send` channel may panic if the channel is
    ///   unexpectedly closed.
    fn send_flood_response(&mut self, mut flood_request: FloodRequest) {
        flood_request.path_trace.push((self.id, NodeType::Server));
        let hops = flood_request
            .path_trace
            .iter()
            .map(|(node_id, _)| *node_id)
            .rev()
            .collect();

        let session_id = self.session_manager.get_and_increment_session_id_counter();
        let flood_response_packet = Packet {
            pack_type: PacketType::FloodResponse(FloodResponse {
                flood_id: flood_request.flood_id,
                path_trace: flood_request.path_trace,
            }),
            routing_header: SourceRoutingHeader { hop_index: 1, hops },
            session_id,
        };

        // TODO: assuming the drone connected to the server exists
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
    fn handle_flood_response(&mut self, response: FloodResponse) {
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

        let session_id = self.session_manager.get_and_increment_session_id_counter();
        let flood_request_packet = Packet {
            pack_type: PacketType::FloodRequest(flood_request),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![],
            },
            session_id,
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

    fn send_ack(&mut self, fragment: Fragment, to: NodeId, session_id: u64) {
        // TODO: to test

        let ack = PacketType::Ack(Ack {
            fragment_index: fragment.fragment_index,
        });
        let hops = self.network_topology.source_routing(self.id, to);
        // TODO: what if hops is empty?

        let packet = Packet {
            pack_type: ack,
            routing_header: SourceRoutingHeader { hop_index: 1, hops },
            session_id,
        };

        self.send_packet(packet);
    }

    fn send_packet(&self, packet: Packet) {
        // assuming hop index already set at 1
        // assuming the first node connected to the server exists (TODO: probably to check)
        // TODO: to test

        self.packet_send[&packet.routing_header.hops[1]]
            .send(packet.clone())
            .expect("Error in send_packet");
        self.controller_send
            .send(ServerEvent::PacketSent(packet))
            .expect("Error in controller_send");
    }

    fn send_fragments(
        &mut self,
        session_id: u64,
        fragments: Vec<Fragment>,
        routing_header: SourceRoutingHeader,
    ) {
        self.session_manager.add_session(
            session_id,
            fragments.clone(),
            *routing_header.hops.last().unwrap(),
        ); // assuming hops is not empty

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
            let session_id = self.session_manager.get_and_increment_session_id_counter();
            self.send_fragments(session_id, serialized_message, routing_header);
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
                ServerCommunicationBody::ErrWrongClientId,
            ));
            self.send_message(message.clone(), to);
        }
    }

    fn registered_clients_list(&mut self, client_id: NodeId) {
        // TODO: to test
        let client_list: Vec<NodeId> = self.registered_clients.iter().cloned().collect();
        let message = Message::Server(ServerBody::ServerCommunication(RespClientList(client_list)));
        self.send_message(message, client_id);
    }

    fn send_server_type(&mut self, client_id: NodeId) {
        // TODO: to test
        let message = Message::Server(RespServerType(ServerType::Communication));
        self.send_message(message, client_id);
    }
}

#[cfg(test)]
mod tests {
    use wg_2024::packet::{FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, Packet, PacketType};
    use crate::communication_server_code::communication_server_topology::CommunicationServerNetworkTopology;
    use crate::communication_server_code::communication_server::CommunicationServer;
    use crossbeam_channel::{unbounded, Receiver, Sender};
    use wg_2024::network::{NodeId, SourceRoutingHeader};
    use dn_controller::{ServerCommand, ServerEvent};
    use std::collections::HashMap;
    use rand::Rng;

    // TODO: test with wrong path like the server is a node in the middle of a path. What happens?
    fn init_server() -> (CommunicationServer, Sender<Packet>) {
        // receiving commands from controller
        let (_, controller_recv): (Sender<ServerCommand>, Receiver<ServerCommand>) = unbounded();

        // sending events to the controller
        let (controller_send, _): (Sender<ServerEvent>, Receiver<ServerEvent>) = unbounded();

        let (packet_send_1, packet_recv_1): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_2, _packet_recv_2): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_3, _packet_recv_3): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_5, _packet_recv_5): (Sender<Packet>, Receiver<Packet>) = unbounded();

        let mut packet_send_map = HashMap::new();
        packet_send_map.insert(3, packet_send_3);
        packet_send_map.insert(2, packet_send_2);
        packet_send_map.insert(5, packet_send_5);

        let mut c_s = CommunicationServer::new(
            controller_send,
            controller_recv,
            packet_send_map,
            packet_recv_1,
            1,
        );
        init_topology(&mut c_s);
        (c_s, packet_send_1)
    }

    fn init_topology(communication_server: &mut CommunicationServer) {
        let mut topology = CommunicationServerNetworkTopology::new();

        topology.add_node(1, NodeType::Server);
        topology.add_node(2, NodeType::Drone);
        topology.add_node(3, NodeType::Drone);
        topology.add_node(4, NodeType::Drone);
        topology.add_node(5, NodeType::Client);
        topology.add_node(6, NodeType::Client);
        topology.add_node(7, NodeType::Drone);

        topology.add_edge(1, 2);
        topology.add_edge(2, 3);
        topology.add_edge(3, 7);
        topology.add_edge(7, 4);
        topology.add_edge(4, 5);
        topology.add_edge(1, 5);
        topology.add_edge(3, 1);
        topology.add_edge(3, 6);

        communication_server.network_topology = topology;
    }

    fn test_received_packet(
        packet_type: PacketType,
        hops: Vec<NodeId>,
        hop_index: usize,
    ) -> (Packet, u64) {
        let session_id: u64 = rand::rng().random();
        (
            Packet {
                routing_header: SourceRoutingHeader { hop_index, hops },
                session_id,
                pack_type: packet_type,
            },
            session_id,
        )
    }

    fn test_fragment(fragment_index: u64, total_n_fragments: u64) -> Fragment {
        let data: [u8; 128] = [0; 128];
        Fragment {
            fragment_index,
            total_n_fragments,
            length: 0,
            data,
        }
    }

    #[test]
    fn test_source_routing() {
        let (mut server, _) = init_server();

        let route = server.network_topology.source_routing(server.id, 1);

        assert!(!route.is_empty());
        assert_eq!(route[0], server.id);

        let route = server.network_topology.source_routing(server.id, 3);

        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);

        let route = server.network_topology.source_routing(server.id, 4);
        // should avoid passing through 5 because it's a client
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 7);
        assert_eq!(route[3], 4);

        server
            .network_topology
            .update_node_type(7, NodeType::Server);
        let route = server.network_topology.source_routing(server.id, 4);
        // should avoid passing through 5 because it's a client, but also through 7 because it's a
        // server. So the path is empty
        assert!(route.is_empty());

        server.network_topology.update_node_type(5, NodeType::Drone);
        server.network_topology.update_node_type(7, NodeType::Drone);
        // should pass through 5 now because it's a drone and the path is shorter than the one passing
        // through 7
        let route = server.network_topology.source_routing(server.id, 4);
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 5);
        assert_eq!(route[2], 4);
    }

    #[test]
    fn test_send_flood_response() {
        let (controller_send_event, controller_recv_event) = unbounded();
        let (_controller_send_command, controller_recv_command) = unbounded();
        let (packet_send, packet_recv) = unbounded();
        let mut packet_map = HashMap::new();
        let node_id = 2;
        packet_map.insert(1, packet_send.clone());

        let mut communication_server = CommunicationServer::new(
            controller_send_event,
            controller_recv_command,
            packet_map,
            packet_recv,
            node_id,
        );

        let flood_request = FloodRequest {
            flood_id: 42,
            initiator_id: 0,
            path_trace: vec![(0, NodeType::Client), (1, NodeType::Drone)],
        };

        communication_server.send_flood_response(flood_request);
        let validate_flood_response = |sent_packet: &Packet| {
            if let PacketType::FloodResponse(flood_response) = &sent_packet.pack_type {
                assert_eq!(flood_response.flood_id, 42);
                assert_eq!(
                    flood_response.path_trace,
                    vec![
                        (0, NodeType::Client),
                        (1, NodeType::Drone),
                        (2, NodeType::Server)
                    ]
                );
            } else {
                panic!("Expected FloodResponse packet");
            }
        };
        // tests the flood response
        if let Ok(sent_packet) = communication_server.packet_recv.try_recv() {
            validate_flood_response(&sent_packet);
            assert_eq!(sent_packet.routing_header.hops, vec![2, 1, 0]);
        } else {
            panic!("No packet was sent");
        }

        // tests the server event
        if let Ok(event) = controller_recv_event.try_recv() {
            if let ServerEvent::PacketSent(packet) = event {
                validate_flood_response(&packet);
            } else {
                panic!("Expected ServerEvent::PacketSent");
            }
        } else {
            panic!("No server event was generated");
        }
    }

    #[test]
    fn test_handle_flood_response() {
        let (mut server, _) = init_server();

        let flood_response = FloodResponse {
            flood_id: 1,
            path_trace: vec![
                (1, NodeType::Server),
                (2, NodeType::Drone),
                (25, NodeType::Drone),
                (26, NodeType::Client),
            ],
        };

        assert!(!server.network_topology.contains_node(25));
        assert!(!server.network_topology.contains_node(26));
        assert!(!server.network_topology.contains_edge(2, 25));
        assert!(!server.network_topology.contains_edge(25, 26));
        assert!(!server.network_topology.contains_type(&25));
        assert!(!server.network_topology.contains_type(&26));

        server.handle_flood_response(flood_response);

        assert!(server.network_topology.contains_node(25));
        assert!(server.network_topology.contains_node(26));

        assert!(server.network_topology.contains_edge(2, 25));
        assert!(server.network_topology.contains_edge(25, 26));

        assert_eq!(
            server.network_topology.get_node_type(&25),
            Some(&NodeType::Drone)
        );
        assert_eq!(
            server.network_topology.get_node_type(&26),
            Some(&NodeType::Client)
        );
    }

    #[test]
    fn test_handle_nack() {
        // INIT
        // receiving events from the controller
        let (_send_from_controller_to_server, recv_from_controller): (
            Sender<ServerCommand>,
            Receiver<ServerCommand>,
        ) = unbounded();

        // sending events to the controller
        let (send_from_server_to_controller, _recv_from_server): (
            Sender<ServerEvent>,
            Receiver<ServerEvent>,
        ) = unbounded();

        let (_packet_send_1, packet_recv_1): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_2, _packet_recv_2): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_3, packet_recv_3): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_5, _packet_recv_5): (Sender<Packet>, Receiver<Packet>) = unbounded();

        let mut packet_send_map = HashMap::new();
        packet_send_map.insert(3, packet_send_3);
        packet_send_map.insert(2, packet_send_2);
        packet_send_map.insert(5, packet_send_5);

        let mut server = CommunicationServer::new(
            send_from_server_to_controller,
            recv_from_controller,
            packet_send_map,
            packet_recv_1,
            1,
        );
        init_topology(&mut server);

        // ERROR IN ROUTING NACK
        let fragment_index = 23;
        let (packet, session_id) = test_received_packet(
            PacketType::Nack(Nack {
                fragment_index,
                nack_type: NackType::ErrorInRouting(3),
            }),
            vec![2, 1],
            2,
        );
        let pending_fragment = test_fragment(fragment_index, 100);
        server
            .session_manager
            .add_session(session_id, vec![pending_fragment], 6);

        assert!(server.network_topology.contains_edge(2, 3));
        server.handle_packet(packet);
        assert!(!server.network_topology.contains_edge(2, 3));

        let flood_req = packet_recv_3
            .try_recv()
            .expect("Expected flood_req because of update topology");
        match flood_req.pack_type {
            PacketType::FloodRequest(_) => {}
            _ => panic!("Expected FloodRequest pack"),
        }
        let received_packet = packet_recv_3
            .try_recv()
            .expect("No recover packet received on channel 3");
        assert_eq!(received_packet.session_id, session_id);

        // DESTINATION IS DRONE
        let (packet, _session_id) = test_received_packet(
            PacketType::Nack(Nack {
                fragment_index: 0,
                nack_type: NackType::DestinationIsDrone,
            }),
            vec![5, 1],
            2,
        );
        assert_eq!(
            server.network_topology.get_node_type(&5),
            Some(&NodeType::Client)
        );
        server.handle_packet(packet);
        assert_eq!(
            server.network_topology.get_node_type(&5),
            Some(&NodeType::Drone)
        );
        // reset to client
        server
            .network_topology
            .update_node_type(5, NodeType::Client);
        assert_eq!(
            server.network_topology.get_node_type(&5),
            Some(&NodeType::Client)
        );

        // PACKET DROPPED
        let fragment_index = 25;
        let (packet, session_id) = test_received_packet(
            PacketType::Nack(Nack {
                fragment_index,
                nack_type: NackType::Dropped,
            }),
            vec![3, 1],
            2,
        );
        let fragment = test_fragment(fragment_index, 1);
        server
            .session_manager
            .add_session(session_id, vec![fragment], 6);

        server.handle_packet(packet);
        let received_packet = packet_recv_3
            .try_recv()
            .expect("No recover packet received on channel 3");
        assert_eq!(received_packet.session_id, session_id);

        // UNEXPECTED RECIPIENT
    }
}
