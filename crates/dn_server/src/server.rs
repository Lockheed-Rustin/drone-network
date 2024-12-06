use crossbeam_channel::{Receiver, Sender};
use dn_controller::ServerCommand;
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
                println!("Server#{} received fragment", self.id);
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
                println!("Server#{} received ack", self.id);
            }
            PacketType::Nack(_) => {
                println!("Server#{} received nack", self.id);

            }
            PacketType::FloodRequest(mut flood_request) => {
                println!("Server#{} received flood request", self.id);
                self.send_flood_response(packet.session_id, flood_request);
            }
            PacketType::FloodResponse(_) => {
                println!("Server#{} received fragment", self.id);
            }
        }
    }

    fn send_flood_response(&self, session_id: u64, mut flood_request: FloodRequest) {
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
            session_id,
        };

        self.packet_send[&flood_response_packet.routing_header.hops[1]].send(flood_response_packet).expect("Error in send");
    }
}
