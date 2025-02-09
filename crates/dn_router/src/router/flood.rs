use super::Router;
use wg_2024::{
    network::SourceRoutingHeader,
    packet::{FloodRequest, FloodResponse, NodeType, Packet, PacketType},
};

impl Router {
    pub(crate) fn handle_flood_request(&mut self, mut flood_request: FloodRequest) {
        flood_request.path_trace.push((self.id, NodeType::Server));
        let mut hops = flood_request
            .path_trace
            .iter()
            .cloned()
            .map(|(id, _)| id)
            .rev()
            .collect::<Vec<_>>();
        if hops.last() != Some(&flood_request.initiator_id) {
            hops.push(flood_request.initiator_id);
        }

        let session_id = self.inc_session_id();
        self.routing.send_flood_response(Packet {
            routing_header: SourceRoutingHeader { hop_index: 1, hops },
            session_id,
            pack_type: PacketType::FloodResponse(FloodResponse {
                flood_id: flood_request.flood_id,
                path_trace: flood_request.path_trace,
            }),
        });
    }

    pub(crate) fn handle_flood_response(&mut self, flood: FloodResponse) {
        self.routing.add_path(flood.path_trace);
    }

    pub(crate) fn should_flood(&mut self) -> bool {
        self.drop_count = (self.drop_count + 1) % 10;
        self.drop_count == 0
    }
}
