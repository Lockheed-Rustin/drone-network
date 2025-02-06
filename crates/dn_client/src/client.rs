use crossbeam_channel::{select_biased, Receiver, Sender};
use dn_controller::{ClientCommand, ClientEvent};
use dn_message::{Assembler, ClientBody, Message};
use std::collections::{HashMap};
use wg_2024::network::SourceRoutingHeader;
use wg_2024::packet::{Ack, FloodRequest, FloodResponse, Fragment, Nack, NackType, NodeType, PacketType};
use wg_2024::{network::NodeId, packet::Packet};
use crate::ClientRouting;



//---------- CUSTOM TYPES ----------//
type PendingFragments = HashMap<u64, Fragment>;

pub struct Client {
    pub id: NodeId,
    pub controller_send: Sender<ClientEvent>,
    pub controller_recv: Receiver<ClientCommand>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
    pub packet_recv: Receiver<Packet>,

    pub session_id: u64,
    pub flood_id: u64,
    pub assembler: Assembler,
    pub source_routing: ClientRouting,

    //NOTE: assembler manages incoming messages and rebuild them;
    //      following structures manage sent messages
    pending_sessions: HashMap<u64, (NodeId, PendingFragments)>, // (dest, session_id) -> (fragment_index -> fragment)
    unsendable_fragments: HashMap<NodeId, Vec<(u64, Fragment)>>, // dest -> Vec<(session_id, fragment)>
}

impl Client {
    pub fn new(
        id: NodeId,
        controller_send: Sender<ClientEvent>,
        controller_recv: Receiver<ClientCommand>,
        packet_send: HashMap<NodeId, Sender<Packet>>,
        packet_recv: Receiver<Packet>,
    ) -> Self{
        let mut source_routing= ClientRouting::new(id);

        for neighbor in packet_send.keys() {
            source_routing.add_channel_to_neighbor(*neighbor);
        }

        Self {
            id,
            controller_send,
            controller_recv,
            packet_send,
            packet_recv,
            session_id: 0,
            flood_id: 0,
            assembler: Assembler::new(),
            source_routing,
            pending_sessions: HashMap::new(),
            unsendable_fragments: HashMap::new(),
        }
    }

    pub fn run(&mut self) {
        self.send_flood_request();

        loop {
            select_biased! {
                recv(self.controller_recv) -> command => {
                    if let Ok(cmd) = command {
                        match cmd {
                            ClientCommand::Return => {return;},
                             _ => self.handle_command(cmd),
                        }
                    }
                },
                recv(self.packet_recv) -> packet => {
                    if let Ok(pckt) = packet {
                        self.handle_packet(pckt);
                    }
                }
            }
        }
    }


    //---------- handle receiver ----------//
    fn handle_command(&mut self, command: ClientCommand) {
        match command {
            ClientCommand::SendMessage(client_body, to) => self.send_message(client_body, to),
            ClientCommand::SendFloodRequest => self.send_flood_request(),
            ClientCommand::RemoveSender(n) => self.remove_sender(n),
            ClientCommand::AddSender(n, sender) => self.add_sender(n, sender),
            _ => {},
        }
    }

    fn handle_packet(&mut self, packet: Packet) {
        //notify controller about receiving packet
        self.controller_send
            .send(ClientEvent::PacketReceived(packet.clone(), self.id))
            .expect("Error in controller_send");

        if !self.is_valid_packet(&packet) { //packet is only traveling through me
            return;
        }

        match packet.pack_type {
            PacketType::MsgFragment(fragment) => {
                self.handle_fragment(&fragment, &packet.routing_header, packet.session_id);
            }

            PacketType::Ack(ack) => {
                self.handle_ack(&ack, &packet.routing_header, packet.session_id);
            }

            PacketType::Nack(nack) => {
                self.handle_nack(&nack, &packet.routing_header, packet.session_id)

            }

            PacketType::FloodRequest(flood_request) => {
                self.handle_flood_request(packet.session_id, flood_request);
            }

            PacketType::FloodResponse(flood_response) => {

                self.handle_flood_response(&flood_response);
            }
        }
    }


