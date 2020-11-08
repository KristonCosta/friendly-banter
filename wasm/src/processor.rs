use std::collections::HashMap;

use crate::{client::WebRTCClient, runtime::WasmRuntime};
use common::message::{Message, MessageProcessor, Object};
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

    message_receiver: async_channel::Receiver<Message>,

    pending_messages: Vec<String>,

    position: (f32, f32),

    _promise: Promise,

    state: HashMap<u32, Object>,
}

#[wasm_bindgen]
impl Processor {
    pub fn start() -> Self {
        let (tx, rx) = async_channel::unbounded();
        let mut message_processor = MessageProcessor::new(WasmRuntime::new());
        let client = WebRTCClient::new(
            "http://192.168.100.124:8080/session".to_string(),
            message_processor.outgoing_byte_reader(),
            message_processor.incoming_byte_sender(),
        );

        let inner_receiver = message_processor.message_receiver().clone();

        let queued_messages = rx;
        let message_sender = message_processor.message_sender().clone();

        let promise = future_to_promise(async move {
            let dispatcher = async move {
                loop {
                    for message in queued_messages.recv().await {
                        tracing::info!("Processor sending message {:?}", message);
                        message_sender.send(message).await.unwrap();
                    }
                }
            };
            join![client.run(), message_processor.run(), dispatcher];
            Ok(JsValue::UNDEFINED)
        });
        Self {
            tx,
            message_receiver: inner_receiver,
            pending_messages: Vec::with_capacity(100),
            position: (50.0, 50.0),
            _promise: promise,
            state: HashMap::new(),
        }
    }

    pub fn get_pending(&mut self) -> JSRustVec {
        for _ in 0..10 {
            match self.message_receiver.try_recv() {
                Ok(message) => match message {
                    Message::Sync => {}
                    Message::Text(txt) => {
                        self.pending_messages.push(txt);
                    }
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
        let my_vec: Array = self.pending_messages.drain(..).map(JsValue::from).collect();
        my_vec.unchecked_into::<JSRustVec>()
    }

    pub fn state(&mut self) -> JsValue {
        self.get_pending();
        serde_wasm_bindgen::to_value(&self.state).unwrap()
    }

    pub fn x(&mut self) -> f32 {
        self.get_pending();
        self.position.0
    }

    pub fn y(&mut self) -> f32 {
        self.get_pending();
        self.position.1
    }

    pub fn send(&self, string: String) {
        tracing::info!("Processor sending message {:?}", string);
        self.tx.try_send(Message::Text(string)).unwrap();
    }
}
