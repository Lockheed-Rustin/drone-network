use crate::command::{Command, Event};
use crate::routing::{Routing, RoutingOptions};
use crossbeam_channel::{select_biased, unbounded, Receiver, Sender};
use dn_message::assembler::Assembler;
use std::collections::HashMap;
use wg_2024::network::NodeId;
use wg_2024::packet::{NodeType, Packet};

#[derive(Clone)]
pub struct RouterOptions {
    pub id: NodeId,
    pub node_type: NodeType,
    pub controller_recv: Receiver<Command>,
    pub controller_send: Sender<Event>,
    pub packet_recv: Receiver<Packet>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
}

pub struct Router {
    pub(crate) id: NodeId,

    pub(crate) controller_recv: Receiver<Command>,
    pub(crate) controller_send: Sender<Event>,

    pub(crate) packet_recv: Receiver<Packet>,

    pub(crate) fragment_queue_send: Sender<(Packet, u64, NodeId)>,
    pub(crate) fragment_queue_recv: Receiver<(Packet, u64, NodeId)>,

    pub(crate) routing: Routing,
    pub(crate) assembler: Assembler,

    pub(crate) session_id: u64,
    pub(crate) drop_count: u64,
}

impl Router {
    pub fn new(opt: RouterOptions) -> Self {
        let (fragment_queue_send, fragment_queue_recv) = unbounded();
        Self {
            id: opt.id,
            controller_recv: opt.controller_recv,
            controller_send: opt.controller_send.clone(),
            packet_recv: opt.packet_recv,
            fragment_queue_send,
            fragment_queue_recv,
            routing: Routing::new(RoutingOptions {
                id: opt.id,
                node_type: opt.node_type,
                packet_send: opt.packet_send,
                controller_send: opt.controller_send,
            }),
            assembler: Assembler::new(),
            session_id: 0,
            drop_count: 0,
        }
    }

    pub fn run(&mut self) {
        self.flood();
        loop {
            select_biased! {
                recv(self.controller_recv) -> command => {
                    if let Ok(command) = command {
                        self.handle_command(command);
                    } else {
                        return;
                    }
                },
                recv(self.packet_recv) -> packet => {
                    if let Ok(packet) = packet {
                        self.handle_packet(packet);
                    }
                },
                recv(self.fragment_queue_recv) -> fragment => {
                    if let Ok((fragment, fragment_index, dst)) = fragment {
                        self.send_fragment(fragment, fragment_index, dst);
                    }
                },
            }
        }
    }

    pub(crate) fn inc_session_id(&mut self) -> u64 {
        let session_id = self.session_id;
        self.session_id += 1;
        session_id
    }

    pub(crate) fn send_fragment(&mut self, fragment: Packet, fragment_index: u64, dst: NodeId) {
        let sent = self.routing.send_fragment(fragment, fragment_index, dst);
        if !sent {
            self.flood();
        }
    }
}
