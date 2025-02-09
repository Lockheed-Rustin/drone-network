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
    pub fn new() -> Self {
        Assembler {
            in_progress_messages: HashMap::new(),
        }
    }

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

    pub fn serialize_message(&self, message: Message) -> Vec<Fragment> {
        let message_data = self.serialize_message_data(&message);
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

    fn serialize_message_data(&self, message: &Message) -> Vec<u8> {
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
    pub fn new(total_n_fragments: usize) -> Self {
        MessageBuffer {
            fragments: vec![0; MAX_FRAGMENT_SIZE * total_n_fragments],
            total_fragments: total_n_fragments as u64,
            received_indices: HashSet::new(),
        }
    }

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

    pub fn is_complete(&self) -> bool {
        self.received_indices.len() == self.total_fragments as usize
    }

    pub fn to_message(&self) -> Message {
        bincode::decode_from_slice(&self.fragments, config::standard())
            .unwrap()
            .0
    }
}
