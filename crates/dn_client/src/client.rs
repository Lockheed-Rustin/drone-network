use crossbeam_channel::{select, Receiver, Sender};
use dn_controller::ClientCommand;
use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;

use wg_2024::{network::NodeId, packet::Packet};
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{Ack, Fragment, Nack, NackType, PacketType};

pub struct Client {
    // TODO: create ClientEvent
    // pub controller_send: Sender<NodeEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,
}

impl Client {
    pub fn run(&mut self) {
        sleep(Duration::from_secs(1));

        let fragment = Fragment {
            fragment_index: 0,
            total_n_fragments: 0,
            length: 0,
            data: [0; 128],
        };
        let packet1 = Packet {
            pack_type: PacketType::MsgFragment(fragment),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![4, 2, 1, 5]
            },
            session_id: 0,
        };

        let ack = Ack {
            fragment_index: 0,
        };
        let packet2 = Packet {
            pack_type: PacketType::Ack(ack),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![4, 2, 1, 5]
            },
            session_id: 0,
        };

        let nack = Nack {
            fragment_index: 0,
            nack_type: NackType::Dropped,
        };
        let packet3 = Packet {
            pack_type: PacketType::Nack(nack),
            routing_header: SourceRoutingHeader {
                hop_index: 1,
                hops: vec![4, 2, 1, 5]
            },
            session_id: 0,
        };


        let sender = self.packet_send.get(&2).unwrap();

        loop {
            sender.send(packet1.clone()).expect("Error in send");
            sleep(Duration::from_secs(1));
            sender.send(packet2.clone()).expect("Error in send");
            sleep(Duration::from_secs(1));
            sender.send(packet3.clone()).expect("Error in send");
            sleep(Duration::from_secs(1));
        }
    }


}
