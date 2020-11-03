

#[derive(Clone)]
pub struct Runtime {}

impl Runtime {
    pub fn new() -> Self {
        Self{}
    }
}

impl turbulence::Runtime for Runtime {
    type Instant = common::runtime::RuntimeInstant;

    type Sleep = tokio::time::Sleep;

    fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static {
        tokio::spawn(future);
    }

    fn now(&self) -> Self::Instant {
        common::runtime::RuntimeInstantHandler::now()
    }

    fn elapsed(&self, instant: Self::Instant) -> std::time::Duration {
        common::runtime::RuntimeInstantHandler::elapsed(instant)
    }

    fn duration_between(&self, earlier: Self::Instant, later: Self::Instant) -> std::time::Duration {
        common::runtime::RuntimeInstantHandler::duration_between(earlier, later)
    }

    fn sleep(&self, duration: std::time::Duration) -> Self::Sleep {
        tokio::time::sleep(duration)
    }
}