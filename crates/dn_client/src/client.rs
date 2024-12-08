use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::{ClientCommand, ClientEvent};
use std::collections::HashMap;

use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{Ack, FloodRequest, FloodResponse, Fragment, NodeType, PacketType};
use wg_2024::{network::NodeId, packet::Packet};

pub struct Client {
    pub id: NodeId,
    pub controller_send: Sender<ClientEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
}

impl Client {
    pub fn new(
        id: NodeId,
        controller_send: Sender<ClientEvent>,
        controller_recv: Receiver<ClientCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
    ) -> Self {
        Self {
            id,
            controller_send,
            controller_recv,
            packet_send,
            packet_recv,
        }
    }

    pub fn run(&mut self) {
        let mut session_id = 0;
        loop {
            select! {
                recv(self.controller_recv) -> command => {
                    if let Ok(cmd) = command {
                        self.handle_command(cmd, session_id);
                        session_id += 1;
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

    fn handle_command(&mut self, command: ClientCommand, session_id: u64) {
        match command {
            ClientCommand::SendFragment => self.send_fragment(session_id),
            ClientCommand::SendFloodRequest => self.send_flood_request(session_id),
            ClientCommand::RemoveSender(n) => self.remove_sender(n),
            ClientCommand::AddSender(n, sender) => self.add_sender(n, sender),
            ClientCommand::SendAck => self.send_ack(session_id),
            _ => {}
        }
    }

    fn send_fragment(&self, session_id: u64) {
        let routing_header = if self.id == 1 {
            SourceRoutingHeader {
                hop_index: 1,
                hops: vec![1, 6, 8, 3],
            }
        } else if self.id == 2 {
            SourceRoutingHeader {
                hop_index: 1,
                hops: vec![2, 6, 9, 4],
            }
        } else {
            panic!("error in topology sending fragment");
        };

        let packet = Packet {
            routing_header,
            session_id,
            pack_type: PacketType::MsgFragment(Fragment {
                fragment_index: 0,
                total_n_fragments: 1,
                length: 0,
                data: [0; 128],
            }),
        };
        self.packet_send
            .get(&6)
            .unwrap()
            .send(packet.clone())
            .expect("Error in send");
        self.controller_send
            .send(ClientEvent::PacketSent(packet))
            .expect("Error in controller_send");
    }

    fn send_ack(&self, session_id: u64) {
        let routing_header = if self.id == 1 {
            SourceRoutingHeader {
                hop_index: 1,
                hops: vec![1, 6, 8, 3],
            }
        } else if self.id == 2 {
            SourceRoutingHeader {
                hop_index: 1,
                hops: vec![2, 6, 7, 4],
            }
        } else {
            panic!("error in topology sending fragment");
        };

        let packet = Packet {
            routing_header,
            session_id,
            pack_type: PacketType::Ack(Ack {
                fragment_index: 0,
            }),
        };
        self.packet_send
            .get(&6)
            .unwrap()
            .send(packet.clone())
            .expect("Error in send");
        self.controller_send
            .send(ClientEvent::PacketSent(packet))
            .expect("Error in controller_send");
    }

    fn send_flood_request(&self, session_id: u64) {
        let flood_request_packet = Packet {
            pack_type: PacketType::FloodRequest(FloodRequest {
                flood_id: session_id,
                initiator_id: self.id,
                path_trace: vec![(self.id, NodeType::Client)],
            }),
            routing_header: SourceRoutingHeader {
                hop_index: 0,
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
            .send(ClientEvent::PacketSent(flood_request_packet))
            .expect("Error in controller_send");
    }

    fn remove_sender(&mut self, n: NodeId) {
        self.packet_send.remove(&n);
    }

    fn add_sender(&mut self, n: NodeId, sender: Sender<Packet>) {
        self.packet_send.insert(n, sender);
    }

    fn handle_packet(&self, packet: Packet) {
        self.controller_send
            .send(ClientEvent::PacketReceived(packet.clone(), self.id))
            .expect("Error in controller_send");

        match packet.pack_type {
            PacketType::MsgFragment(_) => {
                println!("Client#{} received fragment", self.id);
            }
            PacketType::Ack(ack) => {
                println!("Client#{} received ack: {:?}", self.id, ack);
            }
            PacketType::Nack(nack) => {
                println!("Client#{} received nack: {:?}", self.id, nack);
            }
            PacketType::FloodRequest(flood_request) => {
                println!("Client#{} received flood request", self.id);
                self.send_flood_response(packet.session_id, flood_request);
            }
            PacketType::FloodResponse(flood_response) => {
                println!(
                    "Client#{} received flood response: {:?}",
                    self.id, flood_response
                );
            }
        }
    }

    fn send_flood_response(&self, session_id: u64, mut flood_request: FloodRequest) {
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
            session_id,
        };

        self.packet_send[&flood_response_packet.routing_header.hops[1]]
            .send(flood_response_packet.clone())
            .expect("Error in send");
        self.controller_send
            .send(ClientEvent::PacketSent(flood_response_packet))
            .expect("Error in controller_send");
    }
}
