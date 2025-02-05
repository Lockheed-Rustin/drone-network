use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::{ClientCommand, ClientEvent};
use dn_message::{Assembler, ClientBody, Message};
use std::collections::HashMap;
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, PacketType};
use wg_2024::{network::NodeId, packet::Packet};
use crate::ClientRouting;



//---------- CUSTOM TYPES ----------//
type PendingFragments = HashMap<u64, Fragment>;

pub struct Client {
    pub id: NodeId,
    pub controller_send: Sender<ClientEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,

    pub session_id: u64,
    pub assembler: Assembler,
    pub source_routing: ClientRouting,

    //NOTE: assembler manages incoming messages and rebuild them;
    //      following structures manage sent messages
    pending_sessions: HashMap<(NodeId, u64), PendingFragments>, // (dest, session_id) -> (fragment_index -> fragment)
    unsendable_fragments: HashMap<NodeId, Vec<(u64, Fragment)>>, // dest -> Vec<(session_id, fragment)>
}

impl Client {
    pub fn new(
        id: NodeId,
        controller_send: Sender<ClientEvent>,
        controller_recv: Receiver<ClientCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
    ) -> Self{
        Self {
            id,
            controller_send,
            controller_recv,
            packet_send,
            packet_recv,
            session_id: 0,
            assembler: Assembler::new(),
            source_routing: ClientRouting::new(id),
            pending_sessions: HashMap::new(),
            unsendable_fragments: HashMap::new(),
        }
    }

