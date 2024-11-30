use crate::ClientCommand;
use crossbeam_channel::{Receiver, Sender};
use dn_topology::Topology;
use std::{collections::HashMap, thread::JoinHandle};
use wg_2024::{
    controller::{DroneCommand, NodeEvent},
    network::NodeId,
};

pub struct SimulationController {
    pub drones_send: HashMap<NodeId, Sender<DroneCommand>>,
    pub clients_send: HashMap<NodeId, Sender<ClientCommand>>,
    pub server_ids: Vec<NodeId>,

    pub node_recv: Receiver<NodeEvent>,

    pub topology: Topology,

    pub handles: Vec<JoinHandle<()>>,
}

impl SimulationController {
    pub fn crash_drone(&self, drone_id: NodeId) {
        self.drones_send[&drone_id]
            .send(DroneCommand::Crash)
            .unwrap();
    }
}
