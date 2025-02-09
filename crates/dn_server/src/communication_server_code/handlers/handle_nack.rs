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
use crate::communication_server_code::session_manager::{FragmentIndex, SessionId};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Nack, NackType, NodeType, Packet, PacketType};

impl CommunicationServer {
    /// This function processes an incoming NACK and takes the appropriate action based on its type.
    /// The action taken can vary from recovering dropped message fragments to fixing routing issues.
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
                if source_routing_header.hops[0] != error_node {
                    self.network_topology
                        .remove_edge(source_routing_header.hops[0], error_node);
                } else {
                    self.network_topology
                        .remove_edge(source_routing_header.hops[1], error_node);
                }
                self.recover_after_nack(session_id, nack.fragment_index, true);
            }
            NackType::DestinationIsDrone => {
                self.network_topology
                    .update_node_type(source_routing_header.hops[0], NodeType::Drone);
            }
            NackType::Dropped => {
                self.network_topology
                    .update_estimated_pdr(source_routing_header.hops[0], true);

                if self
                    .session_manager
                    .already_dropped
                    .contains(&(session_id, nack.fragment_index))
                {
                    self.recover_after_nack(session_id, nack.fragment_index, true);
                } else {
                    self.session_manager
                        .already_dropped
                        .insert((session_id, nack.fragment_index));
                    self.recover_after_nack(session_id, nack.fragment_index, false);
                }
            }
            NackType::UnexpectedRecipient(_) => {
                self.recover_after_nack(session_id, nack.fragment_index, true);
            }
        }
    }

    /// Recovers a dropped message fragment after a NACK has been received.
    ///
    /// This function retrieves the destination node ID associated with the given session from the
    /// session manager (assuming that an entry exists in the pending sessions destination map).
    /// Then, it removes the saved routing path for that destination from the network topology,
    /// updates the topology, and finally attempts to recover the dropped fragment by calling
    /// `recover_fragment`.
    ///
    /// # Arguments
    /// * `session_id` - The identifier of the session in which the fragment was dropped.
    /// * `fragment_index` - The index of the fragment that needs to be recovered.
    /// * `send_flood` - True if the caller want to send a flood request to update the topology.
    ///
    /// # Panics
    /// This function will panic if there is no destination associated with the session in the session manager,
    /// as indicated by the use of `unwrap()` on the result of `get_pending_sessions_destination`.
    fn recover_after_nack(
        &mut self,
        session_id: SessionId,
        fragment_index: FragmentIndex,
        send_flood: bool,
    ) {
        let dest_id = self
            .session_manager
            .get_pending_sessions_destination(&session_id)
            .unwrap(); // if a packet was dropped, I'm sure that there is an entry in the HashMap
        self.network_topology.remove_path(dest_id);
        if send_flood {
            self.update_network_topology();
        }
        self.recover_fragment(session_id, fragment_index);
    }

    /// Attempts to recover a dropped message fragment and retransmit it.
    ///
    /// This function checks if the requested fragment exists in the session manager and attempts to
    /// reassemble it. If the fragment is successfully recovered, it is transmitted to the correct
    /// recipient.
    ///
    /// If the path to the recipient is not known, the fragment index is added to the waiting
    /// fragments list.
    ///
    /// ### Arguments:
    /// - `session_id`: The session ID associated with the fragment.
    /// - `fragment_index`: The index of the fragment to recover.
    ///
    /// # Panics
    /// This function panics if the specified fragment does not exist in the session manager.
    /// This indicates that an invalid session ID or fragment index was provided.
    pub(crate) fn recover_fragment(&mut self, session_id: SessionId, fragment_index: u64) {
        if let Some((fragment, dest)) = self
            .session_manager
            .recover_fragment(session_id, fragment_index)
        {
            let hops = self
                .network_topology
                .source_routing(self.id, dest)
                .expect("Error in routing");

            if !hops.is_empty() {
                let packet = Packet {
                    routing_header: SourceRoutingHeader { hop_index: 1, hops },
                    session_id,
                    pack_type: PacketType::MsgFragment(fragment),
                };
                self.send_packet(packet);
            } else {
                // I don't know the path to `dest` yet
                self.session_manager
                    .add_to_waiting_fragments(dest, fragment_index, session_id);
            }
        } else {
            panic!(
                "tried to recover a fragment that is not in the session_manager.\n\
                    session_id: {}\n\
                    fragment_index: {}\n",
                session_id, fragment_index
            );
        }
    }

    /// Sends a NACK packet over the network.
    ///
    /// This function wraps the given `Nack` into a `Packet` with a source routing header constructed from
    /// the provided `hops` vector, assigns a new session ID using the session manager, and then forwards the
    /// packet by invoking `send_packet`.
    ///
    /// The `hops` vector represents the intended routing path.
    ///
    /// # Arguments
    ///
    /// * `nack` - The negative acknowledgment data that indicates a transmission error or problem.
    /// * `hops` - A vector of `NodeId` representing the routing path that the packet should follow.
    pub(crate) fn send_nack(&mut self, nack: Nack, hops: Vec<NodeId>) {
        let source_routing_header = SourceRoutingHeader { hop_index: 1, hops };
        let packet = Packet {
            routing_header: source_routing_header,
            session_id: self.session_manager.get_and_increment_session_id_counter(),
            pack_type: PacketType::Nack(nack),
        };
        self.send_packet(packet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;
    use wg_2024::packet::Ack;
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

        test_server_helper.server.network_topology.add_edge(2, 3);
        let (packet, session_id) = TestServerHelper::test_received_packet(
            PacketType::Nack(Nack {
                fragment_index,
                nack_type: NackType::ErrorInRouting(3),
            }),
            vec![3, 2, 1], // it could happen if a drone is crashing and has fragments to process
        );
        // error node == hops[0]

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

        test_server_helper.server.handle_packet(packet.clone());
        assert!(test_server_helper
            .server
            .session_manager
            .already_dropped
            .contains(&(session_id, fragment_index)));
        let received_packet = test_server_helper
            .packet_recv_3
            .try_recv()
            .expect("No recover packet received on channel 3");
        assert_eq!(received_packet.session_id, session_id);

        assert_eq!(
            test_server_helper
                .server
                .network_topology
                .get_node_cost(&3)
                .unwrap(),
            40
        );

        test_server_helper.server.handle_packet(packet.clone());
        assert!(!test_server_helper
            .server
            .session_manager
            .already_dropped
            .contains(&(session_id, fragment_index)));

        let _received_flood_req = test_server_helper
            .packet_recv_3
            .try_recv()
            .expect("No recover packet received on channel 3");

        test_server_helper.server.handle_packet(packet);
        let (packet, session_id) = TestServerHelper::test_received_packet(
            PacketType::Ack(Ack { fragment_index }),
            vec![6, 3, 1],
        );
        test_server_helper.server.handle_packet(packet);
        assert!(!test_server_helper
            .server
            .session_manager
            .already_dropped
            .contains(&(session_id, fragment_index)));
        // UNEXPECTED RECIPIENT
        // nothing to do
    }

    #[test]
    fn test_waiting_fragment_added() {
        let mut test_server_helper = TestServerHelper::new();

        let fragment_index = 23;
        let (packet, session_id) = TestServerHelper::test_received_packet(
            PacketType::Nack(Nack {
                fragment_index,
                nack_type: NackType::ErrorInRouting(6),
            }),
            vec![3, 1],
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
            .contains_edge(3, 6));
        test_server_helper.server.handle_packet(packet);
        assert!(!test_server_helper
            .server
            .network_topology
            .contains_edge(3, 6));

        let flood_req = test_server_helper
            .packet_recv_3
            .try_recv()
            .expect("Expected flood_req because of update topology");
        match flood_req.pack_type {
            PacketType::FloodRequest(_) => {}
            _ => panic!("Expected FloodRequest pack"),
        }

        // THERE IS NO KNOWN PATH TO 6 NOW
        assert!(test_server_helper
            .server
            .session_manager
            .hash_waiting_fragments(&6));
    }
}
