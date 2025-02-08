use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use wg_2024::controller::{DroneCommand, DroneEvent};
use wg_2024::drone::Drone;
use wg_2024::network::NodeId;
use wg_2024::packet::Packet;

pub struct DroneOptions {
    pub id: NodeId,
    pub controller_send: Sender<DroneEvent>,
    pub controller_recv: Receiver<DroneCommand>,
    pub packet_recv: Receiver<Packet>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub pdr: f32,
}

pub type DroneF = dyn Fn(DroneOptions) -> Box<dyn Drone>;

pub fn drone_from_opt<D: Drone + 'static>(opt: DroneOptions) -> Box<dyn Drone> {
    Box::new(D::new(
        opt.id,
        opt.controller_send,
        opt.controller_recv,
        opt.packet_recv,
        opt.packet_send,
        opt.pdr,
    ))
}

macro_rules! fair_drones {
    ($($d:path),*) => {
        [
            $(
                Box::new(drone_from_opt::<$d>) as Box<DroneF>,
            )*
        ]
    };
}

pub fn drones_from_opts(opts: Vec<DroneOptions>) -> Vec<Box<dyn Drone>> {
    let drones_fn = fair_drones!(
        bagel_bomber::BagelBomber,
        flypath::FlyPath,
        fungi_drone::FungiDrone,
        rust_do_it::RustDoIt,
        rust_roveri::RustRoveri,
        rustastic_drone::RustasticDrone,
        rustbusters_drone::RustBustersDrone,
        rusty_drones::RustyDrone,
        skylink::SkyLinkDrone,
        ledron_james::Drone
    );
    opts.into_iter()
        .enumerate()
        .map(|(i, opt)| {
            let drone_fn = &drones_fn[i % drones_fn.len()];
            drone_fn(opt)
        })
        .collect()
}