    //---------- check routing ----------//
    fn is_valid_packet(&self, packet: &Packet) -> bool {
        match packet.pack_type {
            PacketType::FloodRequest(_) => {
                true
            }
            PacketType::MsgFragment(_) => {
                if packet.routing_header.len() < 2 {
                    return false;
                }

                match packet.routing_header.current_hop() {
                    Some(curr_hop) if curr_hop == self.id => {
                        packet.routing_header.is_last_hop()
                    }
                    Some(_) => {
                        let mut path = packet.routing_header
                            .hops
                            .iter()
                            .cloned()
                            .take(packet.routing_header.hop_index)
                            .rev()
                            .collect::<Vec<_>>();
                        path.insert(0,self.id);

                        let nack = Packet::new_nack(
                            SourceRoutingHeader{hop_index: 0, hops: path},
                            packet.session_id,
                            Nack{
                                fragment_index: packet.get_fragment_index(),
                                nack_type: NackType::UnexpectedRecipient(self.id),
                            }
                        );

                        self.send_packet(nack);

                        false
                    }
                    _ => {false}
                }
            }
            _ => {
                packet.routing_header.len() >= 2
            }
        }
    }


    //---------- add/rmv sender from client ----------//
    fn remove_sender(&mut self, n: NodeId) {
        self.packet_send.remove(&n);
        self.source_routing.remove_channel_to_neighbor(n);
    }

    fn add_sender(&mut self, n: NodeId, sender: Sender<Packet>) {
        if self.packet_send.contains_key(&n) || self.packet_send.len() < 2 { //client can't have more than 2 senders
            self.packet_send.insert(n, sender);

            if let Some(servers_became_reachable) = self.source_routing.add_channel_to_neighbor(n) {
                self.send_unsended(servers_became_reachable);
            }
        }
    }


    //---------- send ----------//
    fn send_unsended(&mut self, servers: Vec<(NodeId, Vec<NodeId>)>) {
        for (server, path) in servers {
            if path.len() >= 2 {
                if let Some(unsendeds) = self.unsendable_fragments.remove(&server) {
                    let header = SourceRoutingHeader::initialize(path);
                    for (session, fragment) in unsendeds {
                        let packet = Packet::new_fragment(header.clone(), session, fragment);

                        self.send_packet(packet);
                    }
                }
            }
        }
    }

    fn send_message(&mut self, client_body: ClientBody, dest: NodeId) {
        //fragment message and notify controller
        let fragments = self.assembler.serialize_message(&Message::Client(client_body.clone()));

        self.controller_send
            .send(ClientEvent::MessageFragmented{
                body: client_body,
                from: self.id,
                to: dest,
            })
            .expect("Error in controller_send");

        //add fragments to pending session
        let mut pending_fragment: PendingFragments = HashMap::new();

        for fragment in fragments.iter() {
                pending_fragment.insert(fragment.fragment_index, fragment.clone());
        }

        self.pending_sessions.insert(self.session_id, (dest, pending_fragment));

        let mut pkt_not_sended = false;
        for fragment in fragments {
            if !self.send_fragment(dest, fragment) {
                pkt_not_sended = true;
            }
        }
        if pkt_not_sended {
            self.send_flood_request();
        }

        self.session_id += 1;
    }

    fn send_flood_request(&mut self) {
        let flood_request_packet = Packet::new_flood_request(
            SourceRoutingHeader::empty_route(),
            self.session_id,
            FloodRequest::initialize(self.flood_id, self.id, NodeType::Client)
        );
        self.flood_id += 1;
        self.session_id += 1;

        for (_, sender) in self.packet_send.iter() {
            sender
                .send(flood_request_packet.clone())
                .expect("Error in send");
        }

        self.controller_send
            .send(ClientEvent::PacketSent(flood_request_packet))
            .expect("Error in controller_send");

        self.source_routing.clear_topology();
    }

