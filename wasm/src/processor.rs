use std::collections::HashMap;
use futures::try_join;
use futures::pin_mut;
use futures::FutureExt;
use crate::{client::WebRTCClient, runtime::WasmRuntime};
use common::message::{Message, Object, ReliableMessage, SignedMessage, InternalMessage, RawMessage};
use futures::select;
use js_sys::{Array, Promise};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::future_to_promise;
use common::runtime::Runtime;
use wasm_bindgen::__rt::core::time::Duration;
use common::multiplexer::ConnectionMultiplexer;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Array<string>")]
    pub type JSRustVec;
}

#[wasm_bindgen]
pub struct ConnectionBundle {
    tx: async_channel::Sender<Message>,
    reliable_tx: async_channel::Sender<ReliableMessage>,
    internal_tx: async_channel::Sender<InternalMessage>,

    channel_number: usize
}

#[wasm_bindgen]
pub struct Processor {
    connection_bundle: Option<ConnectionBundle>,
    multiplexer: ConnectionMultiplexer<WasmRuntime>,
    pending_messages: Vec<String>,

    message_receiver: async_channel::Receiver<SignedMessage<Message>>,
    reliable_message_receiver: async_channel::Receiver<SignedMessage<ReliableMessage>>,
    signed_packet_receiver: async_channel::Receiver<SignedMessage<RawMessage>>,

    position: (f32, f32),

    state: HashMap<u32, Object>,

    players: HashMap<u32, (f32, f32)>,

    connected: bool,
}

#[wasm_bindgen]
impl Processor {
    pub fn start() -> Self {
        let (signed_packet_sender, signed_packet_receiver) = async_channel::unbounded();
        let mut multiplexer = common::multiplexer::ConnectionMultiplexer::new(WasmRuntime::new(), signed_packet_sender);
        let message_receiver = multiplexer.message_receiver();
        let reliable_message_receiver = multiplexer.reliable_message_receiver();
        let mut s = Self {
            connection_bundle: None,
            multiplexer,
            connected: false,
            pending_messages: Vec::with_capacity(100),
            position: (50.0, 50.0),
            players: HashMap::new(),
            state: HashMap::new(),
            message_receiver,
            reliable_message_receiver,
            signed_packet_receiver
        };
        s
    }

    pub fn connect(&mut self, url: String) {
        if self.connected {
            tracing::info!("Already connected please disconnect first");
            return
        }
        let (tx, rx) = async_channel::unbounded();
        let (reliable_tx, reliable_rx) = async_channel::unbounded();
        let (internal_tx, internal_rx) = async_channel::unbounded();
        let channel_number = self.multiplexer.register();
        let client = WebRTCClient::new(
            url,
            self.signed_packet_receiver.clone(),
            self.multiplexer.get_raw_channel(channel_number),
            internal_rx.clone(),
        );


        let queued_messages = rx;
        let message_sender = self.multiplexer.get_message_channel(channel_number);
        let reliable_message_sender = self.multiplexer.get_reliable_message_channel(channel_number);
        let runtime = WasmRuntime::new();
        let inner = tx.clone();
        let inner_internal_rx = internal_rx.clone();
        runtime.spawn(async move {
            let runtime = WasmRuntime::new();
            let ping = async move {
                loop {
                    tracing::info!("Ping");
                    inner.try_send(Message::Sync).unwrap();
                    runtime.sleep(Duration::from_secs(1)).await;
                }
            }.fuse();
            let dispatcher = async move {
                loop {
                    for message in queued_messages.recv().await {
                        tracing::info!("Processor sending message {:?}", message);
                        message_sender.send(message).await.unwrap();
                    }
                }
            }.fuse();
            let reliable_dispatcher = async move {
                loop {
                    for message in reliable_rx.recv().await {
                        tracing::info!("Processor sending reliable message {:?}", message);
                        reliable_message_sender.send(message).await.unwrap();
                    }
                }
            }.fuse();
            let terminate = async move {
                inner_internal_rx.recv().await;
                tracing::info!("terminating connection1");
            }.fuse();
            pin_mut!(dispatcher, reliable_dispatcher, ping, terminate);
            select! {
                () = dispatcher => {},
                () = reliable_dispatcher => {},
                () = ping => {},
                () = terminate => {}
            };
        });
        let inner_internal_rx = internal_rx.clone();
        let promise = future_to_promise(async move {
            let terminate = async move {
                inner_internal_rx.recv().await;
                tracing::info!("terminating connection2");
            }.fuse();
            let client_runner = client.run().fuse();
            pin_mut!(client_runner, terminate);
            select! {
              () = client_runner => {},
              () = terminate => {},
            };
            client.close();
            Ok(JsValue::UNDEFINED)
        });
        self.connection_bundle = Some(
            ConnectionBundle {
                tx,
                reliable_tx,
                internal_tx,
                channel_number
            }
        );
        self.connected = true;
    }

    pub fn disconnect(&mut self) {
        let runtime = WasmRuntime::new();
        let bundle = self.connection_bundle.as_ref();
        match bundle {
            None => {}
            Some(bundle) => {
                self.multiplexer.kill(bundle.channel_number);
                for _ in 0..100 {
                    match bundle.internal_tx.try_send(InternalMessage::Shutdown) {
                        Err(_) => {break;}
                        _ => {}
                    }
                }

            }
        }
        self.connected = false;
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
                    Message::Player(_, _) => {}
                },
                Err(_) => {
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
                Err(_) => {
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
        if !self.connected {
            tracing::info!("Tried to send message but no connection exists!");
            return;
        }
        match self.connection_bundle.as_ref() {
            None => {
                tracing::info!("Tried to send message but no connection exists!");
            }
            Some(bundle) => {
                tracing::info!("Processor sending message position");
                bundle.tx.try_send(Message::Position(x, y)).unwrap();
            }
        }
    }

    pub fn send(&self, string: String) {
        if !self.connected {
            tracing::info!("Tried to send message but no connection exists!");
            return;
        }
        match self.connection_bundle.as_ref() {
            None => {
                tracing::info!("Tried to send message but no connection exists!");
            }
            Some(bundle) => {
                tracing::info!("Processor sending message {:?}", string);
                bundle.reliable_tx.try_send(ReliableMessage::Text(string)).unwrap();
            }
        }
    }
}
