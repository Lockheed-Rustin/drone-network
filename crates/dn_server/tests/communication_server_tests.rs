use crossbeam_channel::{unbounded, Sender, Receiver};
use std::collections::HashMap;
use petgraph::prelude::UnGraphMap;
use wg_2024::network::NodeId;
use wg_2024::packet::Packet;
use dn_controller::{ServerCommand, ServerEvent};
use dn_server::communication_server::CommunicationServer;

fn init_server() -> CommunicationServer {
    // receiving commands from controller
    let (_, controller_recv): (Sender<ServerCommand>, Receiver<ServerCommand>) = unbounded();

    // sending events to the controller
    let (controller_send, _): (Sender<ServerEvent>, Receiver<ServerEvent>) = unbounded();

    // sending and receiving packets
    let (packet_send, packet_recv): (Sender<Packet>, Receiver<Packet>) = unbounded();

    let mut packet_send_map = HashMap::new();
    packet_send_map.insert(1, packet_send);

    let mut c_s = CommunicationServer::new(
        controller_send,
        controller_recv,
        packet_send_map,
        packet_recv,
        1,
    );
    init_topology(&mut c_s);
    c_s
}

fn init_topology(communication_server: &mut CommunicationServer) {
    let mut topology = UnGraphMap::<NodeId, ()>::new();

    topology.add_node(1);
    topology.add_node(2);
    topology.add_node(3);
    topology.add_node(4);
    topology.add_node(5);

    topology.add_edge(1, 2, ());
    topology.add_edge(2, 3, ());
    topology.add_edge(3, 4, ());
    topology.add_edge(4, 5, ());
    topology.add_edge(1, 5, ());
    topology.add_edge(3,  1, ());

    communication_server.topology = topology;

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_routing_valid_path() {
        let mut server = init_server();

        let route = server.source_routing(1);
        println!("{route:?}");

        assert!(!route.is_empty());
        assert_eq!(route[0], server.id);

        let route = server.source_routing(3);
        println!("{route:?}");

        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 3);

        let route = server.source_routing(4);
        println!("{route:?}");
        assert!(!route.is_empty());
        assert_eq!(route[0], 1);
        assert_eq!(route[1], 5);
        assert_eq!(route[2], 4);

    }
}

