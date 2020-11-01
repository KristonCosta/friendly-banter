use futures::Future;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::future_to_promise;
use crate::common;
#[derive(Clone)]
pub struct Runtime {
}

impl Runtime {
    pub fn new() -> Self {
        Self{}
    }
}

impl turbulence::Runtime for Runtime {
    type Instant = common::RuntimeInstant;

    type Sleep = futures_timer::Delay;

    fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static {
        future_to_promise( async move {
            future.await;
            Ok(JsValue::UNDEFINED)
        });
    }

    fn now(&self) -> Self::Instant {
        common::RuntimeInstantHandler::now()
    }

    fn elapsed(&self, instant: Self::Instant) -> std::time::Duration {
        common::RuntimeInstantHandler::elapsed(instant)
    }

    fn duration_between(&self, earlier: Self::Instant, later: Self::Instant) -> std::time::Duration {
        common::RuntimeInstantHandler::duration_between(earlier, later)
    }

    fn sleep(&self, duration: std::time::Duration) -> Self::Sleep {
        futures_timer::Delay::new(duration)
    }
}