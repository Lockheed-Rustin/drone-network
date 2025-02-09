use crate::Message;
use bincode::config;
use std::collections::{HashMap, HashSet};
use wg_2024::network::NodeId;
use wg_2024::packet::Fragment;
use wg_2024::packet::FRAGMENT_DSIZE as MAX_FRAGMENT_SIZE;

/// The `Assembler` struct is responsible for tracking and reassembling fragmented messages.
/// Each message is identified by a unique key consisting of a `(NodeId, session_id)` pair.
#[derive(Default)]
pub struct Assembler {
    in_progress_messages: HashMap<(NodeId, u64), MessageBuffer>,
}

impl Assembler {
    /// Creates a new `Assembler` instance.
    ///
    /// This function initializes the `Assembler` with an empty map to track in-progress messages.
    ///
    /// # Returns
    /// A new `Assembler` instance.
    #[must_use]
    pub fn new() -> Self {
        Assembler {
            in_progress_messages: HashMap::new(),
        }
    }

    /// Handles an incoming message fragment, adding it to the corresponding message buffer.
    /// If the message is complete, it returns the reassembled `Message`.
    ///
    /// # Arguments
    /// - `fragment`: A reference to the incoming fragment.
    /// - `sender_id`: The `NodeId` of the sender.
    /// - `session_id`: The session ID associated with the message.
    ///
    /// # Returns
    /// - `Some(Message)`: If the message has been fully reassembled, it returns the `Message`.
    /// - `None`: If the message is incomplete, it returns `None`.
    pub fn handle_fragment(
        &mut self,
        fragment: &Fragment,
        sender_id: NodeId,
        session_id: u64,
    ) -> Option<Message> {
        let buffer = self
            .in_progress_messages
            .entry((sender_id, session_id))
            .or_insert_with(|| MessageBuffer::new(fragment.total_n_fragments as usize));

        buffer.add_fragment(fragment);

        if buffer.is_complete() {
            let message = buffer.to_message();
            self.in_progress_messages.remove(&(sender_id, session_id));
            Some(message)
        } else {
            None
        }
    }

    /// Serializes a message into a vector of fragments.
    ///
    /// This function splits the message into fragments, each of which contains part of the message data.
    ///
    /// # Arguments
    /// - `message`: A reference to the `Message` to be serialized.
    ///
    /// # Returns
    /// A vector of fragments (`Vec<Fragment>`), each representing a part of the original message.
    #[must_use]
    pub fn serialize_message(&self, message: &Message) -> Vec<Fragment> {
        let message_data = Assembler::serialize_message_data(message);
        let total_fragments = message_data.len().div_ceil(MAX_FRAGMENT_SIZE) as u64;

        let mut fragments = Vec::new();

        for (i, chunk) in message_data.chunks(MAX_FRAGMENT_SIZE).enumerate() {
            let mut data = [0u8; MAX_FRAGMENT_SIZE];
            data[..chunk.len()].copy_from_slice(chunk);
            let fragment = Fragment {
                fragment_index: i as u64,
                total_n_fragments: total_fragments,
                length: chunk.len() as u8,
                data,
            };

            fragments.push(fragment);
        }

        fragments
    }

    /// Serializes the message data into a `Vec<u8>`.
    ///
    /// This function uses `bincode` to encode the message into a binary format.
    ///
    /// # Arguments
    /// - `message`: A reference to the `Message` to be serialized.
    ///
    /// # Returns
    /// A `Vec<u8>` representing the serialized message.
    fn serialize_message_data(message: &Message) -> Vec<u8> {
        bincode::encode_to_vec(message, config::standard()).unwrap()
    }
}

/// `MessageBuffer` stores a fragmented message as it is reassembled.
/// It holds the fragments, tracks the total number of fragments, and maintains a record of the
/// received fragment indices, ensuring proper reassembly while ignoring duplicates.

pub struct MessageBuffer {
    fragments: Vec<u8>,
    total_fragments: u64,
    received_indices: HashSet<u64>,
}

impl MessageBuffer {
    /// Creates a new `MessageBuffer` with the specified total number of fragments.
    ///
    /// This function initializes a buffer with enough space to store all the fragments of the message.
    ///
    /// # Arguments
    /// - `total_n_fragments`: The total number of fragments the message will have.
    ///
    /// # Returns
    /// A new `MessageBuffer` instance.
    #[must_use]
    pub fn new(total_n_fragments: usize) -> Self {
        MessageBuffer {
            fragments: vec![0; MAX_FRAGMENT_SIZE * total_n_fragments],
            total_fragments: total_n_fragments as u64,
            received_indices: HashSet::new(),
        }
    }

    /// Adds a fragment to the `MessageBuffer`.
    ///
    /// This function inserts the fragment data into the appropriate position in the buffer and
    /// tracks the received fragment indices to ensure proper reassembly.
    ///
    /// # Arguments
    /// - `fragment`: A reference to the incoming fragment.
    pub fn add_fragment(&mut self, fragment: &Fragment) {
        let start_index = MAX_FRAGMENT_SIZE * fragment.fragment_index as usize;
        let end_index = start_index + fragment.length as usize;

        if !self.received_indices.insert(fragment.fragment_index) {
            return; //Ignoring duplicates: assuming the first packet had the correct data
        }

        self.total_fragments = fragment.total_n_fragments;

        self.fragments[start_index..end_index]
            .copy_from_slice(&fragment.data[..fragment.length as usize]);
    }

    /// Checks if the message is complete by verifying that all fragments have been received.
    ///
    /// # Returns
    /// - `true`: If all fragments have been received.
    /// - `false`: If any fragments are missing.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.received_indices.len() == self.total_fragments as usize
    }

    /// Converts the current vector of u8 into a `Message`.
    ///
    /// This function decodes the stored `fragments` using `bincode` with
    /// a standard configuration. If decoding fails, it will panic.
    ///
    /// # Returns
    /// A `Message` object reconstructed from the serialized data.
    ///
    /// # Panics
    /// This function panics if the decoding process fails.
    #[must_use]
    pub fn to_message(&self) -> Message {
        bincode::decode_from_slice(&self.fragments, config::standard())
            .unwrap()
            .0
    }
}