    fn send_fragment(&mut self, dest: NodeId, fragment: Fragment) -> bool {
        if let Some(path) = self.source_routing.get_path(dest) {
            let packet= Packet::new_fragment(SourceRoutingHeader::initialize(path.clone()), self.session_id, fragment);
            self.send_packet(packet);

            true
        }
        else {
            let unsendeds = self.unsendable_fragments.entry(dest).or_default();
            unsendeds.push((self.session_id, fragment));

            false
        }
    }

    fn send_packet(&self, mut packet: Packet) {
        if let Some(next_hop) = packet.routing_header.next_hop() {
            packet.routing_header.increase_hop_index();

            self.packet_send
                .get(&next_hop)
                .unwrap()
                .send(packet.clone())
                .expect("Error in send");


            self.controller_send
                .send(ClientEvent::PacketSent(packet))
                .expect("Error in controller_send");
        }
    }


    //---------- handle ----------//

    fn handle_fragment(&mut self, fragment: &Fragment, header: &SourceRoutingHeader, session_id: u64) {
        if header.hops.len() < 2 {return;}

        if let Some(&sender) = header.hops.first() {
            self.source_routing.correct_exchanged_with(sender, &header.hops);
        }

        let &sender = header.hops.first().expect("Unreachable"); // always have first since path.len() >= 2

        if let Some(Message::Server(server_body)) = self.assembler.handle_fragment(fragment, sender, session_id) {
            self.controller_send
                .send(ClientEvent::MessageAssembled{
                    body: server_body,
                    from: sender,
                    to: self.id,
                })
                .expect("Error in controller_send");
        }

        let ack = Packet::new_ack(
            SourceRoutingHeader::initialize(
                header.hops.iter().cloned().rev().collect::<Vec<_>>()
            ),
            session_id,
            fragment.fragment_index,
        );

        self.send_packet(ack);
    }

    fn handle_flood_response(&mut self, flood_response: &FloodResponse) {
        if let Some(servers_became_reachable) = self.source_routing.add_path(&flood_response.path_trace) {
            self.send_unsended(servers_became_reachable);
        }
    }

    fn handle_ack(&mut self, ack: &Ack, header: &SourceRoutingHeader, session_id: u64) {
        if header.hops.len() < 2 {return;}

        let &server = header.hops.first().expect("Unreachable");

        self.source_routing.correct_send_to(server);

        if let Some((_, pending_fragment)) = self.pending_sessions.get_mut(&session_id) {
            pending_fragment.remove(&ack.fragment_index);
            if pending_fragment.is_empty() {
                self.pending_sessions.remove(&session_id);
            }
        }
    }

    fn handle_nack(&mut self, nack: &Nack, header: &SourceRoutingHeader, session_id: u64) {
        match nack.nack_type {
            NackType::ErrorInRouting(node) => {
                self.source_routing.correct_exchanged_with(node, &header.hops);

                self.source_routing.remove_node(node);
            }
            NackType::DestinationIsDrone => {
                self.source_routing.correct_exchanged_with(header.hops[0], &header.hops);

                //TODO: migliorabile -> possible inviare meno flood request?
                self.send_flood_request();
                //in this scenario, fragment will be added to the unsendeds fragments
            }
            NackType::Dropped => {
                self.source_routing.inc_packet_dropped(&header.hops);
            }
            NackType::UnexpectedRecipient(node) => {
                self.source_routing.correct_exchanged_with(node, &header.hops);
            }
        }

        if let Some((dest, pend_fragment)) = self.pending_sessions.get(&session_id) {
            if let Some(fragment) = pend_fragment.get(&nack.fragment_index) {
                //nack is for a "not-acked" fragment
                self.send_fragment(*dest, fragment.clone());
            }
        }
    }


    fn handle_flood_request(&self, session_id: u64, mut flood_request: FloodRequest) {
        flood_request.increment(self.id, NodeType::Client);
        let flood_response = flood_request.generate_response(session_id);

        self.send_packet(flood_response);
    }
}






//---------------------------//
//---------- TESTS ----------//
//---------------------------//
#[cfg(test)]
mod tests {
    use crossbeam_channel::{unbounded};
    use super::*;


