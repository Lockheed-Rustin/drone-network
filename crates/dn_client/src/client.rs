use crate::{ClientRouting, MessageManager, ServerTypeError};
use crossbeam_channel::{select_biased, Receiver, Sender};
use dn_controller::{ClientCommand, ClientEvent};
use dn_message::{
    Assembler, ClientBody, ClientCommunicationBody, Message, ServerBody, ServerCommunicationBody,
    ServerType,
};
use std::collections::HashMap;
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{
    Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, PacketType,
};
use wg_2024::{network::NodeId, packet::Packet};

pub enum PathError {
    UnexpectedRecipient,
    InvalidPath,
}

pub struct Client {
    pub id: NodeId,
    pub controller_send: Sender<ClientEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,

    pub session_id: u64,
    pub flood_id: u64,
    pub assembler: Assembler,
    pub source_routing: ClientRouting,
    message_manager: MessageManager,
}

impl Client {
    #[must_use]
    pub fn new(
        id: NodeId,
        controller_send: Sender<ClientEvent>,
        controller_recv: Receiver<ClientCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
    ) -> Self {
        let mut source_routing = ClientRouting::new(id);

        for neighbor in packet_send.keys() {
            source_routing.add_channel_to_neighbor(*neighbor);
        }

        Self {
            id,
            controller_send,
            controller_recv,
            packet_send,
            packet_recv,
            session_id: 0,
            flood_id: 0,
            assembler: Assembler::new(),
            source_routing,
            message_manager: MessageManager::new(),
        }
    }

    pub fn run(&mut self) {
        self.send_flood_request();

        loop {
            select_biased! {
                recv(self.controller_recv) -> command => {
                    if let Ok(cmd) = command {
                        match cmd {
                            ClientCommand::Return => {return;},
                             _ => self.handle_command(cmd),
                        }
                    }
                },
                recv(self.packet_recv) -> packet => {
                    if let Ok(pckt) = packet {
                        self.handle_packet(pckt);
                    }
                }
            }
        }
    }

    //---------- handle receiver ----------//
    fn handle_command(&mut self, command: ClientCommand) {
        match command {
            ClientCommand::SendMessage(client_body, to) => {
                self.handle_send_message(client_body, to);
            }
            ClientCommand::RemoveSender(n) => self.remove_sender(n),
            ClientCommand::AddSender(n, sender) => self.add_sender(n, sender),
            ClientCommand::Return => {}
        }
    }

    fn handle_packet(&mut self, packet: Packet) {
        //notify controller about receiving packet
        self.controller_send
            .send(ClientEvent::PacketReceived(packet.clone(), self.id))
            .expect("Error in controller_send");

        if let Err(err) = self.check_routing(&packet) {
            if let PathError::UnexpectedRecipient = err {
                self.send_unexp_recp(&packet);
            }
            return;
        }

        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                self.handle_fragment(&fragment, &packet.routing_header, packet.session_id);
            }

            PacketType::Ack(ack) => {
                self.handle_ack(&ack, &packet.routing_header, packet.session_id);
            }

            PacketType::Nack(nack) => {
                self.handle_nack(&nack, &packet.routing_header, packet.session_id);
            }

            PacketType::FloodRequest(flood_request) => {
                self.handle_flood_request(packet.session_id, flood_request);
            }

