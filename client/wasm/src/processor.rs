use crate::{runtime::WasmRuntime, WebRTCClient};

use common::message::{Message, MessageProcessor};
use futures::join;
use js_sys::Array;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::{future_to_promise};

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
}

#[wasm_bindgen]
impl Processor {
    pub fn start() -> Self {
        let (tx, rx) = async_channel::unbounded();
        let mut message_processor = MessageProcessor::new(WasmRuntime::new());
        let client = WebRTCClient::new(
            "http://127.0.0.1:8080/session".to_string(),
            message_processor.outgoing_byte_reader(),
            message_processor.incoming_byte_sender(),
        );

        let inner_receiver = message_processor.message_receiver().clone();

        let queued_messages = rx;
        let message_sender = message_processor.message_sender().clone();

        future_to_promise(async move {
            let dispatcher = async move {
                loop {
                    for message in queued_messages.recv().await {
                        tracing::info!("Processor sending message {:?}", message);
                        message_sender.send(message).await;
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
        }
    }

    pub fn get_pending(&mut self) -> JSRustVec {
        for _ in 0..100 {
            match self.message_receiver.try_recv() {
                Ok(message) => {
                    match message {
                        Message::Sync => {}
                        Message::Text(txt) => {
                            self.pending_messages.push(txt);
                        }
                        Message::Unknown => {}
                    }
                    
                }
                Err(_) => {
                    break;
                }
            }
        }
        let my_vec: Array = self.pending_messages.drain(..).map(JsValue::from).collect();
        my_vec.unchecked_into::<JSRustVec>()
    }

    pub fn send(&self, string: String) {
        tracing::info!("Processor sending message {:?}", string);
        self.tx.try_send(Message::Text(string)).unwrap();
    }
}
