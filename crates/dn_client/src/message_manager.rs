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
    unsent_fragments: HashMap<NodeId, Vec<(u64, Fragment)>>, // dest -> Vec<(session_id, fragment)>
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
            unsent_fragments: HashMap::new(),
            already_dropped: HashSet::new(),

            communication_servers: HashMap::new(),
            content_servers: HashSet::new(),
            unsent_messages: HashMap::new(),
        }
    }

    //---------- checks ----------//
    #[allow(clippy::missing_errors_doc)]
    pub fn is_valid_send(
        &self,
        client_body: &ClientBody,
        dest: NodeId,
    ) -> Result<(), ServerTypeError> {
        match client_body {
            ClientBody::ReqServerType => Ok(()),
            ClientBody::ClientContent(_) => {
                if self.content_servers.contains(&dest) {
                    Ok(())
                } else if self.communication_servers.contains_key(&dest) {
                    Err(ServerTypeError::WrongServerType)
                } else {
                    Err(ServerTypeError::ServerTypeUnknown)
                }
            }
            ClientBody::ClientCommunication(_) => {
                if self.communication_servers.contains_key(&dest) {
                    Ok(())
                } else if self.content_servers.contains(&dest) {
                    Err(ServerTypeError::WrongServerType)
                } else {
                    Err(ServerTypeError::ServerTypeUnknown)
                }
            }
        }
    }

    #[must_use]
    pub fn is_there_unsent_message(&self, dest: NodeId) -> bool {
        self.unsent_messages.contains_key(&dest)
    }

    #[must_use]
    pub fn is_reg_to_comm(&self, dest: NodeId) -> bool {
        matches!(self.communication_servers.get(&dest), Some(&subscribed) if subscribed)
    }


    //---------- get ----------//
    #[must_use]
    pub fn get_pending_fragment(&self, session_id: u64, fragment_index: u64) -> Option<(NodeId, Fragment)> {
        if let Some((dest, pend_fragment)) = self.pending_sessions.get(&session_id) {
            pend_fragment.get(&fragment_index).map(|fragment| (*dest, fragment.clone()))
        }
        else {
            None
        }
    }

    #[must_use]
    pub fn get_unsent_fragments(&mut self, server: NodeId) -> Option<Vec<(u64, Fragment)>> {
        self.unsent_fragments.remove(&server)
    }

    #[must_use]
    pub fn get_unsent_message(&mut self, dest: NodeId) -> Option<Vec<ClientBody>> {
        self.unsent_messages.remove(&dest)
    }



    //---------- add ----------//
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

    pub fn add_pending_session(&mut self, session_id: u64, dest: NodeId, fragments: &Vec<Fragment>) {
        let mut pending_fragment: PendingFragments = HashMap::new();

        for fragment in fragments {
            pending_fragment.insert(fragment.fragment_index, fragment.clone());
        }

        self.pending_sessions
            .insert(session_id, (dest, pending_fragment));
    }

    pub fn add_unsent_fragment(&mut self, session_id: u64, dest: NodeId, fragment: &Fragment) {
        let unsents = self.unsent_fragments.entry(dest).or_default();
        unsents.push((session_id, fragment.clone()));
    }

    pub fn add_unsent_message(&mut self, client_body: &ClientBody, dest: NodeId) {
        let unsents = self.unsent_messages.entry(dest).or_default();
        unsents.push(client_body.clone());
    }


    //---------- fragment dropped managment ----------//
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

    pub fn reset_already_dropped(&mut self) {
        self.already_dropped.clear();
    }


    //---------- ack managment ----------//
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


//---------------------------//
//---------- TESTS ----------//
//---------------------------//
#[cfg(test)]
mod tests {
    use dn_message::ClientContentBody;
    use super::*;

