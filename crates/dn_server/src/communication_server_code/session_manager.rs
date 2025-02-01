use std::collections::HashMap;
use wg_2024::network::NodeId;
use wg_2024::packet::{Ack, Fragment};

type PendingFragments = HashMap<u64, Fragment>;
pub type SessionId = u64;
pub struct SessionManager {
    session_id_counter: SessionId,
    pending_sessions: HashMap<u64, PendingFragments>, // session_id -> (fragment_index -> fragment)
    pending_sessions_destination: HashMap<u64, NodeId>, // session_id -> destination_id: NodeId
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            session_id_counter: 0,
            pending_sessions: HashMap::new(),
            pending_sessions_destination: HashMap::new(),
        }
    }

    /// Adds a new session to the pending sessions map.
    ///
    /// This function takes a session ID and a list of fragments, and stores them in the
    /// `pending_sessions` map. Each fragment is indexed by its fragment index within the session.
    /// This helps keep track of the fragments associated with the session for handling acknowledgments.
    pub fn add_session(&mut self, session_id: SessionId, fragments: Vec<Fragment>, dest: NodeId) {
        let fragment_map: PendingFragments = fragments
            .into_iter()
            .map(|f| (f.fragment_index, f))
            .collect();
        self.pending_sessions.insert(session_id, fragment_map);
        self.pending_sessions_destination.insert(session_id, dest);
    }

    /// Processes an acknowledgment for a specific session.
    ///
    /// This function handles an incoming acknowledgment by removing the corresponding fragment
    /// from the list of pending fragments associated with a session. If all fragments for the
    /// session are acknowledged, the session is removed from the pending sessions.
    pub fn handle_ack(&mut self, ack: Ack, session_id: &SessionId) {
        if let Some(fragment_map) = self.pending_sessions.get_mut(session_id) {
            fragment_map.remove(&ack.fragment_index);
            if fragment_map.is_empty() {
                self.pending_sessions.remove(session_id);
                self.pending_sessions_destination.remove(session_id);
            }
        }
    }

    pub fn recover_fragment(&self, session_id: SessionId, fragment_index: u64) -> Option<(Fragment, u8)> {
        let pending_fragments = self.pending_sessions.get(&session_id)?;
        let node = *self.pending_sessions_destination.get(&session_id)?;
        let fragment = pending_fragments.get(&fragment_index)?.clone();

        Some((fragment, node))
    }

    pub fn get_and_increment_session_id_counter(&mut self) -> SessionId {
        let res = self.session_id_counter;
        self.session_id_counter += 1;
        res
    }

    pub fn get_session_id_counter(&self) -> SessionId {
        self.session_id_counter
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
