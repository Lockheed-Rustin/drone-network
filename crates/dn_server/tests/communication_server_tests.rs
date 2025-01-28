use crossbeam_channel::{unbounded, Sender, Receiver};
use std::collections::HashMap;
use wg_2024::packet::{NodeType, Packet};
use dn_controller::{ServerCommand, ServerEvent};
use dn_server::communication_server_code::communication_server::CommunicationServer;
use dn_server::communication_server_code::communication_server_topology::CommunicationServerNetworkTopology;

fn init_server() -> CommunicationServer {
    // receiving commands from controller
    let (_, controller_recv): (Sender<ServerCommand>, Receiver<ServerCommand>) = unbounded();

    // sending events to the controller
    let (controller_send, _): (Sender<ServerEvent>, Receiver<ServerEvent>) = unbounded();

    let (_packet_send_1, packet_recv_1): (Sender<Packet>, Receiver<Packet>) = unbounded();
    let (packet_send_2, _packet_recv_2): (Sender<Packet>, Receiver<Packet>) = unbounded();
    let (packet_send_3, _packet_recv_3): (Sender<Packet>, Receiver<Packet>) = unbounded();
    let (packet_send_5, _packet_recv_5): (Sender<Packet>, Receiver<Packet>) = unbounded();

    let mut packet_send_map = HashMap::new();
    packet_send_map.insert(3, packet_send_2);
    packet_send_map.insert(2, packet_send_3);
    packet_send_map.insert(5, packet_send_5);

    let mut c_s = CommunicationServer::new(
        controller_send,
        controller_recv,
        packet_send_map,
        packet_recv_1,
        1,
    );
    init_topology(&mut c_s);
    c_s
}

fn init_topology(communication_server: &mut CommunicationServer)    {
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

#[cfg(test)]
mod tests {
    use wg_2024::packet::{FloodRequest, FloodResponse, NodeType, PacketType};
    use super::*;

    // TODO: test with wrong path like the server is a node in the middle of a path. What happens?

    #[test]
    fn test_source_routing() {
        let mut server = init_server();

        let route = server.network_topology.source_routing(server.id, 1);

        assert!(!route.is_empty());
        assert_eq!(route[0], server.id);

        let route = server.network_topology.source_routing(server.id, 3);

        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);

        let route = server.network_topology.source_routing(server.id, 4);
        // should avoid passing through 5 because it's a client
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);
        assert_eq!(route[2], 7);
        assert_eq!(route[3], 4);

        server.network_topology.node_types.insert(7, NodeType::Server);
        let route = server.network_topology.source_routing(server.id, 4);
        // should avoid passing through 5 because it's a client, but also through 7 because it's a
        // server. So the path is empty
        assert!(route.is_empty());

        server.network_topology.node_types.insert(5, NodeType::Drone);
        server.network_topology.node_types.insert(7, NodeType::Drone);
        // should pass through 5 now because it's a drone and the path is shorter than the one passing
        // through 7
        let route = server.network_topology.source_routing(server.id, 4);
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 5);
        assert_eq!(route[2], 4);

    }

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

        // tests the flood response
        if let Ok(sent_packet) = communication_server.packet_recv.try_recv() {
            if let PacketType::FloodResponse(flood_response) = sent_packet.pack_type {
                assert_eq!(flood_response.flood_id, 42);
                assert_eq!(
                    flood_response.path_trace,
                    vec![(0, NodeType::Client), (1, NodeType::Drone), (2, NodeType::Server)]
                );
            } else {
                panic!("Expected FloodResponse packet");
            }
            assert_eq!(sent_packet.routing_header.hops, vec![2, 1, 0]);
        } else {
            panic!("No packet was sent");
        }

        // tests the server event
        if let Ok(event) = controller_recv_event.try_recv() {
            if let ServerEvent::PacketSent(packet) = event {
                if let PacketType::FloodResponse(flood_response) = packet.pack_type {
                    assert_eq!(flood_response.flood_id, 42);
                    assert_eq!(
                        flood_response.path_trace,
                        vec![(0, NodeType::Client), (1, NodeType::Drone), (2, NodeType::Server)]
                    );
                } else {
                    panic!("Expected FloodResponse packet");
                }
            } else {
                panic!("Expected ServerEvent::PacketSent");
            }
        } else {
            panic!("No server event was generated");
        }
    }

    #[test]
    fn test_handle_flood_response() {
        let mut server = init_server();

        let flood_response = FloodResponse {
            flood_id: 1,
            path_trace: vec![
                (1, NodeType::Server),
                (2, NodeType::Drone),
                (25, NodeType::Drone),
                (26, NodeType::Client),
            ],
        };

        assert!(!server.network_topology.graph.contains_node(25));
        assert!(!server.network_topology.graph.contains_node(26));
        assert!(!server.network_topology.graph.contains_edge(2, 25));
        assert!(!server.network_topology.graph.contains_edge(25, 26));
        assert!(!server.network_topology.node_types.contains_key(&25));
        assert!(!server.network_topology.node_types.contains_key(&26));

        server.handle_flood_response(flood_response);

        assert!(server.network_topology.graph.contains_node(25));
        assert!(server.network_topology.graph.contains_node(26));

        assert!(server.network_topology.graph.contains_edge(2, 25));
        assert!(server.network_topology.graph.contains_edge(25, 26));

        assert_eq!(server.network_topology.node_types.get(&25), Some(&NodeType::Drone));
        assert_eq!(server.network_topology.node_types.get(&26), Some(&NodeType::Client));
    }

}

