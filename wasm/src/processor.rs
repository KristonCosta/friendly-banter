use std::collections::HashMap;

use crate::{client::WebRTCClient, runtime::WasmRuntime};
use common::message::{Message, MessageProcessor, Object, ReliableMessage, SignedMessage};
use futures::join;
use js_sys::{Array, Promise};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::future_to_promise;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Array<string>")]
    pub type JSRustVec;
}

#[wasm_bindgen]
pub struct Processor {
    tx: async_channel::Sender<Message>,
    reliable_tx: async_channel::Sender<ReliableMessage>,
    
    message_receiver: async_channel::Receiver<SignedMessage<Message>>,
    reliable_message_receiver: async_channel::Receiver<SignedMessage<ReliableMessage>>,

    pending_messages: Vec<String>,

    position: (f32, f32),

    _promise: Promise,

    state: HashMap<u32, Object>,

    connected: bool,
}

#[wasm_bindgen]
impl Processor {
    pub fn start() -> Self {
        let (tx, rx) = async_channel::unbounded();
        let (reliable_tx, reliable_rx) = async_channel::unbounded();
        let (signed_packet_sender, signed_packet_receiver) = async_channel::unbounded();
        let mut multiplexer = common::multiplexer::ConnectionMultiplexer::new(WasmRuntime::new(), signed_packet_sender);
        multiplexer.register();
        let client = WebRTCClient::new(
            "http://192.168.1.12:8080/session".to_string(),
            signed_packet_receiver,
            multiplexer.get_raw_channel(1),
        );

        let inner_receiver = multiplexer.message_receiver();
        let inner_reliable_receiver = multiplexer.reliable_message_receiver();

        let queued_messages = rx;
        let message_sender = multiplexer.get_message_channel(1);
        let reliable_message_sender = multiplexer.get_reliable_message_channel(1);
        let promise = future_to_promise(async move {
            let dispatcher = async move {
                loop {
                    for message in queued_messages.recv().await {
                        tracing::info!("Processor sending message {:?}", message);
                        message_sender.send(message).await.unwrap();
                    }
                }
            };
            let reliable_dispatcher = async move {
                loop {
                    for message in reliable_rx.recv().await {
                        tracing::info!("Processor sending message {:?}", message);
                        reliable_message_sender.send(message).await.unwrap();
                    }
                }
            };
            join![client.run(), dispatcher, reliable_dispatcher];
            Ok(JsValue::UNDEFINED)
        });
        
        Self {
            tx,
            reliable_tx,
            connected: false,
            message_receiver: inner_receiver,
            reliable_message_receiver: inner_reliable_receiver,
            pending_messages: Vec::with_capacity(100),
            position: (50.0, 50.0),
            _promise: promise,
            state: HashMap::new(),
        }
    }

    pub fn process_pending(&mut self) {
        for _ in 0..10 {
            match self.message_receiver.try_recv() {
                Ok(message) => match message.message {
                    Message::Sync => {}
                    Message::Position(x, y) => {
                        self.position = (x, y);
                        self.pending_messages.push(format!("{:?}", self.position));
                    }
                    Message::Unknown => {}
                    Message::State(object) => {
                        self.state.insert(object.id, object);
                    }
                },
                Err(e) => {
                    break;
                }
            }
        }
        for _ in 0..10 {
            match self.reliable_message_receiver.try_recv() {
                Ok(message) => match message.message {
                    ReliableMessage::State(object) => {
                        tracing::info!("Getting state: {:?}", object);
                        for (key, value) in object {
                            self.state.insert(key, value);
                        }
                       // self.state = object;
                    }
                    ReliableMessage::Disconnected(client) => {
                        tracing::info!("Disconnected: {:?}", client);
                    }
                    ReliableMessage::Connected(client) => {
                        tracing::info!("Connected: {:?}", client);
                    }
                    ReliableMessage::Connect => {
                        tracing::info!("Connect");
                        self.connected = true;
                    }
                    ReliableMessage::Text(txt) => {
                        tracing::info!("Received message: {:?}", txt);
                        self.pending_messages.push(txt);
                    }
                },
                Err(e) => {
                    break;
                }
            }
        }
    }
    pub fn get_pending(&mut self) -> JSRustVec {
        self.process_pending();
        if !self.pending_messages.is_empty() {
            tracing::info!("Pending: {:?}", self.pending_messages);
        }
        let my_vec: Array = self.pending_messages.drain(..).map(JsValue::from).collect();
        
        my_vec.unchecked_into::<JSRustVec>()
    }

    pub fn state(&mut self) -> JsValue {
        if !self.connected {
            self.send("Hello!".to_string());
        }
        self.process_pending();
        serde_wasm_bindgen::to_value(&self.state).unwrap()
    }

    pub fn click(&self, x: f32, y: f32) {
        self.tx.try_send(Message::Position(x, y)).unwrap();
    }

    pub fn send(&self, string: String) {
        tracing::info!("Processor sending message {:?}", string);
        self.reliable_tx.try_send(ReliableMessage::Text(string)).unwrap();
    }
}
