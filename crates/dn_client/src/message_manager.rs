use std::collections::{HashMap, HashSet};
use wg_2024::network::NodeId;
use dn_message::{ClientBody, ServerType};

pub enum ServerTypeError {
    ServerTypeUnknown,
    WrongServerType,
}


pub struct MessageManager {

    communication_servers: HashMap<NodeId, bool>, //server_id -> already logged
    content_servers: HashSet<NodeId>,

    unsent_messages: HashMap<NodeId, Vec<ClientBody>>,
}

impl Default for MessageManager {
    fn default() -> Self {
        Self::new()
    }
}



impl MessageManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            //pending_sessions: HashMap::new(),
            //unsendable_fragments: HashMap::new(),
            //already_dropped: HashSet::new(),

            communication_servers: HashMap::new(),
            content_servers: HashSet::new(),
            unsent_messages: HashMap::new(),
        }
    }


    #[must_use]
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

    #[must_use]
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

    pub fn add_unsent_message(&mut self, client_body: &ClientBody, dest: NodeId) {
        let unsents = self.unsent_messages.entry(dest).or_default();
        unsents.push(client_body.clone());
    }

    #[must_use]
    pub fn get_unsent_message(&mut self, dest: NodeId) -> Option<Vec<ClientBody>> {
        self.unsent_messages.remove(&dest)
    }
    #[must_use]
    pub fn is_there_unsent_message(&self, dest: NodeId) -> bool {
        self.unsent_messages.contains_key(&dest)
    }
}