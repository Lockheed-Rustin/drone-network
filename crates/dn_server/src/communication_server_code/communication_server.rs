use crate::communication_server_code::communication_server_topology::CommunicationServerNetworkTopology;
use crate::communication_server_code::pending_message_queue::PendingMessagesQueue;
use crate::communication_server_code::session_manager::SessionManager;
use crossbeam_channel::{select_biased, Receiver, Sender};
use dn_controller::{ServerCommand, ServerEvent};
use dn_message::assembler::Assembler;
use std::collections::{HashMap, HashSet};
use wg_2024::network::NodeId;
use wg_2024::packet::Packet;
// TODO: instead of firing a lot of update_topology(), try a way to limit them (like when receiving a lot of nack for the same issue)
// TODO: nack tests to be checked.
// TODO: use reference when possible
// TODO: use shortcuts in case there is not a path (just for ack/nack?)

// TODO: IMPORTANTE non puoi fare mandare ad un client non registrato un messaggio. Vanno modificati i messaggi
pub struct CommunicationServer {
    pub(crate) controller_send: Sender<ServerEvent>,
    pub(crate) controller_recv: Receiver<ServerCommand>,
    pub(crate) packet_send: HashMap<NodeId, Sender<Packet>>,
    pub(crate) packet_recv: Receiver<Packet>,

    pub(crate) running: bool,
    pub(crate) id: NodeId,
    pub(crate) flood_id_counter: u64,
    pub(crate) session_manager: SessionManager,
    pub(crate) pending_messages_queue: PendingMessagesQueue,
    pub(crate) assembler: Assembler,
    pub(crate) network_topology: CommunicationServerNetworkTopology,
    pub(crate) registered_clients: HashSet<NodeId>,
}

impl CommunicationServer {
    pub fn new(
        controller_send: Sender<ServerEvent>,
        controller_recv: Receiver<ServerCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
        id: NodeId,
    ) -> Self {
        Self {
            controller_send,
            controller_recv,
            packet_send,
            packet_recv,
            id,
            running: false,
            flood_id_counter: 0,
            session_manager: SessionManager::new(),
            pending_messages_queue: PendingMessagesQueue::new(),
            registered_clients: HashSet::new(),
            network_topology: CommunicationServerNetworkTopology::new(),
            assembler: Assembler::new(),
        }
    }

    pub fn run(&mut self) {
        self.running = true;
        self.update_network_topology(); // first discovery of the network
        while self.running {
            select_biased! {
                recv(self.controller_recv) -> command => {
                    if let Ok(cmd) = command {
                        self.handle_command(cmd);
                        if !self.running { break; }
                    }
                },
                recv(self.packet_recv) -> packet => {
                    if let Ok(p) = packet {
                        self.handle_packet(p);
                        if !self.running { break; }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;
    use crossbeam_channel::unbounded;
    use std::thread;
    use std::time::Duration;
    use wg_2024::network::SourceRoutingHeader;
    use wg_2024::packet::PacketType::MsgFragment;
    use wg_2024::packet::{Fragment, PacketType};

    #[test]
    fn test_run() {
        // receiving events from the controller
        let (send_from_controller_to_server, recv_from_controller): (
            Sender<ServerCommand>,
            Receiver<ServerCommand>,
        ) = unbounded();

        // sending events to the controller
        let (send_from_server_to_controller, _recv_from_server): (
            Sender<ServerEvent>,
            Receiver<ServerEvent>,
        ) = unbounded();

        let (send_packet_to_server, packet_recv_1): (Sender<Packet>, Receiver<Packet>) =
            unbounded();
        let (packet_send_2, _packet_recv_2): (Sender<Packet>, Receiver<Packet>) = unbounded();
        let (packet_send_3, _packet_recv_3): (Sender<Packet>, Receiver<Packet>) = unbounded();
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

        let _handle = thread::spawn(move || {
            server.run();
        });

        thread::sleep(Duration::from_millis(10));
        send_packet_to_server
            .send(Packet {
                routing_header: SourceRoutingHeader {
                    hop_index: 1,
                    hops: vec![5, 1],
                },
                session_id: 111,
                pack_type: MsgFragment(Fragment {
                    fragment_index: 0,
                    total_n_fragments: 1,
                    length: 0,
                    data: [0; 128],
                }),
            })
            .expect("Failed to send packet");
        thread::sleep(Duration::from_millis(10));
        let _flood_req = packet_recv_5.recv().expect("Failed to recv");
        let ack = packet_recv_5.recv().expect("Failed to recv");
        if let PacketType::Ack(a) = ack.pack_type {
            assert_eq!(a.fragment_index, 0);
        } else {
            panic!("expected ack");
        }
        thread::sleep(Duration::from_millis(10));
        send_from_controller_to_server
            .send(ServerCommand::Return)
            .expect("Failed to send command to server");
    }
}
