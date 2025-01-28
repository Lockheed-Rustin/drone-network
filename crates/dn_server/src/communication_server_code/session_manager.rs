use std::collections::HashMap;
use wg_2024::packet::{Ack, Fragment};

type PendingFragments = HashMap<u64, Fragment>;

pub struct SessionManager {
    session_id_counter: u64,
    pending_sessions: HashMap<u64, PendingFragments>, // session_id -> (fragment_index -> fragment)
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            session_id_counter: 0,
            pending_sessions: HashMap::new(),
        }
    }

    /// Adds a new session to the pending sessions map.
    ///
    /// This function takes a session ID and a list of fragments, and stores them in the
    /// `pending_sessions` map. Each fragment is indexed by its fragment index within the session.
    /// This helps keep track of the fragments associated with the session for handling acknowledgments.
    pub fn add_session(&mut self, session_id: u64, fragments: Vec<Fragment>) {
        let fragment_map: PendingFragments = fragments
            .into_iter()
            .map(|f| (f.fragment_index, f))
            .collect();
        self.pending_sessions.insert(session_id, fragment_map);
    }

    /// Processes an acknowledgment for a specific session.
    ///
    /// This function handles an incoming acknowledgment by removing the corresponding fragment
    /// from the list of pending fragments associated with a session. If all fragments for the
    /// session are acknowledged, the session is removed from the pending sessions.
    pub fn handle_ack(&mut self, ack: Ack, session_id: &u64) {
        if let Some(fragment_map) = self.pending_sessions.get_mut(session_id) {
            fragment_map.remove(&ack.fragment_index);
            if fragment_map.is_empty() {
                self.pending_sessions.remove(session_id);
            }
        }
    }

    pub fn get_and_increment_session_id_counter(&mut self) -> u64 {
        let res = self.session_id_counter;
        self.session_id_counter += 1;
        res
    }

    pub fn get_session_id_counter(&self) -> u64 {
        self.session_id_counter
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}