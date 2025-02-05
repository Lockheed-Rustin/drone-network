//! This module handles the flooding mechanism for updating and managing the network topology
//! in the communication server. It processes flood request and response packets, and updates
//! the network's structure by adding nodes and edges to the topology. The flooding mechanism
//! helps in propagating network information across nodes to maintain a consistent view of the
//! network topology for routing and communication purposes.

use crate::communication_server_code::communication_server::CommunicationServer;
use dn_controller::ServerEvent;
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{FloodRequest, FloodResponse, NodeType, Packet, PacketType};

impl CommunicationServer {
    /// Sends a flood response packet in reply to a flood request.
    ///
    /// This function creates a flood response packet based on the received flood request, updating
    /// the path trace to include the server's ID. It reverses the path trace to determine the
    /// return path and sends the response to the next node in the return path. Additionally, it
    /// notifies the controller about the packet sent.
    ///
    /// # Panics
    /// - The call to `send` on the `packet_send` for the next hop may panic if the channel is closed.
    ///   This should not happen unless there are unexpected issues with the communication channels.
    /// - Similarly, the call to `send` on the `controller_send` channel may panic if the channel is
    ///   unexpectedly closed.
    ///
    /// # Arguments
    /// * `flood_request` - The incoming flood request to reply to.
    pub(crate) fn send_flood_response(&mut self, mut flood_request: FloodRequest) {
        flood_request.path_trace.push((self.id, NodeType::Server));
        let mut hops = flood_request
            .path_trace
            .iter()
            .map(|(node_id, _)| *node_id)
            .rev()
            .collect::<Vec<_>>();
        // make sure there is the initiator ID in the path
        if hops.last() != Some(&flood_request.initiator_id) {
            hops.push(flood_request.initiator_id);
        }

        let session_id = self.session_manager.get_and_increment_session_id_counter();
        let flood_response_packet = Packet {
            pack_type: PacketType::FloodResponse(FloodResponse {
                flood_id: flood_request.flood_id,
                path_trace: flood_request.path_trace,
            }),
            routing_header: SourceRoutingHeader { hop_index: 1, hops },
            session_id,
        };

        // assuming the first drone connected to the server exists
        if self
            .packet_send
            .contains_key(&flood_response_packet.routing_header.hops[1])
        {
            self.packet_send[&flood_response_packet.routing_header.hops[1]]
                .send(flood_response_packet.clone())
                .expect("Error in send");
            self.controller_send
                .send(ServerEvent::PacketSent(flood_response_packet))
                .expect("Error in controller_send");
        }
    }

    /// Handles a flood response packet by updating the network topology.
    ///
    /// This function processes the received flood response to update the local network topology.
    /// It adds any new nodes and edges to the topology based on the path trace contained in the
    /// response. For each pair of consecutive nodes in the path trace, it checks if the nodes and
    /// their connecting edge are already present in the topology. If not, they are added.
    /// It also saves the type of each node in `topology_nodes_type`.
    ///
    /// If any newly discovered nodes have pending messages waiting to be sent, this function
    /// attempts to send them. The same happens for waiting fragments in the session manager.
    ///
    /// # Arguments
    /// * `response` - The flood response to process.
    pub(crate) fn handle_flood_response(&mut self, response: FloodResponse) {
        for &(node_id, node_type) in &response.path_trace {
            self.network_topology.add_node(node_id, node_type);
        }

        for window in response.path_trace.windows(2) {
            let (node_a, _) = window[0];
            let (node_b, _) = window[1];
            self.network_topology.add_edge(node_a, node_b);
        }

        // Check for pending messages and fragments that can now be sent
        for &(node_id, _) in &response.path_trace {
            if self.pending_messages_queue.has_pending_messages(&node_id) {
                if let Some(messages) = self.pending_messages_queue.take_pending_messages(&node_id)
                {
                    for message in messages {
                        self.send_message(message, node_id);
                    }
                }
            }
            if self.session_manager.hash_waiting_fragments(&node_id) {
                if let Some(fragments) = self.session_manager.take_waiting_fragments(&node_id) {
                    for (fragment_index, session_id) in fragments {
                        self.recover_fragment(session_id, fragment_index)
                    }
                }
            }
        }
    }

