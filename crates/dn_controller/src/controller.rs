use crate::ClientCommand;
use crossbeam_channel::{Receiver, SendError, Sender};
use dn_topology::Topology;
use std::collections::HashMap;
use wg_2024::packet::Packet;
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

    pub pool: rayon::ThreadPool,
}

impl SimulationController {

    // Drone commands
    pub fn crash_drone(&self, drone_id: NodeId) -> Result<(), String> {
        let sender = self.get_drone_sender(drone_id)?;
        sender.send(DroneCommand::Crash).map_err(|e| e.to_string())
    }

    pub fn set_pdr(&self, drone_id: NodeId, new_pdr: f32) -> Result<(), String> {
        let sender = self.get_drone_sender(drone_id)?;
        sender
            .send(DroneCommand::SetPacketDropRate(new_pdr))
            .map_err(|e| e.to_string())
    }

    fn get_drone_sender(&self, drone_id: NodeId) -> Result<&Sender<DroneCommand>, String> {
        let get_result = self.drones_send.get(&drone_id);
        match get_result {
            None => Err(format!("Error: Drone #{} not found", drone_id)),
            Some(sender) => Ok(sender),
        }
    }

    // Node commands
    pub fn add_link(&self, node_1: NodeId, node_2: NodeId) -> Result<(), String> {
        // TODO: call add_sender twice
        unimplemented!()
    }

    fn add_sender(
        &self,
        target_node_id: NodeId,
        destination_node_id: NodeId,
        sender: Sender<Packet>,
    ) -> Result<(), String> {
        unimplemented!()
    }


}
