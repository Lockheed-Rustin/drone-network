use dn_message::{ClientBody, ServerType};
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet};
use std::str;
use wg_2024::network::NodeId;
use wg_2024::packet::Fragment;

//---------- SERVER TYPE ERROR ----------//
/// Enum representing errors related to server type validation.
///
/// It indicates issues with server type identification, such as unknown or incorrect server types.
pub enum ServerTypeError {
    ServerTypeUnknown,
    WrongServerType,
}

//---------- CUSTOM TYPES ----------//
type PendingFragments = HashMap<u64, Fragment>;

//---------- MESSAGE MANAGER ----------//
/// Manages the state and operations related to message fragments and sessions.
///
/// This struct holds various fields to track pending message sessions, fragments,
/// content servers, and unsent messages.
///
/// ### Fields:
/// - `pending_sessions`: A `HashMap` mapping from a tuple of `(dest, session_id)` to `(fragment_index -> fragment)`
///   which tracks the pending fragments for active sessions.
/// - `unsent_fragments`: A `HashMap` mapping from `NodeId` to a vector of tuples `(session_id, fragment)` to track
///   fragments that have not been sent yet.
/// - `already_dropped`: A `HashSet` storing pairs of `(session_id, fragment_id)` that have been dropped.
/// - `communication_servers`: A `HashMap` mapping `NodeId` to a boolean value indicating whether a server has already been logged.
/// - `content_servers`: A `HashSet` of `NodeId` values representing content servers.
/// - `unsent_messages`: A `HashMap` mapping `NodeId` to a vector of `ClientBody` instances for unsent messages.

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
    /// Creates a new `MessageManager` instance with default values.
    ///
    /// This function initializes a `MessageManager` struct, setting all fields to their default states:
    /// empty `HashMap`s and `HashSet`s.
    ///
    /// ### Returns:
    /// - A new instance of `MessageManager` with all fields initialized to their default values.
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
    /// Checks if the specified `client_body` can be sent to the given destination.
    ///
    /// This function validates whether a message of type `ClientBody` can be sent to the specified `dest` (destination) node,
    /// depending on the type of server (content or communication server) the destination represents.
    ///
    /// ### Arguments:
    /// - `client_body`: The body of the client request, which can be of type `ReqServerType`, `ClientContent`, or `ClientCommunication`.
    /// - `dest`: The destination `NodeId` to which the client body is being sent.
    ///
    /// ### Returns:
    /// - `Ok(())`: If the destination server type matches the `client_body` type (or if the body is `ReqServerType`).
    /// - `Err(ServerTypeError)`: If the destination server type is invalid or unknown.

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

    /// Checks if there are any unsent messages for the given destination.
    ///
    /// This function verifies whether there are unsent messages stored for the specified `dest` (destination) node.
    ///
    /// ### Arguments:
    /// - `dest`: The destination `NodeId` to check for unsent messages.
    ///
    /// ### Returns:
    /// - `true`: If there are unsent messages for the given destination.
    /// - `false`: Otherwise.
    #[must_use]
    pub fn is_there_unsent_message(&self, dest: NodeId) -> bool {
        self.unsent_messages.contains_key(&dest)
    }

    /// Checks if the given destination is registered as a communication server.
    ///
    /// This function verifies whether the specified `dest` (destination) node is registered as a communication server
    /// and is marked as subscribed.
    ///
    /// ### Arguments:
    /// - `dest`: The destination `NodeId` to check for registration as a communication server.
    ///
    /// ### Returns:
    /// - `true`: If the destination is registered and subscribed as a communication server.
    /// - `false`: Otherwise.
    #[must_use]
    pub fn is_reg_to_comm(&self, dest: NodeId) -> bool {
        matches!(self.communication_servers.get(&dest), Some(&subscribed) if subscribed)
    }

    //---------- get ----------//
    /// Retrieves the pending fragment for a given session and fragment index.
    ///
    /// This function checks if there is a pending fragment for the specified `session_id` and `fragment_index`,
    /// and returns the associated destination and fragment if found.
    ///
    /// ### Arguments:
    /// - `session_id`: The session ID of the pending fragments.
    /// - `fragment_index`: The index of the specific fragment to retrieve.
    ///
    /// ### Returns:
    /// - `Some((NodeId, Fragment))`: The destination `NodeId` and the corresponding fragment if found.
    /// - `None`: If the fragment is not found for the given session ID and index.
    #[must_use]
    pub fn get_pending_fragment(
        &self,
        session_id: u64,
        fragment_index: u64,
    ) -> Option<(NodeId, Fragment)> {
        if let Some((dest, pend_fragment)) = self.pending_sessions.get(&session_id) {
            pend_fragment
                .get(&fragment_index)
                .map(|fragment| (*dest, fragment.clone()))
        } else {
            None
        }
    }

    /// Retrieves and removes the unsent fragments for the given server.
    ///
    /// This function retrieves the list of unsent fragments for the specified `server` node
    /// and removes them from the `unsent_fragments` collection.
    ///
    /// ### Arguments:
    /// - `server`: The `NodeId` of the server to check for unsent fragments.
    ///
    /// ### Returns:
    /// - `Some(Vec<(u64, Fragment)>)`: A vector of tuples containing the session ID and corresponding fragment if found.
    /// - `None`: If no unsent fragments are found for the given server.
    #[must_use]
    pub fn get_unsent_fragments(&mut self, server: NodeId) -> Option<Vec<(u64, Fragment)>> {
        self.unsent_fragments.remove(&server)
    }

    /// Retrieves and removes the unsent messages for the given destination.
    ///
    /// This function retrieves the list of unsent messages for the specified `dest` node
    /// and removes them from the `unsent_messages` collection.
    ///
    /// ### Arguments:
    /// - `dest`: The `NodeId` of the destination to check for unsent messages.
    ///
    /// ### Returns:
    /// - `Some(Vec<ClientBody>)`: A vector of unsent `ClientBody` messages if found.
    /// - `None`: If no unsent messages are found for the given destination.
    #[must_use]
    pub fn get_unsent_message(&mut self, dest: NodeId) -> Option<Vec<ClientBody>> {
        self.unsent_messages.remove(&dest)
    }

    //---------- add ----------//
    /// Adds a server of a specific type to the corresponding server collection.
    ///
    /// This function adds the specified `server` to either the content servers or communication servers collection,
    /// depending on the given `server_type`. If the server is a communication server, it is added with a default value of `false`.
    ///
    /// ### Arguments:
    /// - `server`: The `NodeId` of the server to add.
    /// - `server_type`: The type of the server, which determines which collection to add the server to (Content or Communication).
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

    /// Adds a new pending session with its associated fragments.
    ///
    /// This function stores a new pending session in the `pending_sessions` collection,
    /// mapping the `session_id` to its destination and fragments. Each fragment is added to the `pending_fragment`
    /// map using its `fragment_index` as the key.
    ///
    /// ### Arguments:
    /// - `session_id`: The unique ID for the session.
    /// - `dest`: The destination `NodeId` for the session.
    /// - `fragments`: A reference to a vector of `Fragment` objects associated with the session.
    pub fn add_pending_session(
        &mut self,
        session_id: u64,
        dest: NodeId,
        fragments: &Vec<Fragment>,
    ) {
        let mut pending_fragment: PendingFragments = HashMap::new();

        for fragment in fragments {
            pending_fragment.insert(fragment.fragment_index, fragment.clone());
        }

        self.pending_sessions
            .insert(session_id, (dest, pending_fragment));
    }

    /// Adds an unsent fragment to the collection for the specified destination.
    ///
    /// This function inserts the provided `fragment` into the `unsent_fragments` collection for the given `dest` node,
    /// associating it with the corresponding `session_id`. If no entry exists for the destination, a new entry is created.
    ///
    /// ### Arguments:
    /// - `session_id`: The session ID associated with the fragment.
    /// - `dest`: The destination `NodeId` for the fragment.
    /// - `fragment`: A reference to the `Fragment` to be added.
    pub fn add_unsent_fragment(&mut self, session_id: u64, dest: NodeId, fragment: &Fragment) {
        let unsents = self.unsent_fragments.entry(dest).or_default();
        unsents.push((session_id, fragment.clone()));
    }

    /// Adds an unsent message to the collection for the specified destination.
    ///
    /// This function inserts the provided `client_body` into the `unsent_messages` collection for the given `dest` node.
    /// If no entry exists for the destination, a new entry is created.
    ///
    /// ### Arguments:
    /// - `client_body`: A reference to the `ClientBody` to be added as an unsent message.
    /// - `dest`: The destination `NodeId` for the message.
    pub fn add_unsent_message(&mut self, client_body: &ClientBody, dest: NodeId) {
        let unsents = self.unsent_messages.entry(dest).or_default();
        unsents.push(client_body.clone());
    }

    //---------- fragment dropped managment ----------//
    /// Updates the dropped status of a fragment for a given session.
    ///
    /// This function checks if the specified fragment, identified by `session_id` and `fragment_index`,
    /// has already been marked as dropped. If it has, the fragment is removed from the `already_dropped` set,
    /// and `true` is returned. If the fragment was not previously dropped, it is added to the set, and `false` is returned.
    ///
    /// ### Arguments:
    /// - `session_id`: The session ID for the fragment.
    /// - `fragment_index`: The index of the fragment within the session.
    ///
    /// ### Returns:
    /// - `true`: If the fragment was previously dropped and is now marked as not dropped.
    /// - `false`: Otherwise.
    pub fn update_fragment_dropped(&mut self, session_id: u64, fragment_index: u64) -> bool {
        if self.already_dropped.contains(&(session_id, fragment_index)) {
            self.already_dropped.remove(&(session_id, fragment_index));
            true
        } else {
            self.already_dropped.insert((session_id, fragment_index));
            false
        }
    }

    /// Resets the list of already dropped fragments.
    ///
    /// This function clears the `already_dropped` set, effectively removing all entries of fragments that have been marked as dropped.
    pub fn reset_already_dropped(&mut self) {
        self.already_dropped.clear();
    }

    //---------- ack managment ----------//
    /// Confirms the acknowledgment of a fragment for a given session.
    ///
    /// This function removes the specified fragment, identified by `session_id` and `fragment_index`,
    /// from both the `already_dropped` set and the `pending_sessions` collection.
    /// If no more fragments remain in the session, the session is removed from the `pending_sessions` collection.
    ///
    /// ### Arguments:
    /// - `session_id`: The session ID of the fragment being acknowledged.
    /// - `fragment_index`: The index of the fragment being acknowledged.
    pub fn confirm_ack(&mut self, session_id: u64, fragment_index: u64) {
        self.already_dropped.remove(&(session_id, fragment_index));

        if let Some((_, pending_fragment)) = self.pending_sessions.get_mut(&session_id) {
            pending_fragment.remove(&fragment_index);
            if pending_fragment.is_empty() {
                self.pending_sessions.remove(&session_id);
            }
        }
    }

    //---------- file html x external links ----------//
    /// Checks if a given file is an HTML file based on its MIME type.
    ///
    /// This function uses the `infer` library to detect the MIME type of the provided `file` and checks if the MIME type
    /// is `"text/html"`, indicating the file is an HTML file.
    ///
    /// ### Arguments:
    /// - `file`: A reference to a `Vec<u8>` representing the file to check.
    ///
    /// ### Returns:
    /// - `true`: If the MIME type of the file is `"text/html"`.
    /// - `false`: Otherwise.
    #[must_use]
    pub fn is_html_file(file: &Vec<u8>) -> bool {
        let info = infer::get(file.as_slice());
        if let Some(info) = info {
            info.mime_type() == "text/html"
        } else {
            false
        }
    }

    /// Extracts all internal links (href and src attributes) from an HTML file.
    ///
    /// This function parses the provided `file` (as a `Vec<u8>`) as HTML and extracts all links
    /// from the `href` attributes of `<a>` tags and the `src` attributes of `<img>` tags,
    /// excluding those that start with a hash (`#`). It returns a vector of strings containing the links.
    ///
    /// ### Arguments:
    /// - `file`: A reference to a `Vec<u8>` representing the HTML file to parse.
    ///
    /// ### Returns:
    /// - A `Vec<String>` containing all extracted internal links from the HTML document.
    pub fn get_internal_links(file: &Vec<u8>) -> Vec<String> {
        let Ok(content) = str::from_utf8(file.as_slice()) else {
            return Vec::new();
        };

        let document = Html::parse_document(content);

        let Ok(a_selector) = Selector::parse("a[href]") else {
            return Vec::new();
        };
        let Ok(img_selector) = Selector::parse("img[src]") else {
            return Vec::new();
        };

        let mut links = document
            .select(&a_selector)
            .filter_map(|element| element.value().attr("href"))
            .filter(|link| !link.starts_with('#'))
            .map(String::from)
            .collect::<Vec<String>>();

        links.extend(
            document
                .select(&img_selector)
                .filter_map(|element| element.value().attr("src"))
                .filter(|link| !link.starts_with('#')) // Escludi anche per le immagini
                .map(String::from),
        );

        links
    }
}