    //---------- CLIENT TEST ----------//
    #[test]
    fn client_test() {
        /*
        Topology with 7 nodes: 1(Client), 2(Drone), 3(Drone), 4(Drone), 5(Drone), 6(Server), 7(Server)
        paths: 1-2-3-(6/7), 1-4-5-(6/7), arco 2-5 (to test crash Drone 3);
        */




        //---------- SENDERS - RECEIVERS ----------//

        //---------- sending/receiving events from the controller ----------//
        let (_, serv_recv_command): (
            Sender<ClientCommand>, Receiver<ClientCommand>
        ) = unbounded();

        let (serv_send_event, ctrl_recv_event): (
            Sender<ClientEvent>, Receiver<ClientEvent>
        ) = unbounded();


        //---------- sending/receiving to neighbor ----------//
        let mut server_senders: HashMap<NodeId, Sender<Packet>> = HashMap::new();

        //channel to client
        let (send_to_client, serv_recv): (Sender<Packet>, Receiver<Packet>) = unbounded();

        //channel to node2
        let (serv_send_2, recv_2): (Sender<Packet>, Receiver<Packet>) = unbounded();
        server_senders.insert(2, serv_send_2.clone());

        //channel to node3
        let (serv_send_3, recv_3): (Sender<Packet>, Receiver<Packet>) = unbounded();
        server_senders.insert(3, serv_send_3.clone());




        //---------- INIT ----------//
        let mut client = Client::new(1, serv_send_event, serv_recv_command, server_senders, serv_recv);

        assert_eq!(client.id, 1);
        assert_eq!(client.packet_send.len(), 2);
        assert!(client.packet_send.contains_key(&2));
        assert!(client.packet_send.contains_key(&3));




        //---------- TEST CTRL COMMAND ----------//

        //---------- remove/add sender ----------//
        client.remove_sender(2);
        assert_eq!(client.packet_send.len(), 1);
        assert!(!client.packet_send.contains_key(&2));
        assert!(client.packet_send.contains_key(&3));

        client.add_sender(2, serv_send_2.clone());
        assert_eq!(client.packet_send.len(), 2);
        assert!(client.packet_send.contains_key(&2));
        assert!(client.packet_send.contains_key(&3));

        //---------- send flood request ----------//
        client.send_flood_request();
        match recv_2.recv() {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }
        match recv_3.recv() {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }
        match ctrl_recv_event.recv() {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }


        //---------- HANDLE PACKET ----------//

        //---------- check packet validity ----------//
        let flood_request = Packet::new_flood_request(SourceRoutingHeader::initialize(vec![]), 0, FloodRequest::initialize(0, 1, NodeType::Client));
        assert!(client.is_valid_packet(&flood_request));

        let ack_short_ = Packet::new_ack(SourceRoutingHeader{ hop_index: 0, hops: vec![1]}, 0,0);
        assert!(!client.is_valid_packet(&ack_short_));

        let fragment_short_ = Packet::new_fragment(SourceRoutingHeader{ hop_index: 0, hops: vec![1]}, 0, Fragment::new(0,1, [0; 128]));
        assert!(!client.is_valid_packet(&fragment_short_));

        let valid_fragment = Packet::new_fragment(SourceRoutingHeader{ hop_index: 2, hops: vec![5,3,1]}, 0, Fragment::new(0,1, [0; 128]));
        assert!(client.is_valid_packet(&valid_fragment));

        let travel_fragment = Packet::new_fragment(SourceRoutingHeader{ hop_index: 2, hops: vec![5,3,1,6,4]}, 0, Fragment::new(0,1, [0; 128]));
        assert!(!client.is_valid_packet(&travel_fragment));

        let not_for_me_fragment = Packet::new_fragment(SourceRoutingHeader{ hop_index: 2, hops: vec![5,3,2]}, 0, Fragment::new(0,1, [0; 128]));
        assert!(!client.is_valid_packet(&not_for_me_fragment));
        match recv_3.recv() {
            Ok(_) => assert!(true),
            Err(_) => assert!(false),
        }

    }
}