

// Stolen from turbulence for now
#[derive(Debug, Copy, Clone)]
pub struct SimpleBufferPool(pub usize);

impl turbulence::BufferPool for SimpleBufferPool {
    type Buffer = Box<[u8]>;

    fn acquire(&self) -> Self::Buffer {
        vec![0; self.0].into_boxed_slice()
    }
}