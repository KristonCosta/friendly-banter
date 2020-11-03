use std::future::Future;

// This entire file is pretty much just a wrapper around turbulence
// I probably don't need to do this

#[derive(Copy, Clone)]
pub struct RuntimeInstant(instant::Instant);

pub struct RuntimeInstantHandler {}

impl RuntimeInstantHandler {
    pub fn now() -> RuntimeInstant {
        RuntimeInstant(instant::Instant::now())
    }

    pub fn elapsed(instant: RuntimeInstant) -> std::time::Duration {
        instant::Instant::now().duration_since(instant.0)
    }

    pub fn duration_between(earlier: RuntimeInstant, later: RuntimeInstant) -> std::time::Duration {
        later.0.duration_since(earlier.0)
    }
}



pub trait Runtime: Clone + Send + Sync  {
    type Sleep: Future<Output = ()> + Send;

    fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static;

    fn sleep(&self, duration: std::time::Duration) -> Self::Sleep;
}

#[derive(Clone, Copy)]
pub(crate) struct RuntimeImpl<R> 
where
    R: Runtime
{
    runtime: R
}

impl<R> RuntimeImpl<R> 
where 
    R: Runtime
{
    pub fn new(runtime: R) -> Self {
        Self {
            runtime
        }
    }
}

impl<R> turbulence::Runtime for RuntimeImpl<R> 
where 
    R: Runtime
{
    type Instant = RuntimeInstant;

    type Sleep = R::Sleep;

    fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static {
        self.runtime.spawn(future)
    }

    fn now(&self) -> Self::Instant {
        RuntimeInstantHandler::now()
    }

    fn elapsed(&self, instant: Self::Instant) -> instant::Duration {
        RuntimeInstantHandler::elapsed(instant)
    }

    fn duration_between(&self, earlier: Self::Instant, later: Self::Instant) -> instant::Duration {
        RuntimeInstantHandler::duration_between(earlier, later)
    }

    fn sleep(&self, duration: instant::Duration) -> Self::Sleep {
        self.runtime.sleep(duration)
    }
}