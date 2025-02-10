use super::Router;
use crate::command::{Command, Event};
use dn_message::Message;
use wg_2024::{
    network::{NodeId, SourceRoutingHeader},
    packet::{Packet, PacketType},
};

impl Router {
    pub(crate) fn handle_command(&mut self, command: Command) {
        match command {
            Command::AddSender(id, sender) => self.routing.add_sender(id, sender),
            Command::RemoveSender(id) => self.routing.remove_sender(id),
            Command::SendMessage(msg, dst) => self.handle_message(msg, dst),
            Command::Return => (),
        }
    }
    pub(crate) fn handle_message(&mut self, msg: Message, dst: NodeId) {
        let session_id = self.inc_session_id();
        let fragments = self.assembler.serialize_message(&msg);
        self.controller_send
            .send(Event::MessageFragmented {
                body: msg,
                from: self.id,
                to: dst,
            })
            .unwrap();
        for fragment in fragments {
            let fragment_index = fragment.fragment_index;
            self.fragment_queue_send
                .send((
                    Packet {
                        routing_header: SourceRoutingHeader {
                            hop_index: 0,
                            hops: Vec::new(),
                        },
                        session_id,
                        pack_type: PacketType::MsgFragment(fragment),
                    },
                    fragment_index,
                    dst,
                ))
                .unwrap();
        }
    }
}
