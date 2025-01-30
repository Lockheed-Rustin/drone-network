use petgraph::graphmap::UnGraphMap;
use std::collections::{HashMap, HashSet};
use wg_2024::{
    network::{NodeId, SourceRoutingHeader},
    packet::{Fragment, NodeType, Packet, PacketType, FRAGMENT_DSIZE},
};

// TODO: impl a generic host
// use this host inside specialized hosts

pub type Topology = UnGraphMap<NodeId, ()>;

pub struct BaseTopology<N> {
    topology: Topology,
    nodes: HashMap<NodeId, N>,
}

pub trait Host {
    fn send_packet(&self, packet: Packet);
}

pub trait Router {
    fn run(&self);
    fn send_bytes(&mut self, session_id: u64, bytes: Vec<u8>);
    fn ack_fragment(&mut self, session_id: u64, fragment_index: u64);
    fn nack_fragment(&mut self, session_id: u64, fragment_index: u64, id: NodeId);
    fn update_topology(&mut self, topology: BaseTopology<NodeType>);
}

struct Buffer {
    acked: HashSet<u64>,
    bytes: Vec<u8>,
}

impl Buffer {
    fn compleate(&self) -> bool {
        self.acked.len() == self.bytes.len().div_ceil(FRAGMENT_DSIZE)
    }
}

const ALPHA: f32 = 0.1;

pub struct SimpleRouter<'a, H: Host> {
    topology: BaseTopology<(NodeType, f32)>,
    host: &'a H,
    buffers: HashMap<u64, Buffer>,
}

impl<'a, H: Host> Router for SimpleRouter<'a, H> {
    fn run(&self) {
        // TODO: add this in a rayon thread
        // doing so would need the use of a mutex
        loop {
            for (_, buffer) in self.buffers.iter() {}
        }
    }

    fn send_bytes(&mut self, session_id: u64, bytes: Vec<u8>) {
        let acked = HashSet::new();
        self.buffers.insert(session_id, Buffer { acked, bytes });
    }

    fn ack_fragment(&mut self, session_id: u64, fragment_index: u64) {
        self.buffers
            .get_mut(&session_id)
            .unwrap()
            .acked
            .insert(fragment_index);
        if self.buffers[&session_id].compleate() {
            self.buffers.remove(&session_id);
        }
    }

    fn nack_fragment(&mut self, session_id: u64, fragment_index: u64, id: NodeId) {
        self.buffers
            .get_mut(&session_id)
            .unwrap()
            .acked
            .remove(&fragment_index);
        self.topology
            .nodes
            .entry(id)
            .and_modify(|(_, pdr)| *pdr = ALPHA + (1.0 - ALPHA) * *pdr);
    }

    fn update_topology(&mut self, topology: BaseTopology<NodeType>) {
        self.topology.nodes = topology
            .nodes
            .into_iter()
            .map(|(id, nt)| {
                (
                    id,
                    (
                        nt,
                        self.topology.nodes.get(&id).map_or(0.0, |&(_, pdr)| pdr),
                    ),
                )
            })
            .collect();
        self.topology.topology = topology.topology;
    }
}
