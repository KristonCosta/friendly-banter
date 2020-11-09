use std::{sync::Arc, collections::HashMap};

use async_channel::{Receiver, Sender};
use futures_util::{future::FutureExt, lock::Mutex, pin_mut, select, StreamExt};
use instant::Duration;
use turbulence::{
    reliable_channel, BufferPacket,
    MessageChannelMode::{self, Reliable},
    Packet, PacketPool, Runtime as TRuntime,
};

use crate::{
    buffer::SimpleBufferPool,
    runtime::{Runtime, RuntimeImpl},
};

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
    State(Object),
    Unknown,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ReliableMessage {
    Disconnected(String),
    Connected(String),
    State(HashMap<u32, Object>),
    Text(String),
    Connect
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

enum WrappedMessage {
    Message(Message),
    ReliableMessage(ReliableMessage),
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
                bandwidth: 4096,
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
            max_message_len: 1024,
        },
        message_buffer_size: 64,
        packet_buffer_size: 64,
    };

pub struct BidirectionalChannel<T> {
    pub incoming_reciever: async_channel::Receiver<T>,
    pub incoming_sender: async_channel::Sender<T>,
    pub outgoing_receiver: async_channel::Receiver<T>,
    pub outgoing_sender: async_channel::Sender<T>,
}

impl<T> BidirectionalChannel<T> {
    pub fn new() -> Self {
        let (incoming_message_sender, incoming_message_reciever) = async_channel::unbounded();
        let (outgoing_message_sender, outgoing_message_reciever) = async_channel::unbounded();
        BidirectionalChannel::<T> {
            incoming_reciever: incoming_message_reciever,
            incoming_sender: incoming_message_sender,
            outgoing_receiver: outgoing_message_reciever,
            outgoing_sender: outgoing_message_sender,
        }
    }
}

pub struct DirectionalChannel<T> {
    receiver: async_channel::Receiver<T>,
    sender: async_channel::Sender<T>,
}

impl<T> DirectionalChannel<T> {
    pub fn new() -> Self {
        let (sender, receiver) = async_channel::unbounded();
        DirectionalChannel::<T> { sender, receiver }
    }
}

pub type RawMessage = Box<[u8]>;

pub struct MessageProcessor<R>
where
    R: Runtime,
{
    id: usize,
    runtime: RuntimeImpl<R>,
    pool: turbulence::BufferPacketPool<SimpleBufferPool>,
    turbulence_channels: turbulence::MessageChannels,

    incoming_packets: turbulence::IncomingMultiplexedPackets<turbulence::BufferPacket<RawMessage>>,
    outgoing_packets: turbulence::OutgoingMultiplexedPackets<turbulence::BufferPacket<RawMessage>>,

    reliable_message_channel: DirectionalChannel<ReliableMessage>,
    message_channel: DirectionalChannel<Message>,

    byte_channel_outgoing: Sender<SignedMessage<RawMessage>>,
    message_outgoing: Sender<SignedMessage<Message>>,
    reliable_message_outgoing: Sender<SignedMessage<ReliableMessage>>,
    incoming_byte_channel: DirectionalChannel<RawMessage>,
    internal_channel: DirectionalChannel<InternalMessage>,
}

enum Action {
    EmitBytes(BufferPacket<Box<[u8]>>),
    DispatchBytes(RawMessage),
    DispatchMessage(WrappedMessage),
    EmitMessage(WrappedMessage),
    Error,
    Shutdown,
    Flush,
}

pub(crate) struct ChannelBundle {
    pub message_sender: Sender<Message>,
    pub byte_sender: Sender<RawMessage>,
    pub reliable_message_sender: Sender<ReliableMessage>,  
    pub internal_sender: Sender<InternalMessage>, 
}

pub enum InternalMessage {
    Shutdown
}