    //---------- DRONE INFO TEST ----------//
    #[test]
    fn message_manager_test() {

        //---------- init ----------//
        let mut message_manager = MessageManager::new();

        assert!(message_manager.pending_sessions.is_empty());
        assert!(message_manager.unsent_fragments.is_empty());
        assert!(message_manager.already_dropped.is_empty());
        assert!(message_manager.communication_servers.is_empty());
        assert!(message_manager.content_servers.is_empty());
        assert!(message_manager.unsent_messages.is_empty());


        //---------- pending fragment ----------//
        let session_id: u64 = 0;
        let dest: NodeId = 5;
        let mut fragments: Vec<Fragment> = Vec::new();
        for i in 0..10u64 {
            fragments.push(Fragment{
                fragment_index: i,
                total_n_fragments: 10,
                length: 0,
                data: [0u8; 128],
            })
        }

        message_manager.add_pending_session(session_id, dest, &fragments);
        assert_eq!(message_manager.pending_sessions.len(), 1);
        assert_eq!(message_manager.pending_sessions.get(&session_id).unwrap().0, dest);
        assert_eq!(message_manager.pending_sessions.get(&session_id).unwrap().1.len(), 10);

        message_manager.confirm_ack(session_id, 0);
        assert_eq!(message_manager.pending_sessions.len(), 1);
        assert_eq!(message_manager.pending_sessions.get(&session_id).unwrap().0, dest);
        assert_eq!(message_manager.pending_sessions.get(&session_id).unwrap().1.len(), 9);
        assert!(!message_manager.pending_sessions.get(&session_id).unwrap().1.contains_key(&0));

        let fragment = message_manager.get_pending_fragment(session_id, 1);
        assert!(fragment.is_some());
        assert_eq!(fragment.unwrap().0, dest);
        let fragment2 = message_manager.get_pending_fragment(session_id, 0);
        assert!(fragment2.is_none());


        //---------- unsent fragment ----------//
        message_manager.add_unsent_fragment(session_id, dest, &fragments[0]);
        message_manager.add_unsent_fragment(session_id, dest, &fragments[1]);
        assert_eq!(message_manager.unsent_fragments.len(), 1);
        assert_eq!(message_manager.unsent_fragments.get(&dest).unwrap().len(), 2);

        let unsent_fragments = message_manager.get_unsent_fragments(dest);
        assert!(unsent_fragments.is_some());
        assert_eq!(unsent_fragments.unwrap().len(), 2);
        let unsent_fragments = message_manager.get_unsent_fragments(dest);
        assert!(unsent_fragments.is_none());
        assert!(fragment2.is_none());


        //---------- already dropped ----------//
        assert!(!message_manager.update_fragment_dropped(session_id, 0));
        assert_eq!(message_manager.already_dropped.len(), 1);
        assert!(!message_manager.update_fragment_dropped(session_id, 1));
        assert_eq!(message_manager.already_dropped.len(), 2);
        assert!(message_manager.update_fragment_dropped(session_id, 0));
        assert_eq!(message_manager.already_dropped.len(), 1);
        message_manager.reset_already_dropped();
        assert_eq!(message_manager.already_dropped.len(), 0);


        //---------- servers checks ----------//
        let message = ClientBody::ClientContent(ClientContentBody::ReqFile("A".to_string()));

        let res = message_manager.is_valid_send(&message, dest);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), ServerTypeError::ServerTypeUnknown));

        message_manager.add_server_type(dest, &ServerType::Communication);
        let res = message_manager.is_valid_send(&message, dest);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), ServerTypeError::WrongServerType));

        let new_dest: NodeId = dest+1;
        message_manager.add_server_type(new_dest, &ServerType::Content);
        let res = message_manager.is_valid_send(&message, new_dest);
        assert!(res.is_ok());

        assert!(!message_manager.is_reg_to_comm(new_dest)); //not a communication server
        assert!(!message_manager.is_reg_to_comm(dest)); //not registered
        message_manager.communication_servers.insert(dest, true);
        assert!(message_manager.is_reg_to_comm(dest)); //not registered


        //---------- unsent messages ----------//
        assert!(message_manager.get_unsent_message(dest).is_none());
        message_manager.add_unsent_message(&message, dest);
        assert!(message_manager.is_there_unsent_message(dest));
        assert!(message_manager.get_unsent_message(dest).is_some());
        assert!(!message_manager.is_there_unsent_message(dest));
    }
}