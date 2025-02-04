use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::{ClientCommand, ClientEvent};
use dn_message::{Assembler, ClientBody, ClientCommunicationBody, CommunicationMessage, Message, ServerBody, ServerCommunicationBody, ServerType};
use std::collections::HashMap;
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{FloodRequest, FloodResponse, Fragment, NodeType, PacketType};
use wg_2024::{network::NodeId, packet::Packet};
use crate::ClientRouting;

//pending_sessions: HashMap<(NodeId, u64), PendingFragments>, // (NodeId, session_id) -> (fragment_index -> fragment)
//Types
//type PendingFragments = HashMap<u64, Fragment>;

pub struct Client {
    pub id: NodeId,
    pub controller_send: Sender<ClientEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,

    pub session_id: u64,
    pub assembler: Assembler,
    pub source_routing: ClientRouting,
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


    //---------- handle for SC's Commands ----------//

    fn handle_command(&mut self, command: ClientCommand) {
        match command {
            ClientCommand::SendMessage(client_body, to) => self.send_message(client_body, to),
            ClientCommand::SendFloodRequest => self.send_flood_request(),
            ClientCommand::RemoveSender(n) => self.remove_sender(n),
            ClientCommand::AddSender(n, sender) => self.add_sender(n, sender),
            _ => {}
        }
    }

    fn remove_sender(&mut self, n: NodeId) {
        self.packet_send.remove(&n);
    }

    fn add_sender(&mut self, n: NodeId, sender: Sender<Packet>) {
        self.packet_send.insert(n, sender);
    }


    //---------- handle packet ----------//

    fn handle_packet(&mut self, packet: Packet) {
        //notify controller about receiving packet
        self.controller_send
            .send(ClientEvent::PacketReceived(packet.clone(), self.id))
            .expect("Error in controller_send");

        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                println!("Client#{} received fragment: {:?}", self.id, &fragment);
                self.handle_fragment(&fragment, packet.routing_header.source().unwrap(), packet.session_id);

            }

            PacketType::Ack(ack) => {
                println!("Client#{} received ack: {:?}", self.id, &ack);
            }

            PacketType::Nack(nack) => {
                println!("Client#{} received nack: {:?}", self.id, &nack);

            }

            PacketType::FloodRequest(flood_request) => {
                println!(
                    "Client#{} received flood request from {}",
                    self.id, &flood_request.initiator_id
                );

                self.send_flood_response(packet.session_id, flood_request);
            }

            PacketType::FloodResponse(flood_response) => {
                println!(
                    "Client#{} received flood response: {:?}",
                    self.id, flood_response
                );

                self.handle_flood_response(flood_response);
            }
        }
    }



    //---------- handle for PacketTypes ----------//

    fn handle_fragment(&mut self, fragment: &Fragment, sender_id: NodeId, session_id: u64) {
        if let Some(Message::Server(server_body)) = self.assembler.handle_fragment(&fragment, sender_id, session_id) {
            self.controller_send
                .send(ClientEvent::MessageAssembled(server_body))
                .expect("Error in controller_send");
        }

        //TODO: complete
        let ack_path = match self.source_routing.get_path(sender_id) {
            Ok(_) => {}
            Err(_) => {}
        };

        /*
        let ack_fragment = Packet::new_ack(
          SourceRoutingHeader::with_first_hop()
        );
        */
    }
    fn handle_flood_response(&mut self, mut flood_response: FloodResponse) {
        unimplemented!()
    }



    //---------- send Packets ----------//
    fn send_message(&self, client_body: ClientBody, to: NodeId) {
        unimplemented!()
        /*
        let packet;
        self.packet_send
            .get(&6)
            .unwrap()
            .send(packet.clone())
            .expect("Error in send");
        self.controller_send
            .send(ClientEvent::PacketSent(packet))
            .expect("Error in controller_send");
         */
    }

    fn send_nack(&self) {
        unimplemented!()
    }

    fn send_flood_request(&self) {
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

        for (_, sender) in self.packet_send.iter() {
            sender
                .send(flood_request_packet.clone())
                .expect("Error in send");
        }

        self.controller_send
            .send(ClientEvent::PacketSent(flood_request_packet))
            .expect("Error in controller_send");
    }

    fn send_flood_response(&self, session_id: u64, mut flood_request: FloodRequest) {
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
