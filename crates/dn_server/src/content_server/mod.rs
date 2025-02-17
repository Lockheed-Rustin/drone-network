use crossbeam_channel::{select_biased, unbounded, Receiver, Sender};
use dn_controller::{ServerCommand, ServerEvent};
use dn_message::ClientContentBody;
use dn_message::{ClientBody, Message, ServerBody, ServerContentBody, ServerType};
use dn_router::{
    command::{Command, Event},
    Router, RouterOptions,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use walkdir::{DirEntry, WalkDir};
use wg_2024::{
    network::NodeId,
    packet::{NodeType, Packet},
};

const ASSET_DIR: &str = "assets/content_server";

#[derive(Clone)]
pub struct ContentServerOptions {
    pub id: NodeId,
    pub controller_send: Sender<ServerEvent>,
    pub controller_recv: Receiver<ServerCommand>,
    pub packet_recv: Receiver<Packet>,
    pub packet_send: HashMap<NodeId, Sender<Packet>>,
}

pub struct ContentServer {
    id: NodeId,
    router_opt: RouterOptions,
    controller_send: Sender<ServerEvent>,
    controller_recv: Receiver<ServerCommand>,
    router_send: Receiver<Event>,
    router_recv: Sender<Command>,
}

impl ContentServer {
    #[must_use]
    pub fn new(opt: ContentServerOptions) -> Self {
        let (controller_command_send, controller_command_recv) = unbounded();
        let (controller_event_send, controller_event_recv) = unbounded();
        Self {
            id: opt.id,
            router_opt: RouterOptions {
                id: opt.id,
                node_type: NodeType::Server,
                controller_send: controller_command_send,
                controller_recv: controller_event_recv,
                packet_recv: opt.packet_recv,
                packet_send: opt.packet_send,
            },
            controller_send: opt.controller_send,
            controller_recv: opt.controller_recv,
            router_send: controller_command_recv,
            router_recv: controller_event_send,
        }
    }

    pub fn run(&mut self) {
        let mut router = Router::new(self.router_opt.clone());
        rayon::scope(move |s| {
            s.spawn(move |_| {
                router.run();
            });
            loop {
                select_biased! {
                    recv(self.controller_recv) -> command => {
                        if let Ok(command) = command {
                            if let ServerCommand::Return = command {
                                self.return_router();
                                return;
                            }
                            self.handle_command(command);
                        } else {
                            self.return_router();
                            return;
                        }
                    },
                    recv(self.router_send) -> event => {
                        if let Ok(event) = event {
                            self.handle_event(event);
                        }
                    }
                }
            }
        });
    }

    fn return_router(&self) {
        self.router_recv.send(Command::Return).unwrap();
    }

    fn handle_command(&self, command: ServerCommand) {
        match command {
            ServerCommand::AddSender(id, sender) => {
                self.router_recv
                    .send(Command::AddSender(id, sender))
                    .unwrap();
            }
            ServerCommand::RemoveSender(id) => {
                self.router_recv.send(Command::RemoveSender(id)).unwrap();
            }
            ServerCommand::Return => (),
        }
    }

    fn handle_event(&self, event: Event) {
        match event {
            Event::PacketReceived(packet, id) => self
                .controller_send
                .send(ServerEvent::PacketReceived(packet, id))
                .unwrap(),
            Event::MessageAssembled { body, from, .. } => {
                if let Message::Client(body) = body {
                    self.handle_client_body(body, from);
                }
            }
            Event::MessageFragmented { body, from, to } => {
                if let Message::Server(body) = body {
                    self.controller_send
                        .send(ServerEvent::MessageFragmented { body, from, to })
                        .unwrap();
                }
            }
            Event::PacketSent(packet) => self
                .controller_send
                .send(ServerEvent::PacketSent(packet))
                .unwrap(),
        };
    }

    fn handle_client_body(&self, body: ClientBody, from: NodeId) {
        self.controller_send
            .send(ServerEvent::MessageAssembled {
                body: body.clone(),
                from,
                to: self.id,
            })
            .unwrap();
        match body {
            ClientBody::ReqServerType => {
                self.router_recv
                    .send(Command::SendMessage(
                        Message::Server(ServerBody::RespServerType(ServerType::Content)),
                        from,
                    ))
                    .unwrap();
            }
            ClientBody::ClientContent(body) => match body {
                ClientContentBody::ReqFilesList => self.req_file_list(from),
                ClientContentBody::ReqFile(path) => self.req_file(path, from),
            },
            ClientBody::ClientCommunication(_) => {
                self.router_recv
                    .send(Command::SendMessage(
                        Message::Server(ServerBody::ErrUnsupportedRequestType),
                        from,
                    ))
                    .unwrap();
            }
        }
    }

    fn req_file_list(&self, from: NodeId) {
        let files = WalkDir::new(ASSET_DIR)
            .into_iter()
            .flatten()
            .map(DirEntry::into_path)
            .filter(|p| p.is_file())
            .map(|p| {
                p.strip_prefix(ASSET_DIR)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();

        self.router_recv
            .send(Command::SendMessage(
                Message::Server(ServerBody::ServerContent(ServerContentBody::RespFilesList(
                    files,
                ))),
                from,
            ))
            .unwrap();
    }

    fn req_file(&self, path: String, from: NodeId) {
        let full_path = PathBuf::from(ASSET_DIR).join(&path);
        if let Ok(bytes) = fs::read(full_path) {
            self.router_recv
                .send(Command::SendMessage(
                    Message::Server(ServerBody::ServerContent(ServerContentBody::RespFile(
                        bytes, path,
                    ))),
                    from,
                ))
                .unwrap();
        } else {
            self.router_recv
                .send(Command::SendMessage(
                    Message::Server(ServerBody::ServerContent(
                        ServerContentBody::ErrFileNotFound,
                    )),
                    from,
                ))
                .unwrap();
        }
    }
}
