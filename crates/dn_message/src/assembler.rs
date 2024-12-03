use crate::Message;
use std::collections::{HashMap, HashSet};
use wg_2024::network::NodeId;
use wg_2024::packet::FRAGMENT_DSIZE as MAX_FRAGMENT_SIZE;
use wg_2024::packet::{Fragment, Packet, PacketType};

/// The `Assembler` struct is responsible for tracking and reassembling fragmented messages.
/// Each message is identified by a unique key consisting of a `(NodeId, session_id)` pair.
pub struct Assembler {
    in_progress_messages: HashMap<(NodeId, u64), MessageBuffer>,
}

impl Assembler {
    pub fn new() -> Self {
        Assembler {
            in_progress_messages: HashMap::new(),
        }
    }

    pub fn handle_packet(&mut self, packet: Packet) -> Option<Message> {
        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                let session_id = packet.session_id;
                let buffer = self
                    .in_progress_messages
                    .entry((packet.routing_header.hops[0], session_id))
                    .or_insert_with(|| MessageBuffer::new(fragment.total_n_fragments as usize));

                buffer.add_fragment(fragment);

                if buffer.is_complete() {
                    let message = buffer.to_message();
                    self.in_progress_messages
                        .remove(&(packet.routing_header.hops[0], session_id));
                    Some(message)
                } else {
                    None
                }
            }
            _ => None, // ignoring other type of packets
        }
    }

    pub fn serialize_message(&self, message: Message, hops: Vec<NodeId>) -> Vec<Fragment> {
        let message_data = self.serialize_message_data(&message);
        let total_fragments =
            ((message_data.len() + MAX_FRAGMENT_SIZE - 1) / MAX_FRAGMENT_SIZE) as u64;

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
        unimplemented!()
    }
}

/// `MessageBuffer` stores a fragmented message as it is reassembled.
/// It holds the fragments, tracks the total number of fragments, and maintains a record of the
/// received fragment indices, ensuring proper reassembly while ignoring duplicates.
struct MessageBuffer {
    fragments: Vec<u8>,
    total_fragments: u64,
    received_indices: HashSet<u64>,
}

impl MessageBuffer {
    fn new(total_n_fragments: usize) -> Self {
        MessageBuffer {
            fragments: vec![0; MAX_FRAGMENT_SIZE * total_n_fragments],
            total_fragments: total_n_fragments as u64,
            received_indices: HashSet::new(),
        }
    }

    fn add_fragment(&mut self, fragment: Fragment) {
        let start_index = MAX_FRAGMENT_SIZE * fragment.fragment_index as usize;
        let end_index = start_index + fragment.length as usize;

        if !self.received_indices.insert(fragment.fragment_index) {
            return; //Ignoring duplicates: assuming the first packet had the correct data
        }

        self.total_fragments = fragment.total_n_fragments;

        self.fragments[start_index..end_index]
            .copy_from_slice(&fragment.data[..fragment.length as usize]);
    }

    fn is_complete(&self) -> bool {
        self.received_indices.len() == self.total_fragments as usize
    }

    fn to_message(&self) -> Message {
        // TODO
        unimplemented!()
    }
}
