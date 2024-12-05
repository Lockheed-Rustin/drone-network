use crossbeam_channel::{Receiver, Sender};
use dn_controller::{ClientCommand, ServerCommand};
use std::collections::HashMap;
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Packet, PacketType, Ack, FloodResponse, NodeType, FloodRequest};

pub struct Server {
    pub id: NodeId,
    // TODO: create ServerEvent (2 different enums for the 2 server types?)
    // pub controller_send: Sender<NodeEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
}


impl Server {
    pub fn new(
        id: NodeId,
        // pub controller_send: Sender<NodeEvent>,
        controller_recv: Receiver<ServerCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
    ) -> Self {
        Self {
            id,
            // pub controller_send: Sender<NodeEvent>,
            controller_recv,
            packet_send,
            packet_recv,
        }
    }

    pub fn run(&mut self) {
        let flood_request_packet = Packet {
            pack_type: PacketType::FloodRequest(FloodRequest {
                flood_id: 0,
                initiator_id: self.id,
                path_trace: vec![(self.id, NodeType::Server)],
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

        loop {
            if let Ok(packet) = self.packet_recv.recv() {
                self.handle_packet(packet);
            } else {
                break;
            }
        }
    }

    fn handle_packet(&mut self, packet: Packet) {
        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                println!("Server received fragment");
                let hops = packet.routing_header.hops.iter().cloned().rev().collect::<Vec<_>>();

                let ack_packet = Packet {
                    pack_type: PacketType::Ack(Ack {
                        fragment_index: fragment.fragment_index,
                    }),
                    routing_header: SourceRoutingHeader {
                        hop_index: 1,
                        hops,
                    },
                    session_id: packet.session_id,
                };

                self.packet_send[&ack_packet.routing_header.hops[1]].send(ack_packet).expect("Error in send");
            }
            PacketType::Ack(_) => {
                println!("Server received ack");
            }
            PacketType::Nack(_) => {
                println!("Server received nack");

            }
            PacketType::FloodRequest(mut flood_request) => {
                println!("Server received flood request");
                flood_request.path_trace.push((5, NodeType::Server));
                let hops = flood_request.path_trace.iter()
                    .map(|(node_id, _)| *node_id)
                    .rev()
                    .collect();

                let flood_response_packet = Packet {
                    pack_type: PacketType::FloodResponse(FloodResponse {
                        flood_id: flood_request.flood_id,
                        path_trace: flood_request.path_trace,
                    }),
                    routing_header: SourceRoutingHeader {
                        hop_index: 1,
                        hops,
                    },
                    session_id: packet.session_id,
                };

                self.packet_send[&flood_response_packet.routing_header.hops[1]].send(flood_response_packet).expect("Error in send");
            }
            PacketType::FloodResponse(_) => {
                println!("Server received fragment");
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use wg_2024::packet::{FloodRequest, Fragment};
    use super::*;

    #[test]
    fn test_handle_fragment() {

        let fragment = Fragment {
            fragment_index: 0,
            total_n_fragments: 0,
            length: 0,
            data: [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],
        };
        let packet = Packet {
            pack_type: PacketType::MsgFragment(fragment.clone()),
            routing_header: SourceRoutingHeader {
                hop_index: 3,
                hops: vec![4,2,1,5],
            },
            session_id: 0,
        };

        let ack_packet = {
            let hops = packet.routing_header.hops.iter().cloned().rev().collect::<Vec<_>>();

            let ack_packet = Packet {
                pack_type: PacketType::Ack(Ack {
                    fragment_index: fragment.fragment_index,
                }),
                routing_header: SourceRoutingHeader {
                    hop_index: 1,
                    hops,
                },
                session_id: packet.session_id,
            };
            ack_packet
        };

        println!("ack_packet hops: {:?}", ack_packet.routing_header.hops);
        assert_eq!(vec![5,1,2,4], ack_packet.routing_header.hops); // This will pass if result == 5
    }

    #[test]
    fn test_handle_flood_request() {

        let mut flood_request = FloodRequest {
            flood_id: 0,
            initiator_id: 4,
            path_trace: vec![(4, NodeType::Client), (2, NodeType::Drone), (1, NodeType::Drone)],
        };
        let packet = Packet {
            pack_type: PacketType::FloodRequest(flood_request.clone()),
            routing_header: SourceRoutingHeader {
                hop_index: 0,
                hops: vec![],
            },
            session_id: 0,
        };

        let flood_response_packet = {
            flood_request.path_trace.push((5, NodeType::Server));
            let hops = flood_request.path_trace.iter()
                .map(|(node_id, _)| *node_id)
                .rev()
                .collect();

            let flood_response_packet = Packet {
                pack_type: PacketType::FloodResponse(FloodResponse {
                    flood_id: flood_request.flood_id,
                    path_trace: flood_request.path_trace,
                }),
                routing_header: SourceRoutingHeader {
                    hop_index: 1,
                    hops,
                },
                session_id: packet.session_id,
            };

            flood_response_packet
        };

        println!("flood_response_packet hops: {:?}", flood_response_packet.routing_header.hops);
        assert_eq!(vec![5,1,2,4], flood_response_packet.routing_header.hops); // This will fail because 2 + 3 != 6
    }
}