//---------------------------//
//---------- TESTS ----------//
//---------------------------//
#[cfg(test)]
mod tests {
    use super::*;
    use dn_message::ClientContentBody;

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
            fragments.push(Fragment {
                fragment_index: i,
                total_n_fragments: 10,
                length: 0,
                data: [0u8; 128],
            })
        }

        message_manager.add_pending_session(session_id, dest, &fragments);
        assert_eq!(message_manager.pending_sessions.len(), 1);
        assert_eq!(
            message_manager.pending_sessions.get(&session_id).unwrap().0,
            dest
        );
        assert_eq!(
            message_manager
                .pending_sessions
                .get(&session_id)
                .unwrap()
                .1
                .len(),
            10
        );

        message_manager.confirm_ack(session_id, 0);
        assert_eq!(message_manager.pending_sessions.len(), 1);
        assert_eq!(
            message_manager.pending_sessions.get(&session_id).unwrap().0,
            dest
        );
        assert_eq!(
            message_manager
                .pending_sessions
                .get(&session_id)
                .unwrap()
                .1
                .len(),
            9
        );
        assert!(!message_manager
            .pending_sessions
            .get(&session_id)
            .unwrap()
            .1
            .contains_key(&0));

        let fragment = message_manager.get_pending_fragment(session_id, 1);
        assert!(fragment.is_some());
        assert_eq!(fragment.unwrap().0, dest);
        let fragment2 = message_manager.get_pending_fragment(session_id, 0);
        assert!(fragment2.is_none());

        //---------- unsent fragment ----------//
        message_manager.add_unsent_fragment(session_id, dest, &fragments[0]);
        message_manager.add_unsent_fragment(session_id, dest, &fragments[1]);
        assert_eq!(message_manager.unsent_fragments.len(), 1);
        assert_eq!(
            message_manager.unsent_fragments.get(&dest).unwrap().len(),
            2
        );

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
        assert!(matches!(
            res.unwrap_err(),
            ServerTypeError::ServerTypeUnknown
        ));

        message_manager.add_server_type(dest, &ServerType::Communication);
        let res = message_manager.is_valid_send(&message, dest);
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), ServerTypeError::WrongServerType));

        let new_dest: NodeId = dest + 1;
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

        //---------- parser html ----------//
        let html_content = b"<!DOCTYPE html><html><body>Hello, world!</body></html>".to_vec();
        let not_html_content = b"Questo non HTML.".to_vec();

        assert!(MessageManager::is_html_file(&html_content));
        assert!(!MessageManager::is_html_file(&not_html_content));

        let html_content_with_links = b"<!DOCTYPE html>
            <html>
                <body>
                    <a href=\"media\\\\quack.png\">lol</a>
                </body>
            </html>"
            .to_vec();

        assert!(MessageManager::get_internal_links(&html_content).is_empty());
        let vec = MessageManager::get_internal_links(&html_content_with_links);
        assert!(!vec.is_empty());
        assert_eq!(vec.len(), 4);
        assert!(vec.contains(&"media\\\\quack.png".to_string()));
        assert!(vec.contains(&"/relative-link".to_string()));
        assert!(vec.contains(&"https://example.com/image.jpg".to_string()));
        assert!(vec.contains(&"../relative-image.jpg".to_string()));
    }
}