impl<R> MessageProcessor<R>
where
    R: Runtime + Clone + 'static,
{
    fn new(
        id: usize,
        runtime: R,
        byte_channel_outgoing: Sender<SignedMessage<RawMessage>>,
        reliable_message_outgoing: Sender<SignedMessage<ReliableMessage>>,
        message_outgoing: Sender<SignedMessage<Message>>,
    ) -> Self {
        let pool = turbulence::BufferPacketPool::new(SimpleBufferPool(32));
        let runtime = RuntimeImpl::new(runtime);
        let mut multiplexer = turbulence::PacketMultiplexer::new();
        let mut builder = turbulence::MessageChannelsBuilder::new(runtime.clone(), pool);
        builder.register::<Message>(MESSAGE_SETTINGS).unwrap();
        builder.register::<ReliableMessage>(RELIABLE_MESSAGE_SETTINGS).unwrap();

        let turbulence_channels = builder.build(&mut multiplexer);
        let (incoming_packets, outgoing_packets) = multiplexer.start();

        let message_channel = DirectionalChannel::<Message>::new();
        let reliable_message_channel = DirectionalChannel::<ReliableMessage>::new();
        let incoming_byte_channel = DirectionalChannel::<RawMessage>::new();
        let internal_channel = DirectionalChannel::<InternalMessage>::new();

        MessageProcessor {
            id,
            pool,
            runtime,
            turbulence_channels,
            incoming_packets,
            outgoing_packets,

            reliable_message_channel,
            message_channel,
            byte_channel_outgoing,
            incoming_byte_channel,
            reliable_message_outgoing, 
            message_outgoing,
            internal_channel,
        }
    }

    pub async fn run(mut self) {
        loop {
            let action = {
                let pending_outgoing_message = self.message_channel.receiver.recv().fuse();
                pin_mut!(pending_outgoing_message);

                let pending_outgoing_reliable_message = self.reliable_message_channel.receiver.recv().fuse();
                pin_mut!(pending_outgoing_reliable_message);

                let channel_lock = Arc::new(Mutex::new(&mut self.turbulence_channels));

                let pending_incoming_message = async {
                    let mut lock = channel_lock.lock().await;
                    lock.try_async_recv::<Message>().await
                }
                .fuse();
                pin_mut!(pending_incoming_message);

                let pending_incoming_reliable_message = async {
                    let mut lock = channel_lock.lock().await;
                    lock.try_async_recv::<ReliableMessage>().await
                }
                .fuse();
                pin_mut!(pending_incoming_reliable_message);

                let pending_incoming_byte = self.incoming_byte_channel.receiver.recv().fuse();
                pin_mut!(pending_incoming_byte);

                let pending_outgoing_byte = self.outgoing_packets.next().fuse();
                pin_mut!(pending_outgoing_byte);


                let pending_internal = self.internal_channel.receiver.recv().fuse();
                pin_mut!(pending_internal);

                let sleep_timer = self
                    .runtime
                    .sleep(std::time::Duration::from_millis(5))
                    .fuse();
                pin_mut!(sleep_timer);

                let action: Action = select! {
                    incoming = pending_incoming_message => {
                        match incoming {
                            Ok(message) => Action::EmitMessage(WrappedMessage::Message(message)),
                            _ => Action::Error
                        }
                    },
                    reliable_incoming = pending_incoming_reliable_message => {
                        match reliable_incoming {
                            Ok(message) => Action::EmitMessage(WrappedMessage::ReliableMessage(message)),
                            _ => Action::Error
                        }
                    },
                    outgoing = pending_outgoing_message => {
                        match outgoing {
                            Ok(message) => Action::DispatchMessage(WrappedMessage::Message(message)),
                            _ => Action::Error
                        }
                    },
                    reliable_outgoing = pending_outgoing_reliable_message => {
                        match reliable_outgoing {
                            Ok(message) => Action::DispatchMessage(WrappedMessage::ReliableMessage(message)),
                            _ => Action::Error
                        }
                    },
                    incoming = pending_incoming_byte => {
                        match incoming {
                            Ok(bytes) => Action::DispatchBytes(bytes),
                            _ => Action::Error
                        }
                    },
                    outgoing = pending_outgoing_byte => {
                        match outgoing {
                            Some(bytes) => Action::EmitBytes(bytes),
                            _ => Action::Error
                        }
                    },
                    message = pending_internal => { 
                        match message {
                            Ok(message) => {
                                match message {
                                    InternalMessage::Shutdown => Action::Shutdown
                                }
                            }, 
                            _ => Action::Error
                        }
                    },
                    _ = sleep_timer => {
                        Action::Flush
                    }
                };
                action
            };
            match action {
                Action::DispatchBytes(message) => {
                    let mut packet = self.pool.acquire();
                    packet.extend(&message);
                    self.incoming_packets.try_send(packet).unwrap();
                }
                Action::EmitBytes(buf_bytes) => {
                    let mut bytes = Vec::with_capacity(buf_bytes.len());
                    bytes.extend_from_slice(buf_bytes.as_slice());
                    self.byte_channel_outgoing
                        .send(SignedMessage::<RawMessage> {
                            id: self.id,
                            message: bytes.into_boxed_slice(),
                        })
                        .await
                        .unwrap();
                }
                Action::DispatchMessage(message) => match message {
                    WrappedMessage::Message(message) => {
                        self.turbulence_channels.send(message);
                    }
                    WrappedMessage::ReliableMessage(message) => {
                        self.turbulence_channels.send(message);
                    }
                },
                Action::EmitMessage(message) => match message {
                    WrappedMessage::Message(message) => {
                        self.message_outgoing
                            .send(SignedMessage::<Message> {
                                id: self.id,
                                message
                            })
                            .await
                            .unwrap();
                    }
                    WrappedMessage::ReliableMessage(message) => {
                        self.reliable_message_outgoing
                            .send(SignedMessage::<ReliableMessage> {
                                id: self.id,
                                message
                            })
                            .await
                            .unwrap();
                    }
                },
                Action::Error => {
                    // panic!("Error in the message processor.");
                }
                Action::Flush => {
                    self.turbulence_channels.flush::<ReliableMessage>();
                    self.turbulence_channels.flush::<Message>();
                },
                Action::Shutdown => { 
                    tracing::info!("Killing message processor");
                    break; 
                }
            }
        }
    }

    pub(crate) fn channel_bundle(&self) -> ChannelBundle {
        ChannelBundle {
            message_sender: self.message_channel.sender.clone(),
            reliable_message_sender: self.reliable_message_channel.sender.clone(),
            byte_sender: self.incoming_byte_channel.sender.clone(),
            internal_sender: self.internal_channel.sender.clone(),
        }
    }
}

#[derive(Clone)]
pub struct MessageProcessorFactory<R>
where
    R: Runtime + 'static,
{
    runtime: R,
    byte_channel_outgoing: Sender<SignedMessage<RawMessage>>,
    message_channel_outgoing: Sender<SignedMessage<Message>>,
    reliable_channel_outgoing: Sender<SignedMessage<ReliableMessage>>,
}

impl<R> MessageProcessorFactory<R>
where
    R: Runtime,
{
    pub fn new(runtime: R, byte_channel_outgoing: Sender<SignedMessage<RawMessage>>, message_channel_outgoing: Sender<SignedMessage<Message>>, reliable_channel_outgoing: Sender<SignedMessage<ReliableMessage>>) -> Self {
        Self {
            byte_channel_outgoing,
            runtime,
            message_channel_outgoing,
            reliable_channel_outgoing
        }
    }

    pub fn build(&self, id: usize) -> MessageProcessor<R> {
        MessageProcessor::new(
            id,
            self.runtime.clone(),
            self.byte_channel_outgoing.clone(),
            self.reliable_channel_outgoing.clone(),
            self.message_channel_outgoing.clone()
        )
    }
}
