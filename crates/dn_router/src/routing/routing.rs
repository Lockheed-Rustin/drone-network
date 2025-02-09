use crate::command::Event;
use crossbeam_channel::Sender;
use petgraph::{algo, prelude::UnGraphMap, visit::EdgeRef};
use std::collections::HashMap;
use wg_2024::{
    network::{NodeId, SourceRoutingHeader},
    packet::{FloodRequest, NodeType, Packet, PacketType},
};

pub const DEFAULT_PDR: f32 = 0.1;
pub const ALPHA: f32 = 0.1;

pub type Topology = UnGraphMap<NodeId, ()>;

#[derive(Clone)]
pub struct RoutingOptions {
    pub id: NodeId,
    pub node_type: NodeType,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub controller_send: Sender<Event>,
}

pub struct Routing {
    id: NodeId,
    node_type: NodeType,

    topology: Topology,
    estimated_pdr: HashMap<NodeId, f32>,
    node_types: HashMap<NodeId, NodeType>,

    packet_send: HashMap<NodeId, Sender<Packet>>,
    controller_send: Sender<Event>,

    /// the key is (session_id, fragment_indext)
    pending_ack: HashMap<(u64, u64), (Packet, NodeId)>,
    /// the key is (session_id, fragment_indext)
    pending_path: HashMap<(u64, u64), (Packet, NodeId)>,
}

impl Routing {
    pub fn new(opt: RoutingOptions) -> Self {
        let mut topology = Topology::new();
        topology.add_node(opt.id);
        Self {
            id: opt.id,
            node_type: opt.node_type,
            topology,
            estimated_pdr: HashMap::new(),
            node_types: HashMap::new(),
            packet_send: opt.packet_send,
            controller_send: opt.controller_send,
            pending_ack: HashMap::new(),
            pending_path: HashMap::new(),
        }
    }

    pub fn send_fragment(&mut self, mut packet: Packet, fragment_index: u64, dst: NodeId) -> bool {
        let path = algo::astar(
            &self.topology,
            self.id,
            |n| n == dst,
            |e| {
                let id = e.source();
                if let NodeType::Drone = self.node_types[&id] {
                    self.estimated_pdr
                        .get(&e.source())
                        .cloned()
                        .unwrap_or(DEFAULT_PDR)
                } else {
                    f32::INFINITY
                }
            },
            |_| 0.0,
        );
        let session_id = packet.session_id;
        match path {
            Some((_, hops)) => {
                packet.routing_header = SourceRoutingHeader { hop_index: 1, hops };
                self.send_packet(packet.clone());
                self.pending_ack
                    .insert((session_id, fragment_index), (packet, dst));
                true
            }
            None => {
                self.pending_path
                    .insert((session_id, fragment_index), (packet, dst));
                false
            }
        }
    }

    pub fn send_flood_response(&self, flood_response: Packet) {
        let next_hop = flood_response.routing_header.hops[1];
        if self.packet_send.contains_key(&next_hop) {
            self.send_packet(flood_response);
        }
    }

    pub fn send_flood_request(&self, session_id: u64) {
        let flood_request = Packet {
            routing_header: SourceRoutingHeader {
                hop_index: 0,
                hops: Vec::new(),
            },
            session_id,
            pack_type: PacketType::FloodRequest(FloodRequest {
                flood_id: 0,
                initiator_id: 0,
                path_trace: vec![(self.id, self.node_type)],
            }),
        };
        for (_, sender) in self.packet_send.iter() {
            sender.send(flood_request.clone()).unwrap();
            self.controller_send
                .send(Event::PacketSent(flood_request.clone()))
                .unwrap();
        }
    }

    pub fn send_packet(&self, mut packet: Packet) {
        let next_hop = packet.routing_header.hops[1];
        packet.routing_header.hop_index += 1;
        self.packet_send[&next_hop].send(packet.clone()).unwrap();
        self.controller_send
            .send(Event::PacketSent(packet))
            .unwrap();
    }

    pub fn add_sender(&mut self, id: NodeId, sender: Sender<Packet>) {
        self.packet_send.insert(id, sender);
        self.topology.add_edge(self.id, id, ());
    }

    pub fn remove_sender(&mut self, id: NodeId) {
        self.crash_node(id);
    }

    pub fn crash_node(&mut self, id: NodeId) {
        self.packet_send.remove(&id);
        self.topology.remove_node(id);
    }

    pub fn update_estimated_pdr(&mut self, id: NodeId, dropped: bool) {
        let pdr = self.estimated_pdr.entry(id).or_insert(DEFAULT_PDR);
        *pdr = if dropped { ALPHA } else { 0.0 } + (1.0 - ALPHA) * *pdr;
    }

    pub fn add_path(&mut self, path: Vec<(NodeId, NodeType)>) {
        for (id, node_type) in path.iter().cloned() {
            self.node_types.insert(id, node_type);
        }
        let mut updated = false;
        for w in path.windows(2) {
            if self.topology.add_edge(w[0].0, w[1].0, ()).is_none() {
                updated = true;
            }
        }
        if updated {
            self.send_pending();
        }
    }

    pub fn send_pending(&mut self) {
        let pending = self.pending_path.drain().collect::<HashMap<_, _>>();
        for ((_, fragment_index), (packet, dst)) in pending {
            self.send_fragment(packet, fragment_index, dst);
        }
    }

    pub fn ack(&mut self, packet: Packet, fragment_index: u64) {
        self.pending_ack
            .remove(&(packet.session_id, fragment_index));
        for hop in packet.routing_header.hops {
            self.update_estimated_pdr(hop, false);
        }
    }

    pub fn nack(&mut self, session_id: u64, fragment_index: u64) {
        if let Some((fragment, dst)) = self.pending_ack.remove(&(session_id, fragment_index)) {
            self.send_fragment(fragment, fragment_index, dst);
        }
    }
}
