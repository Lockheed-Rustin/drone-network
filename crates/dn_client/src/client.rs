use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::ClientCommand;
use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

use wg_2024::{network::NodeId, packet::Packet};
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, PacketType};

pub struct Client {
    // TODO: create ClientEvent
    // pub controller_send: Sender<NodeEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
}

impl Client {
    pub fn run(&mut self) {
        let flood_request_packet = Packet {
            pack_type: PacketType::FloodRequest(FloodRequest {
                flood_id: 0,
                initiator_id: 4,
                path_trace: vec![(4, NodeType::Client)],
            }),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![],
            },
            session_id: 0,
        };

        for (_, sender) in self.packet_send.iter() {
            sender.send(flood_request_packet.clone()).expect("Error in send");
        }

        loop {
            if let Ok(packet) = self.packet_recv.recv() {
                self.handle_packet(packet);
            } else {
                break;
            }
        }


        /*
        sleep(Duration::from_secs(1));

        let fragment = Fragment {
            fragment_index: 0,
            total_n_fragments: 0,
            length: 0,
            data: [0; 128],
        };
        let packet1 = Packet {
            pack_type: PacketType::MsgFragment(fragment),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![4, 2, 1, 5]
            },
            session_id: 0,
        };

        let ack = Ack {
            fragment_index: 0,
        };
        let packet2 = Packet {
            pack_type: PacketType::Ack(ack),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![4, 2, 1, 5]
            },
            session_id: 0,
        };

        let nack = Nack {
            fragment_index: 0,
            nack_type: NackType::Dropped,
        };
        let packet3 = Packet {
            pack_type: PacketType::Nack(nack),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![4, 2, 1, 5]
            },
            session_id: 0,
        };


        let sender = self.packet_send.get(&2).unwrap();

        loop {
            sender.send(packet1.clone()).expect("Error in send");
            sleep(Duration::from_secs(1));
            sender.send(packet2.clone()).expect("Error in send");
            sleep(Duration::from_secs(1));
            sender.send(packet3.clone()).expect("Error in send");
            sleep(Duration::from_secs(1));
        }
        */
    }

    fn handle_packet(&mut self, packet: Packet) {
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
