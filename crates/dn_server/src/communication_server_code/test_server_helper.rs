use crate::communication_server_code::communication_server::CommunicationServer;
use crate::communication_server_code::communication_server_topology::CommunicationServerNetworkTopology;
use crossbeam_channel::{unbounded, Receiver, Sender};
use dn_controller::{ServerCommand, ServerEvent};
use dn_message::assembler::Assembler;
use dn_message::{ClientBody, ClientCommunicationBody, Message};
use rand::Rng;
use std::collections::HashMap;
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Fragment, NodeType, Packet, PacketType};

pub struct TestServerHelper {
    pub server: CommunicationServer,
    pub _send_packet_to_server: Sender<Packet>,
    pub packet_recv_2: Receiver<Packet>,
    pub packet_recv_3: Receiver<Packet>,
    pub packet_recv_5: Receiver<Packet>,
    pub _event_recv_from_server: Receiver<ServerEvent>,
    pub assembler: Assembler,
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

        let (_send_packet_to_server, packet_recv_1): (Sender<Packet>, Receiver<Packet>) =
            unbounded();
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
            _send_packet_to_server,
            packet_recv_2,
            packet_recv_3,
            packet_recv_5,
            _event_recv_from_server: recv_from_server,
            assembler: Assembler::new(),
        }
    }

    fn init_topology(communication_server: &mut CommunicationServer) {
        let mut topology = CommunicationServerNetworkTopology::new();

        topology.add_node(1, NodeType::Server);
        topology.add_node(2, NodeType::Drone);
        topology.add_node(3, NodeType::Drone);
        topology.add_node(4, NodeType::Client);
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

    pub fn test_received_packet(packet_type: PacketType, hops: Vec<NodeId>) -> (Packet, u64) {
        let session_id: u64 = rand::rng().random_range(0..=100);
        (
            Packet {
                routing_header: SourceRoutingHeader {
                    hop_index: hops.len() - 1,
                    hops,
                },
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

    pub fn wait_for_ack_on_node_x(&self, nr_of_fragments: usize, target_node: NodeId) {
        for _ in 0..nr_of_fragments {
            let _ack = match target_node {
                3 => {
                    self.packet_recv_3.try_recv().expect("Expected ack packet")
                },
                2 => { self.packet_recv_2.try_recv().expect("Expected ack packet") },
                5 => { self.packet_recv_5.try_recv().expect("Expected ack packet") },
                _ => { return;}
            };
        }
    }

    pub fn send_fragments_to_server(&mut self, fragments: Vec<Fragment>, hops: Vec<NodeId>) {
        for f in fragments {
            let (packet, _session_id) =
                TestServerHelper::test_received_packet(PacketType::MsgFragment(f), hops.clone());
            self.server.handle_packet(packet);
        }
    }

    pub fn serialize_message(&self, message: Message) -> Vec<Fragment> {
        self.assembler.serialize_message(message)
    }

    pub fn reconstruct_response_on_node_x(&mut self, target_node: NodeId) -> Message {
        let mut reconstructed_response = None;
        loop {
            let response_packet= match target_node {
                2 => {
                    self.packet_recv_2.try_recv()
                },
                3 => {
                    self.packet_recv_3.try_recv()
                },
                _ => {
                    self.packet_recv_5.try_recv()
                }
            };

            if let Ok(packet) = response_packet {
                if let PacketType::MsgFragment(fragment) = packet.pack_type {
                    reconstructed_response = self.assembler.handle_fragment(
                        fragment,
                        packet.routing_header.hops[0],
                        packet.session_id,
                    );
                }
            } else {
                panic!("[ERROR IN reconstruct_response_on_node_x]Expected a packet on node {}, but something went wrong", target_node);
            }

            if let Some(response) = reconstructed_response {
                return response;
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    pub fn register_client_6(&mut self) {
        let message = Message::Client(ClientBody::ClientCommunication(
            ClientCommunicationBody::ReqRegistrationToChat,
        ));
        let serialized_message = self.serialize_message(message);
        let nr_of_fragments = serialized_message.len();

        self.send_fragments_to_server(serialized_message, vec![6, 3, 1]);
        self.wait_for_ack_on_node_x(nr_of_fragments, 3);
    }

    pub fn send_message_and_get_response(&mut self, message: Message, hops: Vec<NodeId>, reconstruction_target_node: NodeId) -> Message {
        let serialized_message = self.serialize_message(message);
        self.send_fragments_to_server(serialized_message, hops);
        self.reconstruct_response_on_node_x(reconstruction_target_node)
    }
}
