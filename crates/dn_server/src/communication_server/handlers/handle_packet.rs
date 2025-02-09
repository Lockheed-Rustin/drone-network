//! This module is responsible for processing incoming packets. It evaluates the type of each packet
//! and delegates the appropriate action to handle message fragments, acknowledgments, negative
//! acknowledgments, and flood-related requests or responses. The simulation controller is notified
//! about each received packet.

use crate::communication_server::communication_server::CommunicationServer;
use dn_controller::ServerEvent;
use wg_2024::packet::{Nack, NackType, Packet, PacketType};

impl CommunicationServer {
    /// Processes an incoming packet and performs the corresponding action.
    ///
    /// This function handles packets received by the server, determining the type of packet
    /// and delegating the appropriate action based on its content. The actions may involve
    /// processing message fragments, handling acknowledgments or negative acknowledgments,
    /// or responding to flood requests and responses.
    ///
    /// Before processing, this function notifies the simulation controller that a packet has been received.
    /// It also checks that the server is the actual recipient of the packet.
    ///
    /// # Arguments
    /// * `packet` - The packet to be processed. It can be of various types including:
    ///   - `MsgFragment` for message fragments.
    ///   - `Nack` for negative acknowledgments.
    ///   - `Ack` for acknowledgments.
    ///   - `FloodRequest` for flood requests.
    ///   - `FloodResponse` for flood responses.
    pub(crate) fn handle_packet(&mut self, packet: Packet) {
        self.controller_send
            .send(ServerEvent::PacketReceived(packet.clone(), self.id))
            .expect("Error in controller_send");

        if let PacketType::FloodRequest(f_req) = packet.pack_type {
            self.send_flood_response(f_req);
            return;
        }

        if !self.check_routing(&packet, packet.pack_type.clone()) {
            return;
        }

        let sender_id = packet.routing_header.hops[0];
        match packet.pack_type {
            PacketType::MsgFragment(f) => {
                self.handle_fragment(
                    &f,
                    sender_id,
                    packet.session_id,
                    &packet.routing_header.hops,
                );
            }
            PacketType::Nack(nack) => {
                self.handle_nack(&nack, packet.session_id, &packet.routing_header);
            }
            PacketType::Ack(ack) => {
                // update the estimated pdr for the last path used to go to the ack sender
                for n in self
                    .network_topology
                    .get_saved_path(sender_id)
                    .iter()
                    .skip(1)
                {
                    self.network_topology.update_estimated_pdr(*n, false);
                }
                self.session_manager.handle_ack(&ack, packet.session_id);
            }
            PacketType::FloodResponse(f_res) => self.handle_flood_response(&f_res),
            PacketType::FloodRequest(_) => {}
        }
    }

    /// Checks if the routing information in the packet is correct for this server.
    ///
    /// The function compares the current hop index in the routing header with the server's ID.
    /// - If they match, it checks if the server is the meant recipient.
    /// - If they don't match, the function return false because the packet was not for the server to
    ///   process. In this case, it also sends an Unexpected Recipient nack to the sender.
    ///
    /// # Arguments
    /// * `packet` - The packet whose routing information is to be checked.
    /// * `packet_type` - The packet type field of the packet
    ///
    /// # Returns
    /// * `true` if the packet is intended for this server, otherwise `false`.
    fn check_routing(&mut self, packet: &Packet, packet_type: PacketType) -> bool {
        if packet.routing_header.hops[packet.routing_header.hop_index] == self.id {
            // False if the packet is for me, but I don't have to process it because I'm not the recipient
            packet.routing_header.hops.last() == Some(&self.id)
        } else {
            if let PacketType::MsgFragment(f) = packet_type {
                // the packet is not for me. Unexpected recipient nack is sent.
                let mut hops = packet
                    .routing_header
                    .hops
                    .iter()
                    .copied()
                    .take(packet.routing_header.hop_index + 1)
                    .rev()
                    .collect::<Vec<_>>();
                hops[0] = self.id;

                let nack = Nack {
                    fragment_index: f.fragment_index,
                    nack_type: NackType::UnexpectedRecipient(self.id),
                };
                self.send_nack(nack, hops);
            }

            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server::test_server_helper::TestServerHelper;
    use dn_message::Message;
    use dn_message::ServerBody::ErrUnsupportedRequestType;
    use wg_2024::network::SourceRoutingHeader;
    use wg_2024::packet::{Ack, Fragment};

    #[test]
    fn test_update_pdr_when_receiving_ack() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;
        server.send_message(Message::Server(ErrUnsupportedRequestType), 6);
        server.network_topology.update_estimated_pdr(3, true); // pdr-3 = 40

        assert_eq!(server.network_topology.get_node_cost(3).unwrap(), 40);

        let (packet, _session_id) = TestServerHelper::test_received_packet(
            PacketType::Ack(Ack { fragment_index: 0 }),
            vec![6, 3, 1],
        );
        server.handle_packet(packet);

        // pdr-3 should be = 24
        assert_eq!(server.network_topology.get_node_cost(3).unwrap(), 24);
    }

    #[test]
    fn test_check_routing() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;
        // Test false because not being last hop
        let packet = Packet {
            routing_header: SourceRoutingHeader {
                hop_index: 2,
                hops: vec![6, 3, 1, 8, 9, 10],
            },
            session_id: 0,
            pack_type: PacketType::Ack(Ack { fragment_index: 0 }),
        };
        assert_eq!(
            server.check_routing(&packet, packet.pack_type.clone()),
            false
        );
        let packet = Packet {
            routing_header: SourceRoutingHeader {
                hop_index: 2,
                hops: vec![6, 3, 7],
            },
            session_id: 0,
            pack_type: PacketType::MsgFragment(Fragment {
                fragment_index: 111,
                total_n_fragments: 1,
                length: 0,
                data: [0; 128],
            }),
        };
        assert_eq!(
            server.check_routing(&packet, packet.pack_type.clone()),
            false
        );
        // Test false with unexpected recipient
        let nack = helper.packet_recv_3.try_recv().unwrap();
        if let PacketType::Nack(nack) = nack.pack_type {
            assert_eq!(nack.fragment_index, 111);
            if let NackType::UnexpectedRecipient(nack_id) = nack.nack_type {
                assert_eq!(nack_id, 1);
            } else {
                assert!(false);
            }
        }
        // Test true
        let packet = Packet {
            routing_header: SourceRoutingHeader {
                hop_index: 2,
                hops: vec![6, 3, 1],
            },
            session_id: 0,
            pack_type: PacketType::Ack(Ack { fragment_index: 0 }),
        };
        assert_eq!(
            server.check_routing(&packet, packet.pack_type.clone()),
            true
        );
    }
}
