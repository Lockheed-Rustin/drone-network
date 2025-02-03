//! This module is responsible for handling message fragments, managing the assembly of complete
//! messages, and processing incoming and outgoing messages in the communication server.
//! It provides functions for handling incoming message fragments, assembling and processing
//! messages based on their type, and sending acknowledgments and full messages to clients or servers.
//! Additionally, it handles the sending of fragmented messages using source routing.

use crate::communication_server_code::communication_server::CommunicationServer;
use crate::communication_server_code::session_manager::SessionId;
use dn_controller::ServerEvent;
use dn_message::{ClientBody, ClientCommunicationBody, Message};
use wg_2024::network::{NodeId, SourceRoutingHeader};
use wg_2024::packet::{Ack, Fragment, Packet, PacketType};

impl CommunicationServer {
    /// Processes a message fragment and handles its acknowledgment.
    ///
    /// This function processes an incoming message fragment by attempting to assemble it into a
    /// complete message. If the message is successfully assembled, it delegates the message
    /// handling to the appropriate method. Regardless of the assembly result, it sends an
    /// acknowledgment for the processed fragment.
    ///
    /// # Arguments
    /// * `f` - The fragment of the message to process.
    /// * `sender_id` - The ID of the sender of the fragment.
    /// * `session_id` - The session ID associated with the message.
    pub(crate) fn handle_fragment(
        &mut self,
        f: Fragment,
        sender_id: NodeId,
        session_id: SessionId,
    ) {
        self.send_ack(f.clone(), sender_id, session_id);
        if let Some(message) = self.assembler.handle_fragment(f, sender_id, session_id) {
            self.handle_message(message, sender_id);
        }
    }

    /// Handles incoming messages and executes the appropriate actions based on the message type.
    ///
    /// This function processes client requests, including server type queries, client registration,
    /// message forwarding, and client list requests. It ignores messages from other servers or for
    /// a content server.
    /// This function also notifies the simulation controller that a message has been assembled.
    ///
    /// # Arguments
    /// * `message` - The message to handle.
    /// * `sender_id` - The ID of the sender of the message.
    fn handle_message(&mut self, message: Message, sender_id: NodeId) {
        match message {
            Message::Client(cb) => {
                self.controller_send
                    .send(ServerEvent::MessageAssembled(cb.clone()))
                    .expect("Error in controller_send");
                match cb {
                    ClientBody::ReqServerType => {
                        self.send_server_type(sender_id);
                    }
                    ClientBody::ClientCommunication(comm_body) => match comm_body {
                        ClientCommunicationBody::ReqRegistrationToChat => {
                            self.register_client(sender_id);
                        }
                        ClientCommunicationBody::MessageSend(comm_message) => {
                            self.forward_message(comm_message);
                        }
                        ClientCommunicationBody::ReqClientList => {
                            self.registered_clients_list(sender_id);
                        }
                    },
                    ClientBody::ClientContent(_) => {} // ignoring messages for the content server
                }
            }
            Message::Server(_) => {} // ignoring messages received by other servers
        }
    }

    /// Sends an acknowledgment for a message fragment.
    ///
    /// This function creates an acknowledgment packet for the provided fragment and sends it
    /// to the specified recipient. It uses source routing to ensure the packet is routed correctly.
    ///
    /// # Arguments
    /// * `fragment` - The fragment for which to send an acknowledgment.
    /// * `to` - The recipient node ID.
    /// * `session_id` - The session ID associated with the message.
    ///
    /// # Panics
    /// * This function may panic if `hops` is unexpectedly empty.
    pub(crate) fn send_ack(&mut self, fragment: Fragment, to: NodeId, session_id: SessionId) {
        let ack = PacketType::Ack(Ack {
            fragment_index: fragment.fragment_index,
        });
        let hops = self
            .network_topology
            .source_routing(self.id, to)
            .expect("Error in routing");

        if !hops.is_empty() {
            let packet = Packet {
                pack_type: ack,
                routing_header: SourceRoutingHeader { hop_index: 1, hops },
                session_id,
            };
            self.send_packet(packet);
        } else {
            panic!("error in routing");
        }
    }

    /// Sends message fragments along a predefined route.
    ///
    /// The session is registered in the session manager before sending the fragments.
    /// Each fragment is wrapped in a packet and sent individually.
    ///
    /// # Assumptions
    /// - The `hops` list in `routing_header` is not empty.
    ///
    /// # Panics
    /// - This function may panic if `hops` is unexpectedly empty.
    ///
    /// # Arguments
    /// * `session_id` - The session ID for the message being sent.
    /// * `fragments` - The list of message fragments to send.
    /// * `routing_header` - The routing header specifying the route for the message.
    fn send_fragments(
        &mut self,
        session_id: SessionId,
        fragments: Vec<Fragment>,
        routing_header: SourceRoutingHeader,
    ) {
        self.session_manager.add_session(
            session_id,
            fragments.clone(),
            *routing_header.hops.last().unwrap(),
        ); // assuming hops is not empty

        for fragment in fragments {
            let packet = Packet {
                pack_type: PacketType::MsgFragment(fragment),
                routing_header: routing_header.clone(),
                session_id,
            };
            self.send_packet(packet);
        }
    }

    /// Sends a message to the specified recipient using source routing.
    ///
    /// The message is serialized and split into fragments before being sent.
    ///
    /// # Panics
    /// - If routing to the recipient is not possible, the function will panic.
    ///
    /// # Arguments
    /// * `message` - The message to send.
    /// * `to` - The recipient node ID.
    pub(crate) fn send_message(&mut self, message: Message, to: NodeId) {
        let hops = self
            .network_topology
            .source_routing(self.id, to)
            .expect("Error in routing");
        if !hops.is_empty() {
            if let Message::Server(sb) = message.clone() {
                let serialized_message = self.assembler.serialize_message(message);
                self.controller_send
                    .send(ServerEvent::MessageFragmented(sb))
                    .expect("Error in controller_send");
                let routing_header = SourceRoutingHeader { hop_index: 1, hops };
                let session_id = self.session_manager.get_and_increment_session_id_counter();
                self.send_fragments(session_id, serialized_message, routing_header);
            }
        } else {
            panic!("error in routing");
        }
    }

    /// Sends a packet to the next hop in the routing path.
    ///
    /// This function is responsible for sending a packet to the specified recipient using the
    /// appropriate transport mechanism.
    ///
    /// # Arguments
    /// * `packet` - The packet to send.
    pub(crate) fn send_packet(&self, packet: Packet) {
        // assuming hop index already set at 1
        // assuming the first node connected to the server exists
        if self
            .packet_send
            .contains_key(&packet.routing_header.hops[1])
        {
            self.packet_send[&packet.routing_header.hops[1]]
                .send(packet.clone())
                .expect("Error in send_packet");
            self.controller_send
                .send(ServerEvent::PacketSent(packet))
                .expect("Error in controller_send");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;

    #[test]
    fn test_send_ack() {
        let mut test_server_helper = TestServerHelper::new();

        let to = 6;
        let session_id = 111;
        let fragment: Fragment = TestServerHelper::test_fragment(13, 50);
        test_server_helper.server.send_ack(fragment, to, session_id);

        let ack = test_server_helper
            .packet_recv_3
            .try_recv()
            .expect("Expected recv packet");
        assert_eq!(ack.session_id, session_id);
        match ack.pack_type {
            PacketType::Ack(c) => {
                assert_eq!(c.fragment_index, 13)
            }
            _ => panic!("Expected Ack"),
        }
    }
}
