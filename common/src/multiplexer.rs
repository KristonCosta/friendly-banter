use std::{collections::HashMap};

use async_channel::{Receiver, Sender};

use crate::{message::{MessageProcessorFactory, RawMessage}, message::{ChannelBundle}, message::{Message, ReliableMessage, SignedMessage}, message::{InternalMessage, Target}, runtime::Runtime};




pub struct ConnectionMultiplexer<R>
where
    R: Runtime + 'static
{
    next_id: usize,

    runtime: R,
    connections: HashMap<usize, ChannelBundle>, 
    processor_factory: MessageProcessorFactory<R>,

    reliable_message_receiver: Receiver<SignedMessage<ReliableMessage>>,
    message_receiver: Receiver<SignedMessage<Message>>,
}

impl<R> ConnectionMultiplexer<R> 
where  
    R: Runtime 
{
    pub fn new(runtime: R, packet_sender: Sender<SignedMessage<RawMessage>>) -> Self {
        let (message_sender, message_receiver) = async_channel::unbounded();
        let (reliable_message_sender, reliable_message_receiver) = async_channel::unbounded();
        Self {
            next_id: 1,
            runtime: runtime.clone(),
            processor_factory: MessageProcessorFactory::new(runtime, packet_sender, message_sender, reliable_message_sender), 
            connections: HashMap::new(),          
            message_receiver, 
            reliable_message_receiver
        }
        
    }

    pub fn register(&mut self) -> usize {
        tracing::info!("Registering");
        let id = self.next_id;
        self.next_id += 1;
        let mut processor = self.processor_factory.build(id);
        let bundle = processor.channel_bundle();
        self.runtime.spawn(async move {
            processor.run().await;
        });
        self.connections.insert(id, bundle);
        id
    }   

    pub async fn send_message(&self, target: Target, message: Message) -> Result<(), ()> {
        match target {
            Target::All => {
                for client in self.connections.keys() {
                    self.connections.get(client).unwrap().message_sender.send(message.clone()).await.unwrap();
                }
            }
            Target::Client(client) => {
                if !self.connections.contains_key(&client) {
                    return Err(())
                }
                self.connections.get(&client).unwrap().message_sender.send(message).await.unwrap();
            }
        }
        Ok(())
    }

    pub async fn send_reliable_message(&self, target: Target, message: ReliableMessage) -> Result<(), ()> {
        match target {
            Target::All => {
                for client in self.connections.keys() {
                    self.connections.get(client).unwrap().reliable_message_sender.send(message.clone()).await.unwrap();
                }
            }
            Target::Client(client) => {
                if !self.connections.contains_key(&client) {
                    return Err(())
                }
                self.connections.get(&client).unwrap().reliable_message_sender.send(message).await.unwrap();
            }
        }
        Ok(())
    }

    pub async fn send_raw(&self, target: Target, message: RawMessage) -> Result<(), ()> {
        match target {
            Target::All => {
                for client in self.connections.keys() {
                    self.connections.get(client).unwrap().byte_sender.send(message.clone()).await.unwrap();
                }
            }
            Target::Client(client) => {
                if !self.connections.contains_key(&client) {
                    return Err(())
                }
                self.connections.get(&client).unwrap().byte_sender.send(message).await.unwrap();
            }
        }
        Ok(())
    }

    pub fn get_message_channel(&self, target: usize) -> Sender<Message> {
        self.connections.get(&target).unwrap().message_sender.clone()
    }

    pub fn get_reliable_message_channel(&self, target: usize) -> Sender<ReliableMessage> {
        self.connections.get(&target).unwrap().reliable_message_sender.clone()
    }

    pub fn get_raw_channel(&self, target: usize) -> Sender<RawMessage> {
        self.connections.get(&target).unwrap().byte_sender.clone()
    }

    pub fn reliable_message_receiver(&self) -> Receiver<SignedMessage<ReliableMessage>> {
        self.reliable_message_receiver.clone()
    }

    pub fn message_receiver(&self) -> Receiver<SignedMessage<Message>> {
        self.message_receiver.clone()
    }

    pub fn kill(&mut self, target: usize) {
        if let Some(connection) = self.connections.get(&target) {
            connection.internal_sender.try_send(InternalMessage::Shutdown).unwrap();
            self.connections.remove(&target);
        }
    }
}