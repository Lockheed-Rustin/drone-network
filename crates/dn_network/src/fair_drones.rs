use crossbeam_channel::{Receiver, Sender};
use rand::rng;
use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::marker::PhantomData;
use wg_2024::controller::{DroneCommand, DroneEvent};
use wg_2024::drone::Drone;
use wg_2024::network::NodeId;
use wg_2024::packet::Packet;

pub trait FairDrone {
    fn drone(&self, opt: DroneOptions) -> Box<dyn Drone>;
    fn group_name(&self) -> &str;
}

pub struct DroneOptions {
    pub id: NodeId,
    pub controller_send: Sender<DroneEvent>,
    pub controller_recv: Receiver<DroneCommand>,
    pub packet_recv: Receiver<Packet>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub pdr: f32,
}

struct FD<D: Drone> {
    group_name: String,
    marker: PhantomData<D>,
}

impl<D: Drone + 'static> FairDrone for FD<D> {
    fn drone(&self, opt: DroneOptions) -> Box<dyn Drone> {
        Box::new(D::new(
            opt.id,
            opt.controller_send,
            opt.controller_recv,
            opt.packet_recv,
            opt.packet_send,
            opt.pdr,
        ))
    }
    fn group_name(&self) -> &str {
        &self.group_name
    }
}

pub struct FairDrones(Vec<Box<dyn FairDrone>>);

impl FairDrones {
    pub fn get(&self, i: usize) -> &dyn FairDrone {
        &*self.0[i % self.0.len()]
    }
}

macro_rules! fair_drones {
    ($($d:ident :: $($p:ident)::+, )*) => {{
        let mut drones = Vec::from([
            $(
                Box::new(FD::<$d$(::$p)*>{
                    group_name: String::from(stringify!($d)),
                    marker: PhantomData,
                }) as Box<dyn FairDrone>,
            )*
        ]);
        drones.shuffle(&mut rng());
        FairDrones(drones)
    }};
}

pub fn fair_drones() -> FairDrones {
    fair_drones!(
        bagel_bomber::BagelBomber,
        flypath::FlyPath,
        fungi_drone::FungiDrone,
        rust_do_it::RustDoIt,
        rust_roveri::RustRoveri,
        rustastic_drone::RustasticDrone,
        rustbusters_drone::RustBustersDrone,
        rusty_drones::RustyDrone,
        skylink::SkyLinkDrone,
        ledron_james::Drone,
    )
}

pub fn fair_drones_adapter<D: Drone + 'static>(group_name: String) -> FairDrones {
    FairDrones(Vec::from([Box::new(FD::<D> {
        group_name,
        marker: PhantomData,
    }) as Box<dyn FairDrone>]))
}
