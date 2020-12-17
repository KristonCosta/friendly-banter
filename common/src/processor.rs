use std::sync::Arc;

use async_channel::Sender;
use futures_util::{future::FutureExt, lock::Mutex, pin_mut, select, StreamExt};
use turbulence::{BufferPacket, Packet, PacketPool, Runtime as TRuntime};

use crate::{
    buffer::SimpleBufferPool,
    message::{
        InternalMessage, Message, RawMessage, ReliableMessage, SignedMessage, MESSAGE_SETTINGS,
        RELIABLE_MESSAGE_SETTINGS,
    },
    runtime::{Runtime, RuntimeImpl},
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

enum WrappedMessage {
    Message(Message),
    ReliableMessage(ReliableMessage),
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
        builder
            .register::<ReliableMessage>(RELIABLE_MESSAGE_SETTINGS)
            .unwrap();

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

                let pending_outgoing_reliable_message =
                    self.reliable_message_channel.receiver.recv().fuse();
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
                                message,
                            })
                            .await
                            .unwrap();
                    }
                    WrappedMessage::ReliableMessage(message) => {
                        self.reliable_message_outgoing
                            .send(SignedMessage::<ReliableMessage> {
                                id: self.id,
                                message,
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
                }
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
    pub fn new(
        runtime: R,
        byte_channel_outgoing: Sender<SignedMessage<RawMessage>>,
        message_channel_outgoing: Sender<SignedMessage<Message>>,
        reliable_channel_outgoing: Sender<SignedMessage<ReliableMessage>>,
    ) -> Self {
        Self {
            byte_channel_outgoing,
            runtime,
            message_channel_outgoing,
            reliable_channel_outgoing,
        }
    }

    pub fn build(&self, id: usize) -> MessageProcessor<R> {
        MessageProcessor::new(
            id,
            self.runtime.clone(),
            self.byte_channel_outgoing.clone(),
            self.reliable_channel_outgoing.clone(),
            self.message_channel_outgoing.clone(),
        )
    }
}
