use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::{ClientCommand, ClientEvent};
use dn_message::{Assembler, ClientBody, Message};
use std::collections::HashMap;
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{FloodRequest, FloodResponse, Fragment, NodeType, PacketType};
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
                        self.handle_command(cmd);
                        self.session_id += 1;
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
            _ => {}
        }
    }

    fn handle_packet(&mut self, packet: Packet) {
        //notify controller about receiving packet
        self.controller_send
            .send(ClientEvent::PacketReceived(packet.clone(), self.id))
            .expect("Error in controller_send");

        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                self.handle_fragment(fragment, packet.routing_header.source().unwrap(), packet.session_id);
            }

            PacketType::Ack(ack) => {
                println!("Client#{} received ack: {:?}", self.id, &ack);
            }

            PacketType::Nack(nack) => {
                println!("Client#{} received nack: {:?}", self.id, &nack);

            }

            PacketType::FloodRequest(flood_request) => {
                self.handle_flood_request(packet.session_id, flood_request);
            }

            PacketType::FloodResponse(flood_response) => {

                self.handle_flood_response(flood_response);
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
            for (server, path) in servers_became_reachable {
                if let Some(unsendeds) = self.unsendable_fragments.remove(&server) {
                    let header = SourceRoutingHeader::with_first_hop(path);

                    for (session, fragment) in unsendeds {
                        let packet= Packet::new_fragment(header.clone(), session, fragment);

                        self.packet_send
                            .get(&6)
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


    //---------- send ----------//
    fn send_message(&mut self, client_body: ClientBody, dest: NodeId) {
        let fragments = self.assembler.serialize_message(&Message::Client(client_body));

        let pending_fragment = self.pending_sessions.entry((dest, self.session_id)).or_default();
        for fragment in fragments.iter() {
                pending_fragment.insert(fragment.fragment_index, fragment.clone());
        }

        if let Some(path) = self.source_routing.get_path(dest) {
            let header = SourceRoutingHeader::with_first_hop(path);

            for fragment in fragments {
                let packet= Packet::new_fragment(header.clone(), self.session_id, fragment);

                self.packet_send
                    .get(&6)
                    .unwrap()
                    .send(packet.clone())
                    .expect("Error in send");

                self.controller_send
                    .send(ClientEvent::PacketSent(packet))
                    .expect("Error in controller_send");
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
        let flood_request_packet = Packet {
            pack_type: PacketType::FloodRequest(
                FloodRequest::initialize(self.session_id, self.id, NodeType::Client)
            ),
            routing_header: SourceRoutingHeader {
                hop_index: 0,
                hops: vec![],
            },
            session_id: self.session_id,
        };

        self.source_routing.reset_topology();

        for (_, sender) in self.packet_send.iter() {
            sender
                .send(flood_request_packet.clone())
                .expect("Error in send");
        }

        self.controller_send
            .send(ClientEvent::PacketSent(flood_request_packet))
            .expect("Error in controller_send");
    }




    //---------- handle for PacketTypes ----------//

    fn handle_fragment(&mut self, fragment: Fragment, sender_id: NodeId, session_id: u64) {
        if let Some(Message::Server(server_body)) = self.assembler.handle_fragment(&fragment, sender_id, session_id) {
            self.controller_send
                .send(ClientEvent::MessageAssembled(server_body))
                .expect("Error in controller_send");
        }

        //TODO: complete
        unimplemented!()
    }
    fn handle_flood_response(&mut self, mut flood_response: FloodResponse) {
        unimplemented!()
    }



    //---------- send Packets ----------//
    fn send_nack(&self) {
        unimplemented!()
    }


    fn handle_flood_request(&self, session_id: u64, mut flood_request: FloodRequest) {
        flood_request.increment(self.id, NodeType::Server);
        let flood_response = flood_request.generate_response(session_id);

        //send flood response
        self.packet_send[&flood_response.routing_header.hops[1]]
            .send(flood_response.clone())
            .expect("Error in send");

        //notify controller about sending packet
        self.controller_send
            .send(ClientEvent::PacketSent(flood_response))
            .expect("Error in controller_send");


    }
}
