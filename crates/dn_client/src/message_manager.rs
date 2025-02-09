use dn_message::{ClientBody, ServerType};
use std::collections::{HashMap, HashSet};
use wg_2024::network::NodeId;
use wg_2024::packet::Fragment;



//---------- SERVER TYPE ERROR ----------//
pub enum ServerTypeError {
    ServerTypeUnknown,
    WrongServerType,
}



//---------- CUSTOM TYPES ----------//
type PendingFragments = HashMap<u64, Fragment>;



//---------- MESSAGE MANAGER ----------//
pub struct MessageManager {
    pending_sessions: HashMap<u64, (NodeId, PendingFragments)>, // (dest, session_id) -> (fragment_index -> fragment)
    unsendable_fragments: HashMap<NodeId, Vec<(u64, Fragment)>>, // dest -> Vec<(session_id, fragment)>
    already_dropped: HashSet<(u64, u64)>,

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
            pending_sessions: HashMap::new(),
            unsendable_fragments: HashMap::new(),
            already_dropped: HashSet::new(),

            communication_servers: HashMap::new(),
            content_servers: HashSet::new(),
            unsent_messages: HashMap::new(),
        }
    }

    #[must_use]
    pub fn is_invalid_send(
        &self,
        client_body: &ClientBody,
        dest: NodeId,
    ) -> Option<ServerTypeError> {
        match client_body {
            ClientBody::ReqServerType => None,
            ClientBody::ClientContent(_) => {
                if self.content_servers.contains(&dest) {
                    None
                } else if self.communication_servers.contains_key(&dest) {
                    Some(ServerTypeError::WrongServerType)
                } else {
                    Some(ServerTypeError::ServerTypeUnknown)
                }
            }
            ClientBody::ClientCommunication(_) => {
                if self.communication_servers.contains_key(&dest) {
                    None
                } else if self.content_servers.contains(&dest) {
                    Some(ServerTypeError::WrongServerType)
                } else {
                    Some(ServerTypeError::ServerTypeUnknown)
                }
            }
        }
    }

    #[must_use]
    pub fn is_reg_to_comm(&self, dest: NodeId) -> bool {
        matches!(self.communication_servers.get(&dest), Some(&subscribed) if subscribed)
    }

    pub fn reset_already_dropped(&mut self) {
        self.already_dropped.clear();
    }

    ///Check if a fragment has been already dropped once:
    ///
    pub fn update_fragment_dropped(&mut self, session_id: u64, fragment_index: u64) -> bool {
        if self
            .already_dropped
            .contains(&(session_id, fragment_index))
        {
            self
                .already_dropped
                .remove(&(session_id, fragment_index));
            true
        } else {
            self.already_dropped
                .insert((session_id, fragment_index));
            false
        }
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

    pub fn add_pending_fragments(&mut self, session_id: u64, dest: NodeId, fragments: &Vec<Fragment>) {
        let mut pending_fragment: PendingFragments = HashMap::new();

        for fragment in fragments {
            pending_fragment.insert(fragment.fragment_index, fragment.clone());
        }

        self.pending_sessions
            .insert(session_id, (dest, pending_fragment));
    }

    pub fn add_unsent_message(&mut self, client_body: &ClientBody, dest: NodeId) {
        let unsents = self.unsent_messages.entry(dest).or_default();
        unsents.push(client_body.clone());
    }

    pub fn add_unsent_fragment(&mut self, session_id: u64, dest: NodeId, fragment: Fragment) {
        let unsents = self.unsendable_fragments.entry(dest).or_default();
        unsents.push((session_id, fragment));
    }

    pub fn get_unsent_fragments(&mut self, server: NodeId) -> Option<Vec<(u64, Fragment)>> {
        self.unsendable_fragments.remove(&server)
    }

    #[must_use]
    pub fn get_unsent_message(&mut self, dest: NodeId) -> Option<Vec<ClientBody>> {
        self.unsent_messages.remove(&dest)
    }
    #[must_use]
    pub fn is_there_unsent_message(&self, dest: NodeId) -> bool {
        self.unsent_messages.contains_key(&dest)
    }

    pub fn get_pending_fragment(&self, session_id: u64, fragment_index: u64) -> Option<(NodeId, Fragment)> {
        if let Some((dest, pend_fragment)) = self.pending_sessions.get(&session_id) {
            pend_fragment.get(&fragment_index).map(|fragment| (*dest, fragment.clone()))
        }
        else {
            None
        }
    }

    pub fn confirm_ack(&mut self, session_id: u64, fragment_index: u64) {

        self.already_dropped
            .remove(&(session_id, fragment_index));

        if let Some((_, pending_fragment)) = self.pending_sessions.get_mut(&session_id) {
            pending_fragment.remove(&fragment_index);
            if pending_fragment.is_empty() {
                self.pending_sessions.remove(&session_id);
            }
        }
    }
}
