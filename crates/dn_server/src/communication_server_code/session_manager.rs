//! The `SessionManager` module is responsible for managing and tracking sessions and their fragments.
//! It allows the creation of new sessions, adding fragments to those sessions, and managing
//! acknowledgments for the fragments that have been sent or received. The module tracks all pending
//! sessions and fragments, and facilitates the recovery of fragments based on their session and index.
//! It also provides an auto-incremented session ID for each new session created.
//!
//! Key Features:
//! - Adds and manages multiple sessions with their associated fragments.
//! - Tracks pending fragments and their acknowledgments to ensure all fragments are received.
//! - Allows for recovery of fragments and destinations when required.
//! - Auto-increments session IDs to uniquely identify each session.

use std::collections::HashMap;
use wg_2024::network::NodeId;
use wg_2024::packet::{Ack, Fragment};

/// A type alias representing the mapping of fragment index to the corresponding fragment in a session.
/// Used to track the fragments that are part of a session.
type PendingFragments = HashMap<u64, Fragment>;
/// A type alias for the session identifier.
pub type SessionId = u64;

/// The `SessionManager` struct is responsible for managing sessions and their associated fragments.
/// It tracks pending fragments for each session, processes acknowledgments, and manages session states.
/// The `SessionManager` also handles the creation and identification of sessions through a session ID counter.
pub struct SessionManager {
    session_id_counter: SessionId,
    pending_sessions: HashMap<u64, PendingFragments>, // session_id -> (fragment_index -> fragment)
    pending_sessions_destination: HashMap<u64, NodeId>, // session_id -> destination_id: NodeId
}

impl SessionManager {
    /// Creates a new instance of `SessionManager`.
    ///
    /// This function initializes the session manager with an ID counter set to 0, and two empty maps:
    /// one for tracking pending sessions and another for tracking the destination node for each session.
    ///
    /// ### Returns:
    /// - A new `SessionManager` instance.
    pub fn new() -> Self {
        Self {
            session_id_counter: 0,
            pending_sessions: HashMap::new(),
            pending_sessions_destination: HashMap::new(),
        }
    }

    /// Adds a new session to the pending sessions map.
    ///
    /// This function takes a session ID, a vector of fragments, and a destination node ID, and stores them
    /// in the `pending_sessions` and `pending_sessions_destination` maps. The fragments are indexed by their
    /// fragment index within the session, allowing for easy tracking.
    ///
    /// ### Arguments:
    /// - `session_id`: The unique identifier of the session.
    /// - `fragments`: A vector of fragments, the serialized `Message`.
    /// - `dest`: The `NodeId` of the message recipient.
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
    /// session are acknowledged, the session is removed from the pending sessions and its destination
    /// is also removed.
    ///
    /// ### Arguments:
    /// - `ack`: The acknowledgment message containing the index of the acknowledged fragment.
    /// - `session_id`: The ID of the session being processed.
    pub fn handle_ack(&mut self, ack: Ack, session_id: &SessionId) {
        if let Some(fragment_map) = self.pending_sessions.get_mut(session_id) {
            fragment_map.remove(&ack.fragment_index);
            if fragment_map.is_empty() {
                self.pending_sessions.remove(session_id);
                self.pending_sessions_destination.remove(session_id);
            }
        }
    }

    /// Retrieves a specific fragment from the session and returns it with the destination node.
    ///
    /// This function allows for recovering a fragment by its index from the list of pending fragments in
    /// a session. It also returns the destination node ID associated with the session. If the fragment
    /// or session is not found, `None` is returned.
    ///
    /// ### Arguments:
    /// - `session_id`: The ID of the session to which the fragment belongs.
    /// - `fragment_index`: The index of the fragment to recover.
    ///
    /// ### Returns:
    /// - `Some((fragment, node))`: A tuple containing the recovered fragment and the destination node ID.
    /// - `None`: If the session or fragment is not found.
    pub fn recover_fragment(
        &self,
        session_id: SessionId,
        fragment_index: u64,
    ) -> Option<(Fragment, u8)> {
        let pending_fragments = self.pending_sessions.get(&session_id)?;
        let node = *self.pending_sessions_destination.get(&session_id)?;
        let fragment = pending_fragments.get(&fragment_index)?.clone();

        Some((fragment, node))
    }

    /// Retrieves and increments the session ID counter.
    ///
    /// This function returns the current value of the session ID counter and then increments it
    /// for the next session. This is used to generate unique session IDs for new sessions.
    ///
    /// ### Returns:
    /// - The current session ID.
    pub fn get_and_increment_session_id_counter(&mut self) -> SessionId {
        let res = self.session_id_counter;
        self.session_id_counter += 1;
        res
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}
