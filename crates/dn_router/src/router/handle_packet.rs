use super::Router;
use crate::command::Event;
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Ack, Nack, NackType, Packet, PacketType};

impl Router {
    pub(crate) fn handle_packet(&mut self, packet: Packet) {
        self.controller_send
            .send(Event::PacketReceived(packet.clone(), self.id))
            .unwrap();
        match packet.pack_type {
            PacketType::MsgFragment(_) => self.handle_fragment(&packet),
            PacketType::Ack(ref ack) => {
                let fragment_index = ack.fragment_index;
                self.handle_ack(packet, fragment_index);
            }
            PacketType::Nack(ref nack) => {
                let session_id = packet.session_id;
                let drop_id = packet.routing_header.hops[0];
                self.handle_nack(session_id, drop_id, nack);
            }
            PacketType::FloodRequest(flood_request) => self.handle_flood_request(flood_request),
            PacketType::FloodResponse(ref flood_response) => {
                self.handle_flood_response(flood_response);
            }
        }
    }

    pub(crate) fn handle_fragment(&mut self, packet: &Packet) {
        if let PacketType::MsgFragment(ref fragment) = packet.pack_type {
            let sender_id = packet.routing_header.hops.last().copied().unwrap();
            let ack = Packet {
                routing_header: SourceRoutingHeader {
                    hop_index: 0,
                    hops: packet.routing_header.hops.iter().copied().rev().collect(),
                },
                session_id: packet.session_id,
                pack_type: PacketType::Ack(Ack {
                    fragment_index: fragment.fragment_index,
                }),
            };
            self.routing.send_packet(ack);
            if let Some(message) =
                self.assembler
                    .handle_fragment(fragment, sender_id, packet.session_id)
            {
                self.controller_send
                    .send(Event::MessageAssembled {
                        body: message,
                        from: packet.routing_header.hops[0],
                        to: self.id,
                    })
                    .unwrap();
            }
        }
    }

    pub(crate) fn handle_ack(&mut self, packet: Packet, fragment_index: u64) {
        self.routing.ack(packet, fragment_index);
    }

    pub(crate) fn handle_nack(&mut self, session_id: u64, drop_id: NodeId, nack: &Nack) {
        match nack.nack_type {
            NackType::ErrorInRouting(err_id) => {
                self.routing.crash_node(err_id);
                self.routing.nack(session_id, nack.fragment_index);
            }
            NackType::Dropped => {
                self.routing.update_estimated_pdr(drop_id, true);
                if self.should_flood() {
                    self.flood();
                }
                self.routing.nack(session_id, nack.fragment_index);
            }
            _ => {
                unreachable!()
            }
        }
    }
}