    /// Sends a flood request to update the server network topology.
    ///
    /// This function generates a flood request to start the process of updating the network
    /// topology. It includes a unique flood ID and the current server's ID in the path trace.
    /// The request is then sent to all connected nodes to propagate the updated topology.
    /// Additionally, the controller is notified about the packet being sent.
    pub(crate) fn update_network_topology(&mut self) {
        // Univocal flood id
        let flood_id = self.flood_id_counter;
        self.flood_id_counter += 1;

        let flood_request = FloodRequest {
            flood_id,
            initiator_id: self.id,
            path_trace: vec![(self.id, NodeType::Server)],
        };

        let session_id = self.session_manager.get_and_increment_session_id_counter();
        let flood_request_packet = Packet {
            pack_type: PacketType::FloodRequest(flood_request),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![],
            },
            session_id,
        };

        for (_, sender) in self.packet_send.iter() {
            sender
                .send(flood_request_packet.clone())
                .expect("Error in send");
        }

        self.controller_send
            .send(ServerEvent::PacketSent(flood_request_packet))
            .expect("Error in controller_send");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;
    use crossbeam_channel::unbounded;
    use dn_message::Message;
    use dn_message::ServerBody::ErrUnsupportedRequestType;
    use std::collections::HashMap;
    use wg_2024::packet::{Fragment, Nack, NackType};

    #[test]
    fn test_send_flood_response() {
        let (controller_send_event, controller_recv_event) = unbounded();
        let (_controller_send_command, controller_recv_command) = unbounded();
        let (packet_send, packet_recv) = unbounded();
        let mut packet_map = HashMap::new();
        let node_id = 2;
        packet_map.insert(1, packet_send.clone());

        let mut communication_server = CommunicationServer::new(
            controller_send_event,
            controller_recv_command,
            packet_map,
            packet_recv,
            node_id,
        );

        let flood_request = FloodRequest {
            flood_id: 42,
            initiator_id: 0,
            path_trace: vec![(0, NodeType::Client), (1, NodeType::Drone)],
        };

        communication_server.send_flood_response(flood_request);
        let validate_flood_response = |sent_packet: &Packet| {
            if let PacketType::FloodResponse(flood_response) = &sent_packet.pack_type {
                assert_eq!(flood_response.flood_id, 42);
                assert_eq!(
                    flood_response.path_trace,
                    vec![
                        (0, NodeType::Client),
                        (1, NodeType::Drone),
                        (2, NodeType::Server)
                    ]
                );
            } else {
                panic!("Expected FloodResponse packet");
            }
        };
        // tests the flood response
        if let Ok(sent_packet) = communication_server.packet_recv.try_recv() {
            validate_flood_response(&sent_packet);
            assert_eq!(sent_packet.routing_header.hops, vec![2, 1, 0]);
        } else {
            panic!("No packet was sent");
        }

        // tests the server event
        if let Ok(event) = controller_recv_event.try_recv() {
            if let ServerEvent::PacketSent(packet) = event {
                validate_flood_response(&packet);
            } else {
                panic!("Expected ServerEvent::PacketSent");
            }
        } else {
            panic!("No server event was generated");
        }
    }

    #[test]
    fn test_handle_flood_response() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;

        let flood_response = FloodResponse {
            flood_id: 1,
            path_trace: vec![
                (1, NodeType::Server),
                (2, NodeType::Drone),
                (25, NodeType::Drone),
                (26, NodeType::Client),
            ],
        };

        assert!(!server.network_topology.contains_node(25));
        assert!(!server.network_topology.contains_node(26));
        assert!(!server.network_topology.contains_edge(2, 25));
        assert!(!server.network_topology.contains_edge(25, 26));
        assert!(!server.network_topology.contains_type(&25));
        assert!(!server.network_topology.contains_type(&26));

