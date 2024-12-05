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
        //send flood request
        let flood_request_packet = Packet {
            pack_type: PacketType::FloodRequest(FloodRequest {
                flood_id: 0,
                initiator_id: self.id,
                path_trace: vec![(self.id, NodeType::Client)],
            }),
            routing_header: SourceRoutingHeader {
                hop_index: 0,
                hops: vec![],
            },
            session_id: 0,
        };

        for (_, sender) in self.packet_send.iter() {
            sender.send(flood_request_packet.clone()).expect("Error in send");
        }

        //send other packets
        let sender = self.packet_send.get(&2).unwrap();
        let mut fragment_index = 0;
        let mut start_time = Instant::now();

        loop {
            if start_time.elapsed() >= Duration::from_secs(1) {
                sender.send(Packet {
                    pack_type: PacketType::MsgFragment(Fragment {
                        fragment_index,
                        total_n_fragments: 0,
                        length: 0,
                        data: [0; 128],
                    }),
                    routing_header: SourceRoutingHeader {
                        hop_index: 1,
                        hops: vec![self.id, 2, 1, 5]
                    },
                    session_id: 0,
                }).expect("Error in send");

                fragment_index += 1;
                start_time = Instant::now();
            }
            if let Ok(packet) = self.packet_recv.try_recv() {
                self.handle_packet(packet);
            }
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