            PacketType::FloodResponse(flood_response) => {
                self.handle_flood_response(&flood_response);
            }
        }
    }

    //---------- check routing ----------//
    fn check_routing(&self, packet: &Packet) -> Result<(), PathError> {
        match packet.pack_type {
            PacketType::FloodRequest(_) => Ok(()),
            PacketType::MsgFragment(_) => {
                if packet.routing_header.len() < 2 {
                    return Err(PathError::InvalidPath);
                }

                match packet.routing_header.current_hop() {
                    Some(curr_hop) => {
                        if curr_hop == self.id {
                            if packet.routing_header.is_last_hop() {
                                Ok(())
                            } else {
                                Err(PathError::InvalidPath)
                            }
                        } else {
                            Err(PathError::UnexpectedRecipient)
                        }
                    }
                    None => Err(PathError::InvalidPath),
                }
            }
            _ => {
                if packet.routing_header.len() > 1 {
                    Ok(())
                } else {
                    Err(PathError::InvalidPath)
                }
            }
        }
    }

    //---------- add/rmv sender from client ----------//
    fn remove_sender(&mut self, n: NodeId) {
        if self.packet_send.len() < 2 {
            return;
        }

        self.packet_send.remove(&n);
        self.source_routing.remove_channel_to_neighbor(n);
    }

    fn add_sender(&mut self, n: NodeId, sender: Sender<Packet>) {
        if !self.packet_send.contains_key(&n) {
            self.packet_send.insert(n, sender);
            if let Some(servers_became_reachable) = self.source_routing.add_channel_to_neighbor(n) {
                self.send_unsent(servers_became_reachable);
            }
        }
    }

    //---------- send ----------//
    fn send_unexp_recp(&self, packet: &Packet) {
        let mut path = packet
            .routing_header
            .hops
            .iter()
            .copied()
            .take(packet.routing_header.hop_index)
            .rev()
            .collect::<Vec<_>>();
        path.insert(0, self.id);

        let nack = Packet {
            routing_header: SourceRoutingHeader {
                hop_index: 0,
                hops: path,
            },
            session_id: packet.session_id,
            pack_type: PacketType::Nack(Nack {
                fragment_index: packet.get_fragment_index(),
                nack_type: NackType::UnexpectedRecipient(self.id),
            }),
        };

        self.send_packet(nack);
    }

    fn send_unsent(&mut self, servers: Vec<(NodeId, Vec<NodeId>)>) {
        for (server, path) in servers {
            if path.len() >= 2 {
                if let Some(unsents) = self.message_manager.get_unsent_fragments(server) {
                    for (session_id, fragment) in unsents {
                        let packet = Packet {
                            routing_header: SourceRoutingHeader {
                                hop_index: 0,
                                hops: path.clone(),
                            },
                            session_id,
                            pack_type: PacketType::MsgFragment(fragment),
                        };

                        self.send_packet(packet);
                    }
                }
            }
        }
    }

    fn send_message(&mut self, client_body: ClientBody, dest: NodeId) {
        //fragment message and notify controller
        let fragments = self
            .assembler
            .serialize_message(&Message::Client(client_body.clone()));

        self.controller_send
            .send(ClientEvent::MessageFragmented {
                body: client_body,
                from: self.id,
                to: dest,
            })
            .expect("Error in controller_send");

        self.message_manager
            .add_pending_session(self.session_id, dest, &fragments);

        let mut pkt_not_sended = false;
        for fragment in fragments {
            if !self.send_fragment(dest, fragment, self.session_id) {
                pkt_not_sended = true;
            }
        }
        if pkt_not_sended {
            self.send_flood_request();
        }

        self.session_id += 1;
    }

    fn send_flood_request(&mut self) {
        let flood_request_packet = Packet {
            routing_header: SourceRoutingHeader {
                hop_index: 0,
                hops: Vec::new(),
            },
            session_id: self.session_id,
            pack_type: PacketType::FloodRequest(FloodRequest {
                flood_id: self.flood_id,
                initiator_id: self.id,
                path_trace: vec![(self.id, NodeType::Client)],
            }),
        };

        self.flood_id += 1;
        self.session_id += 1;

        self.message_manager.reset_already_dropped();

        for sender in self.packet_send.values() {
            sender
                .send(flood_request_packet.clone())
                .expect("Error in send");

            self.controller_send
                .send(ClientEvent::PacketSent(flood_request_packet.clone()))
                .expect("Error in controller_send");
        }

        self.source_routing.clear_topology();
    }

    fn send_fragment(&mut self, dest: NodeId, fragment: Fragment, session_id: u64) -> bool {
        if let Some(path) = self.source_routing.get_path(dest) {
            let packet = Packet {
                routing_header: SourceRoutingHeader {
                    hop_index: 0,
                    hops: path.clone(),
                },
                session_id,
                pack_type: PacketType::MsgFragment(fragment),
            };

            self.send_packet(packet);

            true
        } else {
            self.message_manager
                .add_unsent_fragment(session_id, dest, &fragment);

            false
        }
    }

    fn send_packet(&self, mut packet: Packet) {
        if let Some(next_hop) = packet.routing_header.next_hop() {
            packet.routing_header.increase_hop_index();

            self.packet_send
                .get(&next_hop)
                .unwrap()
                .send(packet.clone())
                .expect("Error in send");

            self.controller_send
                .send(ClientEvent::PacketSent(packet))
                .expect("Error in controller_send");
        }
    }

    //---------- handle ----------//
    fn handle_send_message(&mut self, client_body: ClientBody, dest: NodeId) {
        if let Err(err) = self.message_manager.is_valid_send(&client_body, dest) {
            match err {
                ServerTypeError::ServerTypeUnknown => {
                    self.message_manager.add_unsent_message(&client_body, dest);

                    self.send_message(ClientBody::ReqServerType, dest);
                }
                ServerTypeError::WrongServerType => {
                    self.controller_send
                        .send(ClientEvent::MessageAssembled {
                            body: ServerBody::ErrUnsupportedRequestType,
                            from: dest,
                            to: self.id,
                        })
                        .expect("Error in controller_send");
                }
            }
        } else {
            match &client_body {
                ClientBody::ClientCommunication(_) => {
                    if self.message_manager.is_reg_to_comm(dest) {
                        self.send_message(client_body, dest);
                    } else {
                        self.message_manager.add_unsent_message(&client_body, dest);

                        self.send_message(
                            ClientBody::ClientCommunication(
                                ClientCommunicationBody::ReqRegistrationToChat,
                            ),
                            dest,
                        );
                    }
                }

                _ => {
                    self.send_message(client_body, dest);
                }
            }
        }
    }

    fn handle_fragment(
        &mut self,
        fragment: &Fragment,
        header: &SourceRoutingHeader,
        session_id: u64,
    ) {
        if header.hops.len() < 2 {
            return;
        }

        self.source_routing.correct_exchanged_with(&header.hops);

        let &sender = header.hops.first().unwrap(); // always have first since path.len() >= 2

        let ack = Packet {
            routing_header: SourceRoutingHeader {
                hop_index: 0,
                hops: header.hops.iter().copied().rev().collect::<Vec<_>>(),
            },
            session_id,
            pack_type: PacketType::Ack(Ack {
                fragment_index: fragment.fragment_index,
            }),
        };

        self.send_packet(ack);

        if let Some(Message::Server(server_body)) =
            self.assembler.handle_fragment(fragment, sender, session_id)
        {
            self.controller_send
                .send(ClientEvent::MessageAssembled {
                    body: server_body.clone(),
                    from: sender,
                    to: self.id,
                })
                .expect("Error in controller_send");

            match &server_body {
                ServerBody::RespServerType(server_type) => {
                    self.message_manager.add_server_type(sender, server_type);

                    match server_type {
                        ServerType::Communication
                            if !self.message_manager.is_reg_to_comm(sender) =>
                        {
                            if self.message_manager.is_there_unsent_message(sender) {
                                self.send_message(
                                    ClientBody::ClientCommunication(
                                        ClientCommunicationBody::ReqRegistrationToChat,
                                    ),
                                    sender,
                                );
                            }
                        }
                        _ => {
                            if let Some(unsent) = self.message_manager.get_unsent_message(sender) {
                                for client_body in unsent {
                                    self.send_message(client_body, sender);
                                }
                            }
                        }
                    }
                }
                ServerBody::ServerCommunication(comm_server_body) => match comm_server_body {
                    ServerCommunicationBody::ErrNotRegistered => {
                        self.send_message(
                            ClientBody::ClientCommunication(
                                ClientCommunicationBody::ReqRegistrationToChat,
                            ),
                            sender,
                        );
                    }
                    ServerCommunicationBody::RegistrationSuccess => {
                        if let Some(unsent) = self.message_manager.get_unsent_message(sender) {
                            for client_body in unsent {
                                self.send_message(client_body, sender);
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }

    fn handle_flood_response(&mut self, flood_response: &FloodResponse) {
        if let Some(servers_became_reachable) =
            self.source_routing.add_path(&flood_response.path_trace)
        {
            self.send_unsent(servers_became_reachable);
        }
    }

    fn handle_ack(&mut self, ack: &Ack, header: &SourceRoutingHeader, session_id: u64) {
        if header.hops.len() < 2 {
            return;
        }

        let &server = header.hops.first().unwrap();

        self.message_manager
            .confirm_ack(session_id, ack.fragment_index);

        self.source_routing.correct_send_to(server);
    }

    fn handle_nack(&mut self, nack: &Nack, header: &SourceRoutingHeader, session_id: u64) {
        match nack.nack_type {
            NackType::ErrorInRouting(node) => {
                self.source_routing.correct_exchanged_with(&header.hops);

                self.send_flood_request();

                self.source_routing.remove_node(node);
            }
            NackType::DestinationIsDrone => {
                self.source_routing.correct_exchanged_with(&header.hops);

                self.send_flood_request();
                //in this scenario, fragment will be added to the unsents fragments
            }
            NackType::Dropped => {
                self.source_routing.inc_packet_dropped(&header.hops);

                if self
                    .message_manager
                    .update_fragment_dropped(session_id, nack.fragment_index)
                {
                    self.send_flood_request();
                }
            }
            NackType::UnexpectedRecipient(_) => {
                self.source_routing.correct_exchanged_with(&header.hops);
            }
        }

        if let Some((dest, fragment)) = self
            .message_manager
            .get_pending_fragment(session_id, nack.fragment_index)
        {
            self.send_fragment(dest, fragment.clone(), session_id);
        }
    }

    fn handle_flood_request(&self, session_id: u64, mut flood_request: FloodRequest) {
        flood_request.increment(self.id, NodeType::Client);
        let flood_response = flood_request.generate_response(session_id);

        self.send_packet(flood_response);
    }
}

//---------------------------//
//---------- TESTS ----------//
//---------------------------//
#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    //---------- CLIENT TEST ----------//
    #[test]
    fn client_test() {
        /*
        Topology with 7 nodes: 1(Client), 2(Drone), 3(Drone), 4(Drone), 5(Drone), 6(Server), 7(Server)
        paths: 1-2-3-(6/7), 1-4-5-(6/7), arco 2-5 (to test crash Drone 3);
        */

        //---------- SENDERS - RECEIVERS ----------//

        //---------- sending/receiving events from the controller ----------//
        let (_, serv_recv_command): (Sender<ClientCommand>, Receiver<ClientCommand>) = unbounded();

        let (serv_send_event, ctrl_recv_event): (Sender<ClientEvent>, Receiver<ClientEvent>) =
            unbounded();

        //---------- sending/receiving to neighbor ----------//
        let mut server_senders: HashMap<NodeId, Sender<Packet>> = HashMap::new();

        //channel to client
        let (_send_to_client, serv_recv): (Sender<Packet>, Receiver<Packet>) = unbounded();

        //channel to node2
        let (serv_send_2, recv_2): (Sender<Packet>, Receiver<Packet>) = unbounded();
        server_senders.insert(2, serv_send_2.clone());

        //channel to node3
        let (serv_send_3, recv_3): (Sender<Packet>, Receiver<Packet>) = unbounded();
        server_senders.insert(3, serv_send_3.clone());

        //---------- INIT ----------//
        let mut client = Client::new(
            1,
            serv_send_event,
            serv_recv_command,
            server_senders,
            serv_recv,
        );

        assert_eq!(client.id, 1);
        assert_eq!(client.packet_send.len(), 2);
        assert!(client.packet_send.contains_key(&2));
        assert!(client.packet_send.contains_key(&3));

        //---------- TEST CTRL COMMAND ----------//

        //---------- remove/add sender ----------//
        client.remove_sender(2);
        assert_eq!(client.packet_send.len(), 1);
        assert!(!client.packet_send.contains_key(&2));
        assert!(client.packet_send.contains_key(&3));

        client.add_sender(2, serv_send_2.clone());
        assert_eq!(client.packet_send.len(), 2);
        assert!(client.packet_send.contains_key(&2));
        assert!(client.packet_send.contains_key(&3));

        //---------- send flood request ----------//
        client.send_flood_request();
        match recv_2.recv() {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }
        match recv_3.recv() {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }
        match ctrl_recv_event.recv() {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }

        //---------- HANDLE PACKET ----------//

        //---------- check packet validity ----------//
        let flood_request = Packet::new_flood_request(
            SourceRoutingHeader::initialize(vec![]),
            0,
            FloodRequest::initialize(0, 1, NodeType::Client),
        );
        assert!(client.check_routing(&flood_request));

        let ack_short_ = Packet::new_ack(
            SourceRoutingHeader {
                hop_index: 0,
                hops: vec![1],
            },
            0,
            0,
        );
        assert!(!client.check_routing(&ack_short_));

        let fragment_short_ = Packet::new_fragment(
            SourceRoutingHeader {
                hop_index: 0,
                hops: vec![1],
            },
            0,
            Fragment::new(0, 1, [0; 128]),
        );
        assert!(!client.check_routing(&fragment_short_));

        let valid_fragment = Packet::new_fragment(
            SourceRoutingHeader {
                hop_index: 2,
                hops: vec![5, 3, 1],
            },
            0,
            Fragment::new(0, 1, [0; 128]),
        );
        assert!(client.check_routing(&valid_fragment));

        let travel_fragment = Packet::new_fragment(
            SourceRoutingHeader {
                hop_index: 2,
                hops: vec![5, 3, 1, 6, 4],
            },
            0,
            Fragment::new(0, 1, [0; 128]),
        );
        assert!(!client.check_routing(&travel_fragment));

        let not_for_me_fragment = Packet::new_fragment(
            SourceRoutingHeader {
                hop_index: 2,
                hops: vec![5, 3, 2],
            },
            0,
            Fragment::new(0, 1, [0; 128]),
        );
        assert!(!client.check_routing(&not_for_me_fragment));
        match recv_3.recv() {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }
    }
}
