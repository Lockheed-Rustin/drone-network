use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::ClientCommand;
use std::collections::HashMap;
use std::thread::sleep;
use std::time::{Duration, Instant};

use wg_2024::{network::NodeId, packet::Packet};
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, PacketType};

pub struct Client {
    pub id: NodeId,
    // TODO: create ClientEvent
    // pub controller_send: Sender<NodeEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
}

impl Client {
    pub fn new(
        id: NodeId,
        // pub controller_send: Sender<NodeEvent>,
        controller_recv: Receiver<ClientCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
    )-> Self {
        Self {
            id,
            // pub controller_send: Sender<NodeEvent>,
            controller_recv,
            packet_send,
            packet_recv,
        }
    }

    pub fn run(&self) {
        loop {
            let mut session_id = 0;
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
    
    fn handle_command(&self, command: ClientCommand, session_id: u64) {
        match command {
            ClientCommand::SendFragment => {
                self.send_fragment(session_id);
            }
            ClientCommand::SendFloodRequest => {
                self.send_flood_request(session_id);
            }
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
                hops: vec![2, 6, 7, 4],
            }
        } else {
            panic!("error in topology sending fragment");
        };

        let packet = Packet {
            routing_header,
            session_id,
            pack_type: PacketType::MsgFragment(
                Fragment {
                    fragment_index: 0,
                    total_n_fragments: 1,
                    length: 0,
                    data: [0; 128],
                }
            ),
        };
        self.packet_send.get(&6).unwrap().send(packet.clone()).expect("Error in send");
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
            sender.send(flood_request_packet.clone()).expect("Error in send");
        }
    }

    fn handle_packet(&self, packet: Packet) {
        match packet.pack_type {
            PacketType::MsgFragment(_) => {
                println!("Client received fragment");
            }
            PacketType::Ack(ack) => {
                println!("Client received ack\n{:?}", ack);
            }
            PacketType::Nack(_) => {
                println!("Client received nack");

            }
            PacketType::FloodRequest(_) => {
                println!("Client received flood request");
            }
            PacketType::FloodResponse(flood_response) => {
                println!("Client received flood response:\n{:?}", flood_response);
            }
        }
    }
}
