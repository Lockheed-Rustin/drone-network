use crate::communication_server_code::communication_server::CommunicationServer;
use crate::communication_server_code::communication_server_topology::CommunicationServerNetworkTopology;
use crossbeam_channel::{unbounded, Receiver, Sender};
use dn_controller::{ServerCommand, ServerEvent};
use dn_message::{ClientBody, ClientCommunicationBody, Message};
use rand::Rng;
use std::collections::HashMap;
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Fragment, NodeType, Packet, PacketType};

pub struct TestServerHelper {
    pub server: CommunicationServer,
    pub send_to_server: Sender<Packet>,
    pub packet_recv_2: Receiver<Packet>,
    pub packet_recv_3: Receiver<Packet>,
    pub packet_recv_5: Receiver<Packet>,
    pub event_recv_from_server: Receiver<ServerEvent>,
}

impl TestServerHelper {
    pub fn new() -> Self {
        // receiving events from the controller
        let (_send_from_controller_to_server, recv_from_controller): (
            Sender<ServerCommand>,
            Receiver<ServerCommand>,
        ) = unbounded();

        // sending events to the controller
        let (send_from_server_to_controller, recv_from_server): (
            Sender<ServerEvent>,
            Receiver<ServerEvent>,
        ) = unbounded();

        let (send_to_server, packet_recv_1): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_2, packet_recv_2): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_3, packet_recv_3): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_5, packet_recv_5): (Sender<Packet>, Receiver<Packet>) = unbounded();

        let mut packet_send_map = HashMap::new();
        packet_send_map.insert(3, packet_send_3);
        packet_send_map.insert(2, packet_send_2);
        packet_send_map.insert(5, packet_send_5);

        let mut server = CommunicationServer::new(
            send_from_server_to_controller,
            recv_from_controller,
            packet_send_map,
            packet_recv_1,
            1,
        );

        TestServerHelper::init_topology(&mut server);

        Self {
            server,
            send_to_server,
            packet_recv_2,
            packet_recv_3,
            packet_recv_5,
            event_recv_from_server: recv_from_server,
        }
    }

    fn init_topology(communication_server: &mut CommunicationServer) {
        let mut topology = CommunicationServerNetworkTopology::new();

        topology.add_node(1, NodeType::Server);
        topology.add_node(2, NodeType::Drone);
        topology.add_node(3, NodeType::Drone);
        topology.add_node(4, NodeType::Drone);
        topology.add_node(5, NodeType::Client);
        topology.add_node(6, NodeType::Client);
        topology.add_node(7, NodeType::Drone);

        topology.add_edge(1, 2);
        topology.add_edge(2, 3);
        topology.add_edge(3, 7);
        topology.add_edge(7, 4);
        topology.add_edge(4, 5);
        topology.add_edge(1, 5);
        topology.add_edge(3, 1);
        topology.add_edge(3, 6);

        communication_server.network_topology = topology;
    }

    pub fn test_received_packet(
        packet_type: PacketType,
        hops: Vec<NodeId>,
    ) -> (Packet, u64) {
        let session_id: u64 = rand::rng().random();
        (
            Packet {
                routing_header: SourceRoutingHeader { hop_index: hops.len()-1, hops },
                session_id,
                pack_type: packet_type,
            },
            session_id,
        )
    }

    pub fn test_fragment(fragment_index: u64, total_n_fragments: u64) -> Fragment {
        let data: [u8; 128] = [0; 128];
        Fragment {
            fragment_index,
            total_n_fragments,
            length: 0,
            data,
        }
    }

    pub fn test_client_message(client_communication_body: ClientCommunicationBody) -> Message {
        Message::Client(ClientBody::ClientCommunication(client_communication_body))
    }

    pub fn wait_for_ack_on_node_3(&self, nr_of_fragments: usize) {
        for _ in 0..nr_of_fragments {
            self.packet_recv_3.try_recv().expect("Expected ack packet");
        }
    }

    pub fn send_fragments_to_server(&mut self, fragments: Vec<Fragment>) {
        for f in fragments {
            let (packet, _session_id) = TestServerHelper::test_received_packet(PacketType::MsgFragment(f), vec![6, 3, 1]);
            self.server.handle_packet(packet);
        }
    }

    pub fn serialize_message(&self, message: Message) -> Vec<Fragment> {
        self
            .server
            .assembler
            .serialize_message(message)
    }

    pub fn reconstruct_response_on_node_3(&mut self, nr_of_fragments: usize) -> Message {
        let mut reconstructed_response = None;
        for _ in 0..nr_of_fragments {
            let response_packet = self
                .packet_recv_3
                .try_recv()
                .expect("Expected recv packet");
            if let PacketType::MsgFragment(f) = response_packet.pack_type {
                reconstructed_response = self.server.assembler.handle_fragment(
                    f,
                    response_packet.routing_header.hops[0],
                    response_packet.session_id,
                );
            }
        }

        reconstructed_response.expect("Expected reconstructed response")
    }
}
