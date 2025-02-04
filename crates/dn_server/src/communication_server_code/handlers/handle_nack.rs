//! This module handles the processing of negative acknowledgments (NACKs) received by the server.
//!
//! It is responsible for managing NACKs that indicate problems or errors in the transmission of
//! message fragments.
//! The server takes different actions based on the type of NACK received, such as correcting
//! routing issues, updating the network topology, or attempting to recover dropped fragments.
//!
//! ### Functions:
//!
//! - **`handle_nack`**: Processes an incoming NACK and performs the appropriate action based on its type.
//! - **`recover_fragment`**: Attempts to retrieve a missing or dropped message fragment, either by
//!                           retransmitting it or re-initiating the routing process.

use crate::communication_server_code::communication_server::CommunicationServer;
use crate::communication_server_code::session_manager::SessionId;
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{Nack, NackType, NodeType, Packet, PacketType};

impl CommunicationServer {
    /// This function processes an incoming NACK and takes the appropriate action based on its type.
    /// The action taken can vary from recovering dropped message fragments to fixing routing issues
    /// or updating the network topology.
    ///
    /// ### NACK Types:
    /// - **`Error in Routing`**: a drone tried to send a packet to another drone that was not among
    ///      its neighbors. That edge is removed from the topology.
    /// - **`Destination Is Drone`**: marks the destination node as a drone, changing its type in the
    ///      topology.
    /// - **`Dropped`**: indicates a dropped fragment, prompting the server to attempt recovery.
    /// - **`Unexpected Recipient`**: indicates the packet was delivered to the wrong recipient.
    ///      The topology is updated, and the fragment is retried.
    ///
    /// ### Arguments:
    /// - `nack`: The negative acknowledgment to process.
    /// - `session_id`: The session ID associated with the message fragment.
    /// - `source_routing_header`: The routing information that identifies the source node.
    pub(crate) fn handle_nack(
        &mut self,
        nack: Nack,
        session_id: SessionId,
        source_routing_header: SourceRoutingHeader,
    ) {
        match nack.nack_type {
            NackType::ErrorInRouting(error_node) => {
                self.network_topology
                    .remove_edge(source_routing_header.hops[0], error_node);
                self.update_network_topology();
                self.recover_fragment(session_id, nack.fragment_index);
            }
            NackType::DestinationIsDrone => {
                self.network_topology
                    .update_node_type(source_routing_header.hops[0], NodeType::Drone);
            }
            NackType::Dropped => {
                self.network_topology.update_estimated_pdr(source_routing_header.hops[0], true);
                self.network_topology.remove_path(&source_routing_header.hops[0]);
                self.recover_fragment(session_id, nack.fragment_index);
            }
            NackType::UnexpectedRecipient(_) => {
                self.update_network_topology();
                self.recover_fragment(session_id, nack.fragment_index);
            }
        }
    }

    /// Attempts to recover a dropped message fragment and retransmit it.
    ///
    /// This function checks if the requested fragment exists in the session manager and attempts to
    /// reassemble it. If the fragment is successfully recovered, it is transmitted to the correct
    /// recipient.
    ///
    /// ### Arguments:
    /// - `session_id`: The session ID associated with the fragment.
    /// - `fragment_index`: The index of the fragment to recover.
    ///
    /// # Panics
    /// This function panics if the specified fragment does not exist in the session manager.
    /// This indicates that an invalid session ID or fragment index was provided.
    fn recover_fragment(&mut self, session_id: SessionId, fragment_index: u64) {
        if let Some((fragment, dest)) = self
            .session_manager
            .recover_fragment(session_id, fragment_index)
        {
            let hops = self
                .network_topology
                .source_routing(self.id, dest)
                .expect("Error in routing");
            let packet = Packet {
                routing_header: SourceRoutingHeader { hop_index: 1, hops },
                session_id,
                pack_type: PacketType::MsgFragment(fragment),
            };
            self.send_packet(packet);
        } else {
            panic!(
                "tried to recover a fragment that is not in the session_manager.\n\
                    session_id: {}\n\
                    fragment_index: {}\n",
                session_id, fragment_index
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;
    #[test]
    fn test_handle_nack() {
        let mut test_server_helper = TestServerHelper::new();

        // ERROR IN ROUTING NACK
        let fragment_index = 23;
        let (packet, session_id) = TestServerHelper::test_received_packet(
            PacketType::Nack(Nack {
                fragment_index,
                nack_type: NackType::ErrorInRouting(3),
            }),
            vec![2, 1],
        );
        let pending_fragment = TestServerHelper::test_fragment(fragment_index, 100);
        test_server_helper.server.session_manager.add_session(
            session_id,
            vec![pending_fragment],
            6,
        );

        assert!(test_server_helper
            .server
            .network_topology
            .contains_edge(2, 3));
        test_server_helper.server.handle_packet(packet);
        assert!(!test_server_helper
            .server
            .network_topology
            .contains_edge(2, 3));

        let flood_req = test_server_helper
            .packet_recv_3
            .try_recv()
            .expect("Expected flood_req because of update topology");
        match flood_req.pack_type {
            PacketType::FloodRequest(_) => {}
            _ => panic!("Expected FloodRequest pack"),
        }
        let received_packet = test_server_helper
            .packet_recv_3
            .try_recv()
            .expect("No recover packet received on channel 3");
        assert_eq!(received_packet.session_id, session_id);

        // DESTINATION IS DRONE NACK
        let (packet, _session_id) = TestServerHelper::test_received_packet(
            PacketType::Nack(Nack {
                fragment_index: 0,
                nack_type: NackType::DestinationIsDrone,
            }),
            vec![5, 1],
        );
        assert_eq!(
            test_server_helper.server.network_topology.get_node_type(&5),
            Some(&NodeType::Client)
        );
        test_server_helper.server.handle_packet(packet);
        assert_eq!(
            test_server_helper.server.network_topology.get_node_type(&5),
            Some(&NodeType::Drone)
        );
        // reset to client
        test_server_helper
            .server
            .network_topology
            .update_node_type(5, NodeType::Client);
        assert_eq!(
            test_server_helper.server.network_topology.get_node_type(&5),
            Some(&NodeType::Client)
        );

        // PACKET DROPPED
        let fragment_index = 25;
        let (packet, session_id) = TestServerHelper::test_received_packet(
            PacketType::Nack(Nack {
                fragment_index,
                nack_type: NackType::Dropped,
            }),
            vec![3, 1],
        );
        let fragment = TestServerHelper::test_fragment(fragment_index, 1);
        test_server_helper
            .server
            .session_manager
            .add_session(session_id, vec![fragment], 6);

        test_server_helper.server.handle_packet(packet);
        let received_packet = test_server_helper
            .packet_recv_3
            .try_recv()
            .expect("No recover packet received on channel 3");
        assert_eq!(received_packet.session_id, session_id);

        assert_eq!(test_server_helper.server.network_topology.get_node_cost(&3).unwrap(), 40);

        // UNEXPECTED RECIPIENT
        // nothing to do
    }
}
