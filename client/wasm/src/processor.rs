use crate::{Eventer, common::Message};

use js_sys::Array;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::spawn_local;
use std::sync::mpsc::{self, Receiver, Sender};


#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "Array<string>")]
    pub type JSRustVec;
}

#[wasm_bindgen]
pub struct Processor {
    tx: Sender<Message>,
    rx: Receiver<Message>,
    eventer: Eventer,
    pending_messages: Vec<String>,
    event_chunk_size: usize
}

#[wasm_bindgen]
impl Processor {
    pub fn from(eventer: Eventer) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx, 
            rx,
            eventer,
            event_chunk_size: 1024,
            pending_messages: Vec::with_capacity(100)
        }
    }

    pub fn tick(&mut self) -> Result<JsValue, JsValue> {
        for message in self.rx.try_iter() {
            tracing::info!("Processor sending message {:?}", message);
            self.eventer.channels().send(message);
        }

        loop {
            match self.eventer.channels().try_recv::<Message>() {
                Ok(message) => {
                    match message {
                        Some(message) => {
                            tracing::info!("Processor received message {:?}", message);
                            match message {
                                Message::Sync => {}
                                Message::Text(txt) => {
                                    self.pending_messages.push(txt);
                                }
                                Message::Unknown => {}
                            }
                            
                        }, 
                        None => {break}
                    }
                }
                Err(err) => {tracing::error!("{:?}", err)}
            }
        }
        
        Ok(JsValue::TRUE)
    }

    pub fn get_pending(&mut self) -> JSRustVec {
        let my_vec: Array = self.pending_messages.drain(..).map(JsValue::from).collect();
        my_vec.unchecked_into::<JSRustVec>()
        
    }

    pub fn send(&self, string: String)  {
        tracing::info!("Processor sending message {:?}", string);
        self.tx.send(Message::Text(string)).unwrap();
    }
}

