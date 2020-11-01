use serde::{Deserialize, Serialize};
use turbulence::{MessageChannelMode, MessageChannelSettings};

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Sync,
    Text(String),
    Unknown,
}

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

// Stolen from turbulence for now
#[derive(Debug, Copy, Clone)]
pub struct SimpleBufferPool(pub usize);

impl turbulence::BufferPool for SimpleBufferPool {
    type Buffer = Box<[u8]>;

    fn acquire(&self) -> Self::Buffer {
        vec![0; self.0].into_boxed_slice()
    }
}

pub const MESSAGE_SETTINGS: MessageChannelSettings = MessageChannelSettings {
    channel: 1,
    channel_mode: MessageChannelMode::Unreliable,
    message_buffer_size: 8,
    packet_buffer_size: 8,
};


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
