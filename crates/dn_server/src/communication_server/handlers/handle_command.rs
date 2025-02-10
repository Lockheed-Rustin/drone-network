//! This module handles incoming commands from the Simulation Controller for modifying the
//! communication server's configuration.

use crate::communication_server::communication_server::CommunicationServer;
use dn_controller::ServerCommand;

impl CommunicationServer {
    /// Handles incoming commands to modify the server's neighbors.
    ///
    /// This function processes commands sent to the server, allowing the addition or removal
    /// of packet senders and the server to be stopped from running.
    /// When a sender is added or removed, the network topology is updated to reflect the changes.
    ///
    /// # Arguments
    /// * `command` - The command to be processed. It can be one of the following:
    ///   - `AddSender(node_id, sender)` to add a new sender to the server.
    ///   - `RemoveSender(node_id)` to remove an existing sender from the server.
    ///   - `Return` to stop the server's execution.
    pub(crate) fn handle_command(&mut self, command: ServerCommand) {
        match command {
            ServerCommand::AddSender(node_id, sender) => {
                self.packet_send.insert(node_id, sender);
                self.network_topology.add_edge(self.id, node_id);
            }
            ServerCommand::RemoveSender(node_id) => {
                self.packet_send.remove(&node_id);
                self.network_topology.remove_node(node_id);
            }
            ServerCommand::Return => {
                self.running = false;
            }
        }
    }
}
