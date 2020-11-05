use common::runtime::Runtime;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::future_to_promise;

#[derive(Clone)]
pub struct WasmRuntime {
}

impl WasmRuntime {
    pub fn new() -> Self {
        Self{}
    }
}

impl Runtime for WasmRuntime {
    
    type Sleep = futures_timer::Delay;

    fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static {
        #[allow(dead_code)]
        let _ = future_to_promise( async move {
            future.await;
            Ok(JsValue::UNDEFINED)
        });
    }

    fn sleep(&self, duration: std::time::Duration) -> Self::Sleep {
        futures_timer::Delay::new(duration)
    }
}