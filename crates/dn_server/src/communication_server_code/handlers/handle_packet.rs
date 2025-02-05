//! This module is responsible for processing incoming packets. It evaluates the type of each packet
//! and delegates the appropriate action to handle message fragments, acknowledgments, negative
//! acknowledgments, and flood-related requests or responses. The simulation controller is notified
//! about each received packet.

use crate::communication_server_code::communication_server::CommunicationServer;
use dn_controller::ServerEvent;
use wg_2024::packet::{Packet, PacketType};

impl CommunicationServer {
    /// Processes an incoming packet and performs the corresponding action.
    ///
    /// This function handles packets received by the server, determining the type of packet
    /// and delegating the appropriate action based on its content. The actions may involve
    /// processing message fragments, handling acknowledgments or negative acknowledgments,
    /// or responding to flood requests and responses.
    ///
    /// Before processing, this function notifies the simulation controller that a packet has been received.
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

        if !self.check_routing(&packet) {
            return;
        }

        let sender_id = packet.routing_header.hops[0];
        match packet.pack_type {
            PacketType::MsgFragment(f) => {
                self.handle_fragment(f, sender_id, packet.session_id, packet.routing_header.hops)
            }
            PacketType::Nack(nack) => {
                self.handle_nack(nack, packet.session_id, packet.routing_header)
            }
            PacketType::Ack(ack) => {
                // update the estimated pdr for the last path used to go to the ack sender
                for n in self
                    .network_topology
                    .get_saved_path(&sender_id)
                    .iter()
                    .skip(1)
                {
                    self.network_topology.update_estimated_pdr(*n, false);
                }
                self.session_manager.handle_ack(ack, &packet.session_id);
            }
            PacketType::FloodResponse(f_res) => self.handle_flood_response(f_res),
            _ => {}
        }
    }

    /// Checks if the routing information in the packet is correct for this server.
    ///
    /// The function compares the current hop index in the routing header with the server's ID.
    /// If they match, it means the packet is intended for this server to process.
    ///
    /// # Arguments
    /// * `packet` - The packet whose routing information is to be checked.
    ///
    /// # Returns
    /// * `true` if the packet is intended for this server, otherwise `false`.
    fn check_routing(&self, packet: &Packet) -> bool {
        packet.routing_header.hops[packet.routing_header.hop_index] == self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;
    use dn_message::Message;
    use dn_message::ServerBody::ErrUnsupportedRequestType;
    use wg_2024::packet::Ack;

    #[test]
    fn test_update_pdr_when_receiving_ack() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;
        server.send_message(Message::Server(ErrUnsupportedRequestType), 6);
        server.network_topology.update_estimated_pdr(3, true); // pdr-3 = 40

        assert_eq!(server.network_topology.get_node_cost(&3).unwrap(), 40);

        let (packet, _session_id) = TestServerHelper::test_received_packet(
            PacketType::Ack(Ack { fragment_index: 0 }),
            vec![6, 3, 1],
        );
        server.handle_packet(packet);

        // pdr-3 should be = 24
        assert_eq!(server.network_topology.get_node_cost(&3).unwrap(), 24);
    }
}
