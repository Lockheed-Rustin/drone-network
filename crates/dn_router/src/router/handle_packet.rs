use super::Router;
use crate::command::Event;
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{Ack, Nack, NackType, Packet, PacketType};

impl Router {
    pub(crate) fn handle_packet(&mut self, packet: Packet) {
        self.controller_send
            .send(Event::PacketReceived(packet.clone(), self.id))
            .unwrap();
        match packet.pack_type {
            PacketType::MsgFragment(_) => self.handle_fragment(packet),
            PacketType::Ack(ref ack) => {
                let ack = ack.clone();
                self.handle_ack(packet, ack);
            }
            PacketType::Nack(ref nack) => {
                let nack = nack.clone();
                self.handle_nack(packet, nack)
            }
            PacketType::FloodRequest(flood_request) => self.handle_flood_request(flood_request),
            PacketType::FloodResponse(flood_response) => self.handle_flood_response(flood_response),
        }
    }

    pub(crate) fn handle_fragment(&mut self, packet: Packet) {
        if let PacketType::MsgFragment(ref fragment) = packet.pack_type {
            let sender_id = packet.routing_header.hops.last().cloned().unwrap();
            let ack = Packet {
                routing_header: SourceRoutingHeader {
                    hop_index: 0,
                    hops: packet.routing_header.hops.iter().cloned().rev().collect(),
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

    pub(crate) fn handle_ack(&mut self, packet: Packet, ack: Ack) {
        self.routing.ack(packet, ack.fragment_index);
    }

    pub(crate) fn handle_nack(&mut self, packet: Packet, nack: Nack) {
        match nack.nack_type {
            NackType::ErrorInRouting(err_id) => {
                self.routing.crash_node(err_id);
                self.routing.nack(packet.session_id, nack.fragment_index);
            }
            NackType::Dropped => {
                let drop_id = packet.routing_header.hops[0];
                self.routing.update_estimated_pdr(drop_id, true);
                if self.should_flood() {
                    self.flood();
                }
                self.routing.nack(packet.session_id, nack.fragment_index);
            }
            _ => {
                unreachable!()
            }
        }
    }
}
