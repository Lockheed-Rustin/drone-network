use std::collections::{HashMap, HashSet};
use wg_2024::network::NodeId;
use dn_message::{ClientBody, ServerType};

//type PendingFragments = HashMap<u64, Fragment>;

pub enum ServerTypeError {
    ServerTypeUnknown,
    WrongServerType,
}


pub struct MessageManager {
    //pending_sessions: HashMap<u64, (NodeId, crate::client::PendingFragments)>, // (dest, session_id) -> (fragment_index -> fragment)
    //unsendable_fragments: HashMap<NodeId, Vec<(u64, Fragment)>>, // dest -> Vec<(session_id, fragment)>
    //already_dropped: HashSet<(u64, u64)>,

    communication_servers: HashMap<NodeId, bool>, //server_id -> already logged
    content_servers: HashSet<NodeId>,

    unsended_messages: HashMap<NodeId, Vec<ClientBody>>,
}


impl MessageManager {
    pub fn new() -> Self {
        Self {
            //pending_sessions: HashMap::new(),
            //unsendable_fragments: HashMap::new(),
            //already_dropped: HashSet::new(),

            communication_servers: HashMap::new(),
            content_servers: HashSet::new(),
            unsended_messages: HashMap::new(),
        }
    }

    pub fn is_invalid_send(&self, client_body: &ClientBody, dest: NodeId) -> Option<ServerTypeError> {
        match client_body {
            ClientBody::ReqServerType => {
                None
            }
            ClientBody::ClientContent(_) => {
                if self.content_servers.contains(&dest) {
                    None
                }
                else if self.communication_servers.contains_key(&dest) {
                    Some(ServerTypeError::WrongServerType)
                }
                else {
                    Some(ServerTypeError::ServerTypeUnknown)
                }
            }
            ClientBody::ClientCommunication(_) => {

                if self.communication_servers.contains_key(&dest) {
                    None
                }
                else if self.content_servers.contains(&dest) {
                    Some(ServerTypeError::WrongServerType)
                }
                else {
                    Some(ServerTypeError::ServerTypeUnknown)
                }
            }
        }
    }

    pub fn is_reg_to_comm(&self, dest: NodeId) -> bool {
        matches!(self.communication_servers.get(&dest), Some(&subscribed) if subscribed)
    }

    pub fn add_server_type(&mut self, server: NodeId, server_type: &ServerType) {
        match server_type {
            ServerType::Content => {
                self.content_servers.insert(server);
            }
            ServerType::Communication => {
                self.communication_servers.entry(server).or_insert(false);
            }
        }
    }

    pub fn add_unsended_message(&mut self, client_body: &ClientBody, dest: NodeId) {
        let unsendeds = self.unsended_messages.entry(dest).or_default();
        unsendeds.push(client_body.clone());
    }

    pub fn get_unsended_message(&mut self, dest: NodeId) -> Option<Vec<ClientBody>> {
        self.unsended_messages.remove(&dest)
    }

    pub fn is_there_unsended_message(&self, dest: NodeId) -> bool {
        self.unsended_messages.contains_key(&dest)
    }

    /*
    pub fn add_unsended_fragment(&mut self, dest: NodeId, session_id: u64, fragment: Fragment) {
        let unsendeds = self.unsendable_fragments.entry(dest).or_default();
        unsendeds.push((session_id, fragment));
    }
    */

    /*
    pub fn resend_fragment(&mut self, session_id: u64, fragment_index: u64) {

    }
    */
}