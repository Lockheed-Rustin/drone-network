use std::collections::{HashMap, HashSet, VecDeque};
use crossbeam_channel::{select, Receiver, Sender};
use petgraph::prelude::UnGraphMap;
use dn_message::{
    Assembler, ClientBody, ClientCommunicationBody, CommunicationMessage, Message, ServerBody,
    ServerCommunicationBody,
};
use dn_controller::{ServerCommand, ServerEvent};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, Packet, PacketType};

type PendingFragments = HashMap<u64, Fragment>;
//TODO: my topology doesn't save the information about each node type
type Topology = UnGraphMap<NodeId, ()>;

pub struct CommunicationServer  {
    // channels
    controller_send: Sender<ServerEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,

    pub id: NodeId,
    session_id_counter: u64,
    flood_id_counter: u64,
    registered_clients: HashSet<NodeId>,
    pub topology: Topology,
    topology_nodes_type: HashMap<NodeId, NodeType>,
    assembler: Assembler,

    pending_sessions: HashMap<u64, PendingFragments>, // session_id -> (fragment_index -> fragment)

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
            session_id_counter: 0,
            flood_id_counter:  0,
            registered_clients: HashSet::new(),
            topology: Topology::new(),
            topology_nodes_type: HashMap::new(),
            assembler: Assembler::new(),
            pending_sessions: HashMap::new(),
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
            PacketType::Ack(ack) => self.handle_ack(ack, &packet.session_id),
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

    /// Processes an acknowledgment for a specific session.
    ///
    /// This function handles an incoming acknowledgment by removing the corresponding fragment
    /// from the list of pending fragments associated with a session. If all fragments for the
    /// session are acknowledged, the session is removed from the pending sessions.
    fn handle_ack(&mut self, ack: Ack, session_id: &u64) {
        if let Some(fragment_map) = self.pending_sessions.get_mut(session_id) {
            fragment_map.remove(&ack.fragment_index);
            if fragment_map.is_empty() {
                self.pending_sessions.remove(session_id);
            }
        }
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
            session_id: self.session_id_counter,
        };

        self.session_id_counter += 1;

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
    pub fn handle_flood_response(&mut self, response: FloodResponse) {

        for &(node_id, node_type) in &response.path_trace {
            if !self.topology.contains_node(node_id) {
                self.topology.add_node(node_id);
            }
            self.topology_nodes_type.entry(node_id)
                .or_insert(node_type);
        }

        for window in response.path_trace.windows(2) {
            let (node_a, _) = window[0];
            let (node_b, _) = window[1];

            if !self.topology.contains_edge(node_a, node_b) {
                self.topology.add_edge(node_a, node_b, ());
            }
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

    // source routing
    pub fn source_routing(&self, to: NodeId) -> Vec<NodeId> {

        // todo!: currently using a simple BFS
        // TODO: I have to change the topology to save the type of the nodes and to be sure to not
        // use a server or a client in the path
        let mut visited = HashSet::new();
        let mut parent_map = HashMap::new();
        let mut queue = VecDeque::new();

        queue.push_back(self.id);
        visited.insert(self.id);

        while let Some(current) = queue.pop_front() {
            if current == to {
                let mut route = Vec::new();
                let mut node = current;
                while let Some(&parent) = parent_map.get(&node) {
                    route.push(node);
                    node = parent;
                }
                route.push(self.id);
                route.reverse();
                return route;
            }

            for neighbor in self.topology.neighbors(current) {
                if visited.insert(neighbor) {
                    parent_map.insert(neighbor, current);
                    queue.push_back(neighbor);
                }
            }
        }

        vec![]

    }

    /// This function just send a flood request to update the server network topology
    fn update_network_topology(&mut self) {
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
            session_id: self.session_id_counter
        };
        self.session_id_counter += 1;

        for (_, sender) in self.packet_send.iter() {
            sender
                .send(flood_request_packet.clone())
                .expect("Error in send");
        }

        self.controller_send
            .send(ServerEvent::PacketSent(flood_request_packet))
            .expect("Error in controller_send");
    }

    // TODO!: should ignoring wrong messages be replaced by send_error?
    fn send_error(&self, _destination: NodeId, _error_body: ServerBody) {
        unimplemented!("send error message?");
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
        let hops = self.source_routing(to);

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
        let next_hop = packet.routing_header.hops[1];
        let sender = self.packet_send.get(&next_hop).unwrap();
        if let Err(e) = sender.send(packet) {
            eprintln!("Error during packet sending to {}: {:?}", next_hop, e);
        }
    }

    fn send_fragments(&mut self, session_id: u64, fragments: Vec<Fragment>, routing_header: SourceRoutingHeader) {
        let fragment_map: PendingFragments = fragments
            .iter()
            .map(|f| (f.fragment_index, f.clone()))
            .collect();
        self.pending_sessions.insert(session_id, fragment_map);

        for fragment in fragments {
            let packet = Packet {
                pack_type: PacketType::MsgFragment(fragment),
                routing_header: routing_header.clone(),
                session_id,
            };
            self.send_packet(packet);
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
        // TODO: to test
        let hops = self.source_routing(communication_message.to);
        if !hops.is_empty() {
            let routing_header = SourceRoutingHeader { hop_index: 1, hops };
            let message: Message = Message::Server(ServerBody::ServerCommunication(
                ServerCommunicationBody::MessageReceive(communication_message),
            ));
            let fragments = self.assembler.serialize_message(message);

            self.send_fragments(self.session_id_counter, fragments, routing_header);
            self.session_id_counter += 1;
        }
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
