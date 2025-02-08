//! # `CommunicationServer`
//! This module defines the core `CommunicationServer`, which is responsible for managing
//! communication between clients.
//!
//! The `CommunicationServer` integrates several components such as channels for receiving
//! commands and packets, a session manager, a pending messages queue, an assembler for
//! reassembling fragmented messages, and a network topology manager.
//!
//! It provides functionality to initialize the server, run its main event loop, and handle
//! various events (e.g., commands, incoming packets) as they arrive. The server is designed
//! to work in a drone network environment where it dynamically updates its routing and
//! topology based on received events.

use crate::communication_server_code::communication_server_topology::CommunicationServerNetworkTopology;
use crate::communication_server_code::pending_message_queue::PendingMessagesQueue;
use crate::communication_server_code::session_manager::SessionManager;
use crossbeam_channel::{select_biased, Receiver, Sender};
use dn_controller::{ServerCommand, ServerEvent};
use dn_message::assembler::Assembler;
use std::collections::{HashMap, HashSet};
use wg_2024::network::NodeId;
use wg_2024::packet::Packet;

/// The `CommunicationServer` struct encapsulates the core components required for managing
/// network communication in a drone network. It handles sending and receiving control
/// messages and data packets, manages client registration and session state, maintains a queue
/// for pending messages when no route is known, and updates the network topology dynamically.
///
/// **Fields:**
/// - `controller_send`: A channel sender used to transmit server events to the simulation controller.
/// - `controller_recv`: A channel receiver to receive commands from the simulation controller.
/// - `packet_send`: A mapping of node IDs to channel senders for sending packets to other nodes.
/// - `packet_recv`: A channel receiver for incoming data packets.
/// - `running`: A flag indicating whether the server's main event loop is active.
/// - `id`: The unique identifier of the server.
/// - `flood_id_counter`: A counter used to generate unique flood IDs.
/// - `session_manager`: Manages session state, including message fragmentation and reassembly.
/// - `pending_messages_queue`: Stores messages that could not be sent because a route to the destination was not known.
/// - `assembler`: Responsible for reassembling fragmented messages and serialize messages ready to be sent.
/// - `network_topology`: Maintains the current view of the network topology for routing decisions.
/// - `registered_clients`: A set of node IDs representing clients that have been registered with the server.
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
    /// Creates a new instance of the `CommunicationServer`.
    ///
    /// This function initializes the `CommunicationServer` with the provided channels,
    /// a unique node ID, and default values for its internal components such as the session manager,
    /// pending messages queue, network topology, and assembler.
    ///
    /// # Parameters
    /// - `controller_send`: The channel for sending events to the controller.
    /// - `controller_recv`: The channel for receiving commands from the controller.
    /// - `packet_send`: A mapping from node IDs to channels for sending packets.
    /// - `packet_recv`: The channel for receiving packets.
    /// - `id`: The unique identifier for this server node.
    ///
    /// # Returns
    /// A new instance of `CommunicationServer`.
    #[must_use]
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

    /// Runs the `CommunicationServer`.
    ///
    /// This function starts the server's main event loop by setting the `running` flag to true and
    /// performing an initial network topology update. The server continuously listens for incoming
    /// commands (via `controller_recv`) and packets (via `packet_recv`). Depending on the received event,
    /// it delegates processing to the appropriate handler functions. The loop continues until the
    /// `running` flag is set to false.
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
