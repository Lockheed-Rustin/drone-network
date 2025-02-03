use crate::communication_server_code::communication_server_topology::CommunicationServerNetworkTopology;
use crate::communication_server_code::session_manager::{SessionId, SessionManager};
use crossbeam_channel::{select_biased, Receiver, Sender};
use dn_controller::{ServerCommand, ServerEvent};
use dn_message::assembler::Assembler;
use dn_message::{ClientBody, ClientCommunicationBody, Message};
use std::collections::{HashMap, HashSet};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{
    Ack, FloodRequest, FloodResponse, Fragment, NodeType, Packet, PacketType,
};

// TODO: nack tests to be checked
// TODO: use reference when possible
// TODO: use shortcuts in case there is not a path (just for ack/nack?)
// TODO: do something when check_routing returns false?
// TODO: I could update the infos about the pdr of that drone and use it for my sr-protocol
pub struct CommunicationServer {
    controller_send: Sender<ServerEvent>,
    controller_recv: Receiver<ServerCommand>,
    packet_send: HashMap<NodeId, Sender<Packet>>,
    packet_recv: Receiver<Packet>,

    pub(crate) id: NodeId,
    running: bool,
    flood_id_counter: u64,
    pub(crate) session_manager: SessionManager,
    pub(crate) assembler: Assembler,
    pub(crate) network_topology: CommunicationServerNetworkTopology,
    pub(crate) registered_clients: HashSet<NodeId>,
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
            running: false,
            flood_id_counter: 0,
            session_manager: SessionManager::new(),
            registered_clients: HashSet::new(),
            network_topology: CommunicationServerNetworkTopology::new(),
            assembler: Assembler::new(),
        }
    }

    pub fn run(&mut self) {
        self.running = true;
        self.update_network_topology(); // first discovery of the network
        while self.running {
            select_biased! {
                recv(self.controller_recv) -> command => {
                    if let Ok(cmd) = command {
                        self.handle_command(cmd);
                        if !self.running { break; }
                    }
                },
                recv(self.packet_recv) -> packet => {
                    if let Ok(p) = packet {
                        self.handle_packet(p);
                        if !self.running { break; }
                    }
                }
            }
        }
    }

    /// Handles incoming commands to modify the server's neighbors.
    ///
    /// This function processes commands sent to the server, allowing the addition or removal
    /// of packet senders. When a sender is added or removed, the network topology is updated to
    /// reflect the changes.
    fn handle_command(&mut self, command: ServerCommand) {
        match command {
            ServerCommand::AddSender(node_id, sender) => {
                self.packet_send.insert(node_id, sender);
                self.update_network_topology();
            }
            ServerCommand::RemoveSender(node_id) => {
                self.packet_send.remove(&node_id);
                self.network_topology.remove_node(node_id);
                self.update_network_topology();
            }
            ServerCommand::Return => {
                self.running = false;
            }
        }
    }

    /// Processes an incoming packet and performs the corresponding action.
    ///
    /// This function handles packets received by the server, determining the type of packet
    /// and delegating the appropriate action based on its content. The actions may involve
    /// processing message fragments, handling acknowledgments or negative acknowledgments,
    /// or responding to flood requests and responses.
    /// This function also notifies the simulation controller that a packet has been received.
    pub(crate) fn handle_packet(&mut self, packet: Packet) {
        self.controller_send
            .send(ServerEvent::PacketReceived(packet.clone(), self.id))
            .expect("Error in controller_send");

        if self.check_routing(&packet) {
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
    }

    fn check_routing(&self, packet: &Packet) -> bool {
        packet.routing_header.hops[packet.routing_header.hop_index] == self.id
    }

    /// Processes a message fragment and handles its acknowledgment.
    ///
    /// This function processes an incoming message fragment by attempting to assemble it into a
    /// complete message. If the message is successfully assembled, it delegates the message
    /// handling to the appropriate method. Regardless of the assembly result, it sends an
    /// acknowledgment for the processed fragment.
    fn handle_fragment(&mut self, f: Fragment, sender_id: NodeId, session_id: SessionId) {
        self.send_ack(f.clone(), sender_id, session_id);
        if let Some(message) = self.assembler.handle_fragment(f, sender_id, session_id) {
            self.handle_message(message, sender_id);
        }
    }





    /// Sends a flood response packet in reply to a flood request.
    ///
    /// This function creates a flood response packet based on the received flood request, updating
    /// the path trace to include the server's ID. It reverses the path trace to determine the
    /// return path and sends the response to the next node in the return path. Additionally, it
    /// notifies the controller about the packet sent.
    ///
    /// # Panics
    /// - The call to `send` on the `packet_send` for the next hop may panic if the channel is closed.
    ///   This should not happen unless there are unexpected issues with the communication channels.
    /// - Similarly, the call to `send` on the `controller_send` channel may panic if the channel is
    ///   unexpectedly closed.
    fn send_flood_response(&mut self, mut flood_request: FloodRequest) {
        flood_request.path_trace.push((self.id, NodeType::Server));
        // TODO: to check if the initiator id c'è
        let mut hops = flood_request
            .path_trace
            .iter()
            .map(|(node_id, _)| *node_id)
            .rev()
            .collect::<Vec<_>>();
        // make sure there is the initiator ID in the path
        if hops.last() != Some(&flood_request.initiator_id) {
            hops.push(flood_request.initiator_id);
        }

        let session_id = self.session_manager.get_and_increment_session_id_counter();
        let flood_response_packet = Packet {
            pack_type: PacketType::FloodResponse(FloodResponse {
                flood_id: flood_request.flood_id,
                path_trace: flood_request.path_trace,
            }),
            routing_header: SourceRoutingHeader { hop_index: 1, hops },
            session_id,
        };

        // assuming the first drone connected to the server exists
        if self
            .packet_send
            .contains_key(&flood_response_packet.routing_header.hops[1])
        {
            self.packet_send[&flood_response_packet.routing_header.hops[1]]
                .send(flood_response_packet.clone())
                .expect("Error in send");
            self.controller_send
                .send(ServerEvent::PacketSent(flood_response_packet))
                .expect("Error in controller_send");
        }
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
    /// This function also notifies the simulation controller that a message has been assembled
    fn handle_message(&mut self, message: Message, sender_id: NodeId) {
        match message {
            Message::Client(cb) => {
                self.controller_send
                    .send(ServerEvent::MessageAssembled(cb.clone()))
                    .expect("Error in controller_send");
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
    pub(crate) fn update_network_topology(&mut self) {
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

    fn send_ack(&mut self, fragment: Fragment, to: NodeId, session_id: SessionId) {
        let ack = PacketType::Ack(Ack {
            fragment_index: fragment.fragment_index,
        });
        let hops = self
            .network_topology
            .source_routing(self.id, to)
            .expect("Error in routing");

        if !hops.is_empty() {
            let packet = Packet {
                pack_type: ack,
                routing_header: SourceRoutingHeader { hop_index: 1, hops },
                session_id,
            };

            self.send_packet(packet);
        } else {
            // TODO: what if hops is empty?
            panic!("error in routing");
        }
    }

    pub(crate) fn send_packet(&self, packet: Packet) {
        // assuming hop index already set at 1
        // assuming the first node connected to the server exists
        if self
            .packet_send
            .contains_key(&packet.routing_header.hops[1])
        {
            self.packet_send[&packet.routing_header.hops[1]]
                .send(packet.clone())
                .expect("Error in send_packet");
            self.controller_send
                .send(ServerEvent::PacketSent(packet))
                .expect("Error in controller_send");
        }
    }

    /// Sends message fragments along a predefined route.
    ///
    /// The session is registered in the session manager before sending the fragments.
    /// Each fragment is wrapped in a packet and sent individually.
    ///
    /// # Assumptions
    /// - The `hops` list in `routing_header` is not empty.
    ///
    /// # Panics
    /// - This function may panic if `hops` is unexpectedly empty.
    fn send_fragments(
        &mut self,
        session_id: SessionId,
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

    /// Sends a message to the specified recipient using source routing.
    ///
    /// The message is serialized and split into fragments before being sent.
    /// # Panics
    /// If routing to the recipient is not possible, the function will panic.
    pub(crate) fn send_message(&mut self, message: Message, to: NodeId) {
        let hops = self
            .network_topology
            .source_routing(self.id, to)
            .expect("Error in routing");
        if !hops.is_empty() {
            if let Message::Server(sb) = message.clone() {
                let serialized_message = self.assembler.serialize_message(message);
                self.controller_send
                    .send(ServerEvent::MessageFragmented(sb))
                    .expect("Error in controller_send");
                let routing_header = SourceRoutingHeader { hop_index: 1, hops };
                let session_id = self.session_manager.get_and_increment_session_id_counter();
                self.send_fragments(session_id, serialized_message, routing_header);
            }
        } else {
            panic!("error in routing");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;
    use crossbeam_channel::unbounded;
    use std::thread;
    use std::time::Duration;
    use wg_2024::packet::PacketType::MsgFragment;

    #[test]
    fn test_source_routing() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;

        let route = server.network_topology.source_routing(server.id, 1);

        assert_eq!(route, None);

        let route = server
            .network_topology
            .source_routing(server.id, 6)
            .expect("Error in routing");

        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 6);

        let route = server
            .network_topology
            .source_routing(server.id, 4)
            .expect("Error in routing");
        // should avoid passing through 5 because it's a client
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 7);
        assert_eq!(route[3], 4);

        let helper = TestServerHelper::new();
        let mut server = helper.server;
        server
            .network_topology
            .update_node_type(7, NodeType::Server);
        let route = server
            .network_topology
            .source_routing(server.id, 4)
            .expect("Error in routing");
        // should avoid passing through 5 because it's a client, but also through 7 because it's a
        // server. So the path is empty
        assert!(route.is_empty());

        server.network_topology.update_node_type(5, NodeType::Drone);
        server.network_topology.update_node_type(7, NodeType::Drone);
        // should pass through 5 now because it's a drone and the path is shorter than the one passing
        // through 7
        let route = server
            .network_topology
            .source_routing(server.id, 4)
            .expect("Error in routing");
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 5);
        assert_eq!(route[2], 4);

        // simulating an impossible topology
        // Server_1 <---> Drone_3 <---> Client_6 <---> Drone_69 <---> target-Client_70
        server.network_topology.add_node(69, NodeType::Drone);
        server.network_topology.add_node(70, NodeType::Client);
        server.network_topology.add_edge(6, 69);
        server.network_topology.add_edge(70, 69);
        let route = server
            .network_topology
            .source_routing(server.id, 70)
            .expect("Error in routing");
        assert!(route.is_empty());
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
        let helper = TestServerHelper::new();
        let mut server = helper.server;

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
    fn test_send_ack() {
        let mut test_server_helper = TestServerHelper::new();

        let to = 6;
        let session_id = 111;
        let fragment: Fragment = TestServerHelper::test_fragment(13, 50);
        test_server_helper.server.send_ack(fragment, to, session_id);

        let ack = test_server_helper
            .packet_recv_3
            .try_recv()
            .expect("Expected recv packet");
        assert_eq!(ack.session_id, session_id);
        match ack.pack_type {
            PacketType::Ack(c) => {
                assert_eq!(c.fragment_index, 13)
            }
            _ => panic!("Expected Ack"),
        }
    }

    #[test]
    fn test_run() {
        // receiving events from the controller
        let (send_from_controller_to_server, recv_from_controller): (
            Sender<ServerCommand>,
            Receiver<ServerCommand>,
        ) = unbounded();

        // sending events to the controller
        let (send_from_server_to_controller, _recv_from_server): (
            Sender<ServerEvent>,
            Receiver<ServerEvent>,
        ) = unbounded();

        let (send_packet_to_server, packet_recv_1): (Sender<Packet>, Receiver<Packet>) =
            unbounded();
        let (packet_send_2, _packet_recv_2): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_3, _packet_recv_3): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_5, packet_recv_5): (Sender<Packet>, Receiver<Packet>) = unbounded();

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

        TestServerHelper::init_topology(&mut server);

        let _handle = thread::spawn(move || {
            server.run();
        });

        thread::sleep(Duration::from_millis(10));
        send_packet_to_server
            .send(Packet {
                routing_header: SourceRoutingHeader {
                    hop_index: 1,
                    hops: vec![5, 1],
                },
                session_id: 111,
                pack_type: MsgFragment(Fragment {
                    fragment_index: 0,
                    total_n_fragments: 1,
                    length: 0,
                    data: [0; 128],
                }),
            })
            .expect("Failed to send packet");
        thread::sleep(Duration::from_millis(10));
        let _flood_req = packet_recv_5.recv().expect("Failed to recv");
        let ack = packet_recv_5.recv().expect("Failed to recv");
        if let PacketType::Ack(a) = ack.pack_type {
            assert_eq!(a.fragment_index, 0);
        } else {
            panic!("expected ack");
        }
        thread::sleep(Duration::from_millis(10));
        send_from_controller_to_server
            .send(ServerCommand::Return)
            .expect("Failed to send command to server");
    }
}
