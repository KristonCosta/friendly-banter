use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    time::{Duration, Instant},
};

use async_channel::{Receiver, Sender};
use common::message::{
    GameState, Message, Object, ObjectInfo, ReliableMessage, SignedMessage, Tree,
};
use rand::Rng;

use crate::InternalMessage;

type FP = f32;
const MS_PER_UPDATE: FP = 0.5;

#[derive(Debug)]
pub struct TimeStep {
    last_time: Instant,
    delta_time: FP,
    frame_count: u32,
    frame_time: FP,
}

impl TimeStep {
    // https://gitlab.com/flukejones/diir-doom/blob/master/game/src/main.rs
    // Grabbed this from here
    pub fn new() -> TimeStep {
        TimeStep {
            last_time: Instant::now(),
            delta_time: 0.0,
            frame_count: 0,
            frame_time: 0.0,
        }
    }

    pub fn delta(&mut self) -> FP {
        let current_time = Instant::now();
        let delta = current_time.duration_since(self.last_time).as_micros() as FP * 0.001;
        self.last_time = current_time;
        self.delta_time = delta;
        delta
    }

    // provides the framerate in FPS
    pub fn frame_rate(&mut self) -> Option<u32> {
        self.frame_count += 1;
        self.frame_time += self.delta_time;
        let tmp;
        // per second
        if self.frame_time >= 1000.0 {
            tmp = self.frame_count;
            self.frame_count = 0;
            self.frame_time = 0.0;
            return Some(tmp);
        }
        None
    }
}

pub struct ChannelBundle {
    pub reliable_sender: Sender<SignedMessage<ReliableMessage>>,
    pub reliable_receiver: Receiver<SignedMessage<ReliableMessage>>,
    pub message_sender: Sender<SignedMessage<Message>>,
    pub message_receiver: Receiver<SignedMessage<Message>>,
    pub internal_receiver: Receiver<InternalMessage>,
}

#[derive(PartialEq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Disconnected,
}

pub struct Universe {
    bundle: ChannelBundle,
    timestep: TimeStep,
    state: HashMap<u32, Object>,
    players: HashMap<u32, Player>,
    connected_clients: HashMap<usize, ConnectionState>,
}

struct Player {
    position: (f32, f32)
}

impl Universe {
    fn make_tree(id: u32) -> Object {
        let mut rng = rand::thread_rng();
        Object {
            id,
            object_info: ObjectInfo::Tree(Tree {
                position: (rng.gen_range(0.0, 500.0), rng.gen_range(0.0, 400.0)),
                size: rng.gen_range(3.0, 10.0),
            }),
        }
    }

    pub fn refresh_clients(&mut self, clients: HashSet<usize>) {
        self.connected_clients.iter_mut().for_each(|x| {
            if !clients.contains(x.0) {
                *x.1 = ConnectionState::Disconnected;
            }
        });

        for client in clients {
            if !self.connected_clients.contains_key(&client) {
                self.connected_clients
                    .insert(client, ConnectionState::Connecting);
            }
        }
    }

    pub fn new(bundle: ChannelBundle) -> Self {
        let mut state = HashMap::new();
        for i in 0..20 {
            state.insert(i, Universe::make_tree(i));
        }
        Self {
            bundle,
            timestep: TimeStep::new(),
            connected_clients: HashMap::new(),
            players: HashMap::new(),
            state,
        }
    }

    async fn send_game_state(&mut self, target: usize) {
        tracing::info!("Sending game state {:?}", target);
        self.bundle
            .reliable_sender
            .send(SignedMessage {
                id: target,
                message: ReliableMessage::State(self.state.clone()),
            })
            .await
            .unwrap();
    }

    async fn client_tick(&mut self) {
        if let Ok(message) = self.bundle.internal_receiver.try_recv() {
            match message {
                InternalMessage::ClientSnapshot(clients) => {
                    tracing::info!("Received client snapshot: {:?}", clients);
                    self.refresh_clients(clients);
                    let disconnected_clients: Vec<usize> = self
                        .connected_clients
                        .iter()
                        .filter(|&(_, state)| *state == ConnectionState::Disconnected)
                        .map(|(addr, _)| *addr)
                        .collect();
                    let new_clients: Vec<usize> = self
                        .connected_clients
                        .iter()
                        .filter(|&(_, state)| *state == ConnectionState::Connecting)
                        .map(|(addr, _)| *addr)
                        .collect();
                    let current_clients: Vec<usize> = self
                        .connected_clients
                        .iter()
                        .filter(|&(_, state)| *state == ConnectionState::Connected)
                        .map(|(addr, _)| *addr)
                        .collect();

                    for disconnected in disconnected_clients {
                        for client in &current_clients {
                            self.bundle
                                .reliable_sender
                                .send(SignedMessage::<ReliableMessage> {
                                    id: *client,
                                    message: ReliableMessage::Disconnected(
                                        disconnected.to_string(),
                                    ),
                                })
                                .await
                                .unwrap();
                        }
                    }
                    for connected in &new_clients {
                        self.bundle
                            .reliable_sender
                            .send(SignedMessage {
                                id: *connected,
                                message: ReliableMessage::Connect,
                            })
                            .await
                            .unwrap();
                        self.send_game_state(*connected).await;
                        for client in &current_clients {
                            self.bundle
                                .reliable_sender
                                .send(SignedMessage::<ReliableMessage> {
                                    id: *client,
                                    message: ReliableMessage::Connected(connected.to_string()),
                                })
                                .await
                                .unwrap();
                        }
                    }
                    self.connected_clients.clear();
                    new_clients.into_iter().for_each(|x| {
                        self.initialize_new_client(x);
                        self.connected_clients.insert(x, ConnectionState::Connected);
                    });
                    current_clients.into_iter().for_each(|x| {
                        self.connected_clients.insert(x, ConnectionState::Connected);
                    });
                }
            }
        }
    }

    pub fn initialize_new_client(&mut self, client_id: usize) {
        let player = Player {
            position: (0.0, 0.0)
        };
        self.players.insert(client_id as u32, player);
    }

    pub async fn run(&mut self) {
        // I'm not going for efficiency here...

        loop {
            tokio::time::sleep(Duration::from_millis(16)).await;
            if let Ok(message) = self.bundle.message_receiver.try_recv() {
                match message.message {
                    Message::Sync => {}
                    Message::Position(x, y) => {
                        for (id, _) in &self.connected_clients {
                            self.bundle
                                        .reliable_sender
                                        .send(SignedMessage {
                                            id: *id,
                                            message: ReliableMessage::Text(format!("Client {} clicked {:?}", message.id, (x, y))),
                                        })
                                        .await
                                        .unwrap();
                        }
                    }
                    Message::State(_) => {}
                    Message::Unknown => {}
                    Message::Player(_, _) => {}
                }
                
            }
            self.client_tick().await;
            for (player, pos) in &self.players {
                for (id, _) in &self.connected_clients {
                    self.bundle
                        .message_sender
                        .send(SignedMessage {
                            id: *id,
                            message: Message::Player(*player, pos.position)
                        })
                        .await
                        .unwrap();
                }
            }

        }
    }
}