    pub fn run(&mut self) {
        loop {
            select! {
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
            ClientCommand::SendMessage(client_body, to) => self.send_message(client_body, to),
            ClientCommand::SendFloodRequest => self.send_flood_request(),
            ClientCommand::RemoveSender(n) => self.remove_sender(n),
            ClientCommand::AddSender(n, sender) => self.add_sender(n, sender),
            _ => {},
        }
    }

    fn handle_packet(&mut self, packet: Packet) {
        //notify controller about receiving packet
        self.controller_send
            .send(ClientEvent::PacketReceived(packet.clone(), self.id))
            .expect("Error in controller_send");

        //TODO: gestire caso in cui il messaggio debba solo transitare attraverso me???

        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                self.handle_fragment(&fragment, &packet.routing_header, packet.session_id);
            }

            PacketType::Ack(ack) => {
                self.handle_ack(&ack, &packet.routing_header, packet.session_id);
            }

            PacketType::Nack(nack) => {
                println!("Client#{} received nack: {:?}", self.id, &nack);

            }

            PacketType::FloodRequest(flood_request) => {
                self.handle_flood_request(packet.session_id, flood_request);
            }

            PacketType::FloodResponse(flood_response) => {

                self.handle_flood_response(&flood_response);
            }
        }
    }


    //---------- add/rmv sender from client ----------//
    fn remove_sender(&mut self, n: NodeId) {
        self.packet_send.remove(&n);
        self.source_routing.remove_channel_to_neighbor(n);
    }

    fn add_sender(&mut self, n: NodeId, sender: Sender<Packet>) {
        if self.packet_send.len() > 2 {
            return; //can't link more than 2 drone to client
        }

        self.packet_send.insert(n, sender);

        if let Some(servers_became_reachable) = self.source_routing.add_channel_to_neighbor(n) {
            self.send_unsended(servers_became_reachable);
        }
    }


    //---------- send ----------//
    fn send_unsended(&mut self, servers: Vec<(NodeId, Vec<NodeId>)>) {
        for (server, path) in servers {
            if let Some(unsendeds) = self.unsendable_fragments.remove(&server) {
                let header = SourceRoutingHeader::with_first_hop(path);
                if let Some(next_hop) = header.current_hop() {
                    for (session, fragment) in unsendeds {
                        let packet = Packet::new_fragment(header.clone(), session, fragment);

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
            }
        }
    }

    fn send_message(&mut self, client_body: ClientBody, dest: NodeId) {
        //fragment message and notify controller
        let fragments = self.assembler.serialize_message(&Message::Client(client_body.clone()));

        self.controller_send
            .send(ClientEvent::MessageFragmented{
                body: client_body,
                from: self.id,
                to: dest,
            })
            .expect("Error in controller_send");

        //add fragments to pending session
        let pending_fragment = self.pending_sessions.entry((dest, self.session_id)).or_default();
        for fragment in fragments.iter() {
                pending_fragment.insert(fragment.fragment_index, fragment.clone());
        }

        if let Some(path) = self.source_routing.get_path(dest) {
            let header = SourceRoutingHeader::with_first_hop(path);

            for fragment in fragments {
                let packet= Packet::new_fragment(header.clone(), self.session_id, fragment);
                self.send_packet(packet);
            }
        }
        else {
            let unsendeds = self.unsendable_fragments.entry(dest).or_default();
            for fragment in fragments {
                unsendeds.push((self.session_id, fragment));
            }

            self.send_flood_request();
        }

        self.session_id += 1;
    }

    fn send_flood_request(&mut self) {
        let flood_request_packet = Packet::new_flood_request(
            SourceRoutingHeader::empty_route(),
            self.session_id,
            FloodRequest::initialize(self.session_id, self.id, NodeType::Client)
        );
        self.session_id += 1;

        self.source_routing.clear_topology();

        for (_, sender) in self.packet_send.iter() {
            sender
                .send(flood_request_packet.clone())
                .expect("Error in send");
        }

        self.controller_send
            .send(ClientEvent::PacketSent(flood_request_packet))
            .expect("Error in controller_send");
    }

    fn send_packet(&self, packet: Packet) {
        if let Some(next_hop) = packet.routing_header.current_hop() {
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

    fn send_unexp_recip(&self, header: &SourceRoutingHeader, session_id: u64, fragment_index: u64) {
        let path = header
            .hops
            .iter()
            .cloned()
            .take(header.hop_index + 1)
            .rev()
            .collect::<Vec<_>>();

        let nack = Packet::new_nack(
            SourceRoutingHeader::with_first_hop(path),
            session_id,
            Nack{
                fragment_index,
                nack_type: NackType::UnexpectedRecipient(self.id),
            }
        );

        self.send_packet(nack);
    }




    //---------- handle ----------//

    fn handle_fragment(&mut self, fragment: &Fragment, header: &SourceRoutingHeader, session_id: u64) {
        if header.hops.len() < 2 {return;}

        if let Some(&sender) = header.hops.first() {
            self.source_routing.correct_exchanged_with(sender, &header.hops);
        }

        match header.hops.get(header.hop_index) {
            Some(&curr_hop) if curr_hop == self.id => { //if packet it's for me
                let &sender = header.hops.first().expect("Unreachable"); // always have first since path.len() >= 2

                if let Some(Message::Server(server_body)) = self.assembler.handle_fragment(fragment, sender, session_id) {
                    self.controller_send
                        .send(ClientEvent::MessageAssembled{
                            body: server_body,
                            from: sender,
                            to: self.id,
                        })
                        .expect("Error in controller_send");
                }

                //TODO: invio ack di risposta? -> path mio o reverso quello del paccheto? (Penso la seconda)
                let ack = Packet::new_ack(
                    SourceRoutingHeader::with_first_hop(
                        header.hops.iter().cloned().rev().collect::<Vec<_>>()
                    ),
                    session_id,
                    fragment.fragment_index,
                );

                self.send_packet(ack);
            }
            _ => { //If I received a wrong packet
                self.send_unexp_recip(header, session_id, fragment.fragment_index);
            }
        }


    }

    fn handle_flood_response(&mut self, flood_response: &FloodResponse) {
        if let Some(servers_became_reachable) = self.source_routing.add_path(&flood_response.path_trace) {
            self.send_unsended(servers_became_reachable);
        }
    }

    fn handle_ack(&mut self, ack: &Ack, header: &SourceRoutingHeader, session_id: u64) {
        if header.hops.len() < 2 {return;}

        let &server = header.hops.first().expect("Unreachable");

        self.source_routing.correct_send_to(server);

        if let Some(pending_fragment) = self.pending_sessions.get_mut(&(server, session_id)) {
            pending_fragment.remove(&ack.fragment_index);
            if pending_fragment.is_empty() {
                self.pending_sessions.remove(&(server, session_id));
            }
        }
    }

    fn handle_nack(&mut self, nack: &Nack, header: SourceRoutingHeader, session_id: u64) {
        unimplemented!()
    }


    fn handle_flood_request(&self, session_id: u64, mut flood_request: FloodRequest) {
        flood_request.increment(self.id, NodeType::Client);
        let mut flood_response = flood_request.generate_response(session_id);
        flood_response.routing_header.hop_index += 1;

        self.send_packet(flood_response);
    }
}
