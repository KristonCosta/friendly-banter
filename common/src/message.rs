use std::collections::HashMap;

use instant::Duration;
use turbulence::{reliable_channel, MessageChannelMode};
#[derive(serde::Serialize, serde::Deserialize, Debug, Copy, Clone)]
pub struct Tree {
    pub position: (f32, f32),
    pub size: f32,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Player {
    pub position: (f32, f32),
    pub color: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ObjectInfo {
    Tree(Tree),
    Player(Player),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Object {
    pub id: u32,
    pub object_info: ObjectInfo,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct GameState {
    pub objects: Vec<Object>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum Message {
    Sync,
    Position(f32, f32),
    Player(u32, (f32, f32)),
    State(Object),
    Unknown,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ReliableMessage {
    Disconnected(String),
    Connected(String),
    State(HashMap<u32, Object>),
    Text(String),
    Connect,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum Target {
    All,
    Client(usize),
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct SignedMessage<T> {
    pub id: usize,
    pub message: T,
}

pub enum InternalMessage {
    Shutdown,
}

pub const MESSAGE_SETTINGS: turbulence::MessageChannelSettings =
    turbulence::MessageChannelSettings {
        channel: 1,
        channel_mode: turbulence::MessageChannelMode::Unreliable,
        message_buffer_size: 64,
        packet_buffer_size: 64,
    };

pub const RELIABLE_MESSAGE_SETTINGS: turbulence::MessageChannelSettings =
    turbulence::MessageChannelSettings {
        channel: 0,
        channel_mode: MessageChannelMode::Reliable {
            reliability_settings: reliable_channel::Settings {
                bandwidth: 409600,
                recv_window_size: 1024,
                send_window_size: 1024,
                burst_bandwidth: 1024,
                init_send: 512,
                wakeup_time: Duration::from_millis(100),
                initial_rtt: Duration::from_millis(200),
                max_rtt: Duration::from_secs(1),
                rtt_update_factor: 0.1,
                rtt_resend_factor: 1.5,
            },
            max_message_len: u16::MAX as usize - 1,
        },
        message_buffer_size: 64,
        packet_buffer_size: 64,
    };

pub type RawMessage = Box<[u8]>;
