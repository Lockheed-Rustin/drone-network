//! # Pending Messages Queue
//!
//! This module provides a support structure for the `CommunicationServer` to store messages that
//! could not be sent due to the lack of a known path to the destination node.
//!
//! ## Overview
//! When the server attempts to send a message but does not yet have a routing path to the target node,
//! the message is stored in a queue. Once a valid path is discovered, the queued messages can be retrieved
//! and sent accordingly.

use dn_message::Message;
use std::collections::HashMap;
use wg_2024::network::NodeId;

pub struct PendingMessagesQueue {
    pending_messages: HashMap<NodeId, Vec<Message>>,
}

impl PendingMessagesQueue {
    /// Creates a new empty pending messages queue.
    pub fn new() -> Self {
        Self {
            pending_messages: HashMap::new(),
        }
    }

    /// Adds a message to the queue for a specific node.
    ///
    /// If there are already pending messages for the given `node_id`, the new message
    /// is appended to the existing list. Otherwise, a new entry is created.
    ///
    /// # Arguments
    /// * `node_id` - The destination node ID for which the message is waiting.
    /// * `message` - The message to be queued.
    pub fn add_message(&mut self, node_id: NodeId, message: Message) {
        self.pending_messages
            .entry(node_id)
            .or_default()
            .push(message);
    }

    /// Retrieves and removes all pending messages for a given node.
    ///
    /// This function should be called when a valid path for `node_id` is discovered,
    /// so the stored messages can be sent.
    ///
    /// # Arguments
    /// * `node_id` - The node ID whose pending messages should be retrieved.
    ///
    /// # Returns
    /// A vector of messages if there were pending messages, or `None` if there were none.
    pub fn take_pending_messages(&mut self, node_id: NodeId) -> Option<Vec<Message>> {
        self.pending_messages.remove(&node_id)
    }

    /// Checks if there are pending messages for a given node.
    ///
    /// # Arguments
    /// * `node_id` - The node ID to check.
    ///
    /// # Returns
    /// `true` if there are pending messages for the node, otherwise `false`.
    pub fn has_pending_messages(&self, node_id: NodeId) -> bool {
        self.pending_messages.contains_key(&node_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dn_message::ClientBody;

    /// Helper function to create a dummy message for testing
    fn dummy_message() -> Message {
        Message::Client(ClientBody::ReqServerType)
    }

    #[test]
    fn test_add_message() {
        let mut queue = PendingMessagesQueue::new();
        let node_id = 1;
        let message = dummy_message();

        queue.add_message(node_id, message.clone());

        assert!(queue.has_pending_messages(node_id));
    }

    #[test]
    fn test_take_pending_messages() {
        let mut queue = PendingMessagesQueue::new();
        let node_id = 2;
        let message1 = dummy_message();
        let message2 = dummy_message();

        queue.add_message(node_id, message1.clone());
        queue.add_message(node_id, message2.clone());

        let messages = queue.take_pending_messages(node_id);
        assert!(messages.is_some());
        let messages = messages.unwrap();
        assert_eq!(messages.len(), 2);
        if let Message::Client(ClientBody::ReqServerType) = messages[0].clone() {
            assert!(true);
        } else {
            assert!(false);
        }
        if let Message::Client(ClientBody::ReqServerType) = messages[1].clone() {
            assert!(true);
        } else {
            assert!(false);
        }

        // Ensure messages are removed after being taken
        assert!(!queue.has_pending_messages(node_id));
    }

    #[test]
    fn test_has_pending_messages() {
        let mut queue = PendingMessagesQueue::new();
        let node_id = 3;

        assert!(!queue.has_pending_messages(node_id));

        queue.add_message(node_id, dummy_message());
        assert!(queue.has_pending_messages(node_id));
    }
}
