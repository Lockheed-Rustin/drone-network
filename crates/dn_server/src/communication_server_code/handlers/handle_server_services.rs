//! This module defines the core service functionalities of the `CommunicationServer`.
//!
//! ### Functions:
//! - **`send_server_type`**: sends the type of the server to the specified client.
//! - **`register_client`**: registers a client by adding its ID to the list of registered clients.
//! - **`registered_clients_list`**: sends a list of all registered clients to the requesting client.
//! - **`forward_message`**: forwards a communication message to the intended recipient if they are registered.

use crate::communication_server_code::communication_server::CommunicationServer;
use dn_message::ServerBody::RespServerType;
use dn_message::ServerCommunicationBody::RespClientList;
use dn_message::{ClientBody, ClientCommunicationBody, CommunicationMessage, Message, ServerBody, ServerCommunicationBody, ServerType};
use wg_2024::network::NodeId;

impl CommunicationServer {

    pub(crate) fn handler_client_body(&mut self, client_body: ClientBody, sender_id: NodeId) {
        match client_body {
            ClientBody::ReqServerType => {
                self.send_server_type(sender_id);
            }
            ClientBody::ClientCommunication(comm_body) => {
                self.handle_client_communication_body(comm_body, sender_id);
            },
            ClientBody::ClientContent(_) => {} // ignoring messages for the content server
        }
    }

    fn handle_client_communication_body(&mut self, client_communication_body: ClientCommunicationBody, sender_id: NodeId) {
        match client_communication_body {
            ClientCommunicationBody::ReqRegistrationToChat => {
                self.register_client(sender_id);
            }
            ClientCommunicationBody::MessageSend(comm_message) => {
                self.forward_message(comm_message);
            }
            ClientCommunicationBody::ReqClientList => {
                self.registered_clients_list(sender_id);
            }
        }
    }

    /// Sends the type of the server to the specified client.
    ///
    /// This function informs the given client that the type of the server is `Communication`.
    ///
    /// ### Arguments:
    /// - `client_id`: The unique identifier of the client to which the server type will be sent.
    pub(crate) fn send_server_type(&mut self, client_id: NodeId) {
        let message = Message::Server(RespServerType(ServerType::Communication));
        self.send_message(message, client_id);
    }

    /// Registers a client by adding its ID to the list of registered clients.
    ///
    /// This function registers a client, which allows the server to keep track of the clients that
    /// have connected.
    /// The client ID is inserted into the internal collection of registered clients, making it
    /// available for further communication and message forwarding.
    ///
    /// ### Arguments:
    /// - `client_id`: The unique identifier of the client to be registered.
    fn register_client(&mut self, client_id: NodeId) {
        self.registered_clients.insert(client_id);
    }

    /// Sends a list of all registered clients to the requesting client.
    ///
    /// This function sends a message containing the list of all clients that are currently
    /// registered with the server.
    /// The list is sent to the `client_id` specified.
    ///
    /// ### Arguments:
    /// - `client_id`: The unique identifier of the client who has requested the list of registered clients.
    fn registered_clients_list(&mut self, client_id: NodeId) {
        let client_list: Vec<NodeId> = self.registered_clients.iter().cloned().collect();
        let message = Message::Server(ServerBody::ServerCommunication(RespClientList(client_list)));
        self.send_message(message, client_id);
    }

    /// Forwards a communication message to the intended recipient if they are registered.
    ///
    /// This function checks whether the recipient of the communication message is a registered client.
    /// If the recipient is registered, the server forwards the message to the recipient.
    /// If the recipient is not registered, an error message indicating that the client ID is
    /// incorrect is sent back to the sender.
    ///
    /// ### Arguments:
    /// - `communication_message`: The message containing the details of the communication,
    ///   including the sender (`from`), the recipient (`to`), and the actual content of the message.
    fn forward_message(&mut self, communication_message: CommunicationMessage) {
        let to = communication_message.to;
        if self.registered_clients.contains(&to) {
            let message: Message = Message::Server(ServerBody::ServerCommunication(
                ServerCommunicationBody::MessageReceive(communication_message),
            ));
            self.send_message(message.clone(), to);
        } else {
            let message: Message = Message::Server(ServerBody::ServerCommunication(
                ServerCommunicationBody::ErrWrongClientId,
            ));
            self.send_message(message.clone(), communication_message.from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::communication_server_code::test_server_helper::TestServerHelper;
    use dn_message::ClientBody::{ClientCommunication, ReqServerType};
    use dn_message::ServerBody::ServerCommunication;
    use dn_message::ServerCommunicationBody::MessageReceive;
    use dn_message::{ClientCommunicationBody, Message};

    #[test]
    fn test_send_server_type() {
        let mut test_server_helper = TestServerHelper::new();
        let response = test_server_helper.send_message_and_get_response(
            Message::Client(ReqServerType),
            vec![6, 3, 1],
            3,
        );

        match response {
            Message::Server(RespServerType(st)) => {
                assert_eq!(st, ServerType::Communication);
            }
            _ => panic!("Expected ServerMessage"),
        }
    }

    #[test]
    fn test_register_client() {
        let mut test_server_helper = TestServerHelper::new();

        assert!(!test_server_helper.server.registered_clients.contains(&6));

        test_server_helper.register_client_6();

        assert!(test_server_helper.server.registered_clients.contains(&6));
    }

    #[test]
    fn test_registered_client_list() {
        let mut test_server_helper = TestServerHelper::new();
        test_server_helper.register_client_6();
        let response = test_server_helper.send_message_and_get_response(
            Message::Client(ClientCommunication(ClientCommunicationBody::ReqClientList)),
            vec![6, 3, 1],
            3,
        );
        if let Message::Server(ServerCommunication(RespClientList(list))) = response {
            assert_eq!(list.len(), 1);
            assert_eq!(list[0], 6);
        }
    }

    #[test]
    fn test_forward_message() {
        let mut test_server_helper = TestServerHelper::new();
        let message = Message::Client(ClientCommunication(ClientCommunicationBody::MessageSend(
            CommunicationMessage {
                from: 5,
                to: 6,
                message: "I wanted to say hi!".to_string(),
            },
        )));

        let response =
            test_server_helper.send_message_and_get_response(message.clone(), vec![5, 1], 5);
        if let Message::Server(ServerCommunication(ServerCommunicationBody::ErrWrongClientId)) =
            response
        {
            assert!(true);
        } else {
            assert!(false);
        }

        test_server_helper.register_client_6();
        // the message is sent from 5 to 1. The dest is 6 so we expect the server to send the fragments to node 3.
        // This call reconstruct the response in node 3.
        let response = test_server_helper.send_message_and_get_response(message, vec![5, 1], 3);
        if let Message::Server(ServerCommunication(MessageReceive(cm))) = response {
            assert_eq!(cm.from, 5);
            assert_eq!(cm.to, 6);
            assert_eq!(cm.message, "I wanted to say hi!");
        }
    }
}
