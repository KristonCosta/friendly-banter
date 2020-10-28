use crate::Eventer;
use wasm_bindgen::{prelude::*, JsCast};


#[wasm_bindgen]
#[derive(Clone)]
pub struct Processor {
    eventer: Eventer
}

#[wasm_bindgen]
impl Processor {
    pub fn from(eventer: Eventer) -> Self {
        Self {
            eventer
        }
    }

    pub fn copy(&self) -> Self {
        self.clone()
    }

    pub async fn tick(self) -> Result<JsValue, JsValue> {
        let data = self.eventer.incoming().try_recv();
        match data {
            Ok(event) => {
                match event {
                    crate::Event::String(msg) => {
                        tracing::info!("Processor received message {}", msg);
                    }
                }
            }
            Err(_) => {}
        };
        self.send().await
    }

    pub async fn send(self) -> Result<JsValue, JsValue> {
        let sender = self.eventer.outgoing().clone();
        sender.send(crate::Message::Sync).await.unwrap();
        Ok(JsValue::TRUE)
    }
}