        server.handle_flood_response(flood_response);

        assert!(server.network_topology.contains_node(25));
        assert!(server.network_topology.contains_node(26));
        assert!(server.network_topology.contains_edge(2, 25));
        assert!(server.network_topology.contains_edge(25, 26));
        assert_eq!(
            server.network_topology.get_node_type(&25),
            Some(&NodeType::Drone)
        );
        assert_eq!(
            server.network_topology.get_node_type(&26),
            Some(&NodeType::Client)
        );
    }

    #[test]
    fn test_handle_flood_response_pending_messages_recovery() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;
        server.network_topology.remove_node(6);
        server.send_message(Message::Server(ErrUnsupportedRequestType), 6);
        assert!(server.pending_messages_queue.has_pending_messages(&6));
        server.handle_flood_response(FloodResponse {
            flood_id: 1,
            path_trace: vec![
                (1, NodeType::Server),
                (3, NodeType::Drone),
                (6, NodeType::Client),
            ],
        });
        assert!(!server.pending_messages_queue.has_pending_messages(&6));
        let flood_req = helper.packet_recv_3.recv().unwrap();
        match flood_req.pack_type {
            PacketType::FloodRequest(_) => {
                assert!(true);
            }
            _ => {
                assert!(false);
            }
        }
        let packet = helper.packet_recv_3.recv().unwrap();
        if let PacketType::MsgFragment(_) = packet.pack_type {
            assert_eq!(packet.routing_header.hops, vec![1, 3, 6]);
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_handle_flood_response_waiting_fragments_recovery() {
        let helper = TestServerHelper::new();
        let mut server = helper.server;
        let session_id = 12;
        let fragments = vec![
            Fragment {
                fragment_index: 0,
                total_n_fragments: 3,
                length: 0,
                data: [0; 128],
            },
            Fragment {
                fragment_index: 1,
                total_n_fragments: 3,
                length: 0,
                data: [0; 128],
            },
            Fragment {
                fragment_index: 2,
                total_n_fragments: 3,
                length: 0,
                data: [0; 128],
            },
        ];
        server.session_manager.add_session(session_id, fragments, 6);
        assert!(server.network_topology.contains_edge(3, 6));
        for f_ind in 0..3 {
            let packet = Packet {
                routing_header: SourceRoutingHeader {
                    hop_index: 1,
                    hops: vec![3, 1],
                },
                session_id,
                pack_type: PacketType::Nack(Nack {
                    fragment_index: f_ind,
                    nack_type: NackType::ErrorInRouting(6),
                }),
            };
            server.handle_packet(packet);
        }
        assert!(!server.network_topology.contains_edge(3, 6));

        assert!(server.session_manager.hash_waiting_fragments(&6));

        for _ in 0..3 {
            let flood_req = helper.packet_recv_3.try_recv().unwrap();
            println!("[DEBUG] {:?}", flood_req);
            match flood_req.pack_type {
                PacketType::FloodRequest(_) => {
                    assert!(true);
                }
                _ => {
                    assert!(false);
                }
            }
        }

        let flood_response = FloodResponse {
            flood_id: 0,
            path_trace: vec![
                (1, NodeType::Server),
                (3, NodeType::Drone),
                (7, NodeType::Drone),
                (6, NodeType::Client),
            ],
        };
        server.handle_flood_response(flood_response);
        assert!(!server.session_manager.hash_waiting_fragments(&6));

        for _ in 0..3 {
            let packet = helper.packet_recv_3.try_recv().unwrap();
            println!("[DEBUG] {:?}", packet);
            if let PacketType::MsgFragment(_) = packet.pack_type {
                assert_eq!(packet.routing_header.hops, vec![1, 3, 7, 6]);
            } else {
                assert!(false);
            }
        }
    }
}
