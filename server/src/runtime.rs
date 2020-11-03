

#[derive(Clone)]
pub struct NativeRuntime {}

impl NativeRuntime {
    pub fn new() -> Self {
        Self{}
    }
}

impl common::runtime::Runtime for NativeRuntime {
    type Sleep = tokio::time::Sleep;

    fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static {
        tokio::spawn(future);
    }

    fn sleep(&self, duration: std::time::Duration) -> Self::Sleep {
        tokio::time::sleep(duration)
    }
}