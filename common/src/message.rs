use futures_util::{future::FutureExt, pin_mut, select, StreamExt};
use turbulence::{BufferPacket, Packet, PacketPool, Runtime as TRuntime};

use crate::{
    buffer::SimpleBufferPool,
    runtime::{Runtime, RuntimeImpl},
};

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum Message {
    Sync,
    Text(String),
    Position(f32, f32),
    Unknown,
}

pub const MESSAGE_SETTINGS: turbulence::MessageChannelSettings = turbulence::MessageChannelSettings {
    channel: 1,
    channel_mode: turbulence::MessageChannelMode::Unreliable,
    message_buffer_size: 8,
    packet_buffer_size: 8,
};

pub struct BidirectionalChannel<T> {
    incoming_reciever: async_channel::Receiver<T>,
    incoming_sender: async_channel::Sender<T>,
    outgoing_receiver: async_channel::Receiver<T>,
    outgoing_sender: async_channel::Sender<T>,
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

pub type RawMessage = Box<[u8]>;

pub struct MessageProcessor<R>
where
    R: Runtime,
{
    runtime: RuntimeImpl<R>,
    pool: turbulence::BufferPacketPool<SimpleBufferPool>,
    turbulence_channels: turbulence::MessageChannels,

    incoming_packets: turbulence::IncomingMultiplexedPackets<turbulence::BufferPacket<RawMessage>>,
    outgoing_packets: turbulence::OutgoingMultiplexedPackets<turbulence::BufferPacket<RawMessage>>,

    message_channel: BidirectionalChannel<Message>,
    byte_channel: BidirectionalChannel<RawMessage>,
}

enum Action {
    EmitBytes(BufferPacket<Box<[u8]>>),
    DispatchBytes(Box<[u8]>),
    DispatchMessage(Message),
    EmitMessage(Message),
    Error,
    Flush,
}

impl<R> MessageProcessor<R>
where
    R: Runtime + Clone + 'static,
{
    pub fn new(runtime: R) -> Self {
        let pool = turbulence::BufferPacketPool::new(SimpleBufferPool(32));
        let runtime = RuntimeImpl::new(runtime);
        let mut multiplexer = turbulence::PacketMultiplexer::new();
        let mut builder = turbulence::MessageChannelsBuilder::new(runtime.clone(), pool);
        builder.register::<Message>(MESSAGE_SETTINGS).unwrap();

        let turbulence_channels = builder.build(&mut multiplexer);
        let (incoming_packets, outgoing_packets) = multiplexer.start();

        let message_channel = BidirectionalChannel::<Message>::new();

        let byte_channel = BidirectionalChannel::<RawMessage>::new();

        MessageProcessor {
            pool,
            runtime,
            turbulence_channels,
            incoming_packets,
            outgoing_packets,

            message_channel,
            byte_channel,
        }
    }

    pub async fn run(&mut self) {
        loop {
            let action = {
                let pending_outgoing_message = self.message_channel.outgoing_receiver.recv().fuse();
                pin_mut!(pending_outgoing_message);

                let pending_incoming_message =
                    self.turbulence_channels.try_async_recv::<Message>().fuse();
                pin_mut!(pending_incoming_message);

                let pending_incoming_byte = self.byte_channel.incoming_reciever.recv().fuse();
                pin_mut!(pending_incoming_byte);

                let pending_outgoing_byte = self.outgoing_packets.next().fuse();

                pin_mut!(pending_outgoing_byte);

                let sleep_timer = self.runtime.sleep(std::time::Duration::from_millis(1000)).fuse();
                pin_mut!(sleep_timer);

                let action: Action = select! {
                    incoming = pending_incoming_message => {
                        match incoming {
                            Ok(message) => Action::EmitMessage(message),
                            _ => Action::Error
                        }
                    }
                    outgoing = pending_outgoing_message => {
                        match outgoing {
                            Ok(message) => Action::DispatchMessage(message),
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

                    _ = sleep_timer => {
                        Action::Flush
                    }
                };
                action
            };
            match action {
                Action::DispatchBytes(bytes) => {
                    let mut packet = self.pool.acquire();
                    packet.extend(&bytes);
                    self.incoming_packets.try_send(packet).unwrap();
                }
                Action::EmitBytes(buf_bytes) => {
                    let mut bytes = Vec::with_capacity(buf_bytes.len());
                    bytes.extend_from_slice(buf_bytes.as_slice());
                    self.byte_channel
                        .outgoing_sender
                        .send(bytes.into_boxed_slice())
                        .await
                        .unwrap();
                }
                Action::DispatchMessage(message) => {
                    self.turbulence_channels.send(message);
                }
                Action::EmitMessage(message) => {
                    self.message_channel.incoming_sender.send(message).await.unwrap();
                }
                Action::Error => {
                    panic!("Error in the message processor.");
                }
                Action::Flush => self.turbulence_channels.flush::<Message>(),
            }
        }
    }

    pub fn message_sender(&self) -> async_channel::Sender<Message> {
        self.message_channel.outgoing_sender.clone()
    }

    pub fn message_receiver(&self) -> async_channel::Receiver<Message> {
        self.message_channel.incoming_reciever.clone()
    }

    pub fn incoming_byte_sender(&self) -> async_channel::Sender<RawMessage> {
        self.byte_channel.incoming_sender.clone()
    }

    pub fn outgoing_byte_reader(&self) -> async_channel::Receiver<RawMessage> {
        self.byte_channel.outgoing_receiver.clone()
    }
}
