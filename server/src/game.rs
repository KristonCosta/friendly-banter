use std::time::{Duration, Instant};

use common::message::Message;


type FP = f32;
const MS_PER_UPDATE: FP = 0.5;

#[derive(Debug)]
pub struct TimeStep {
    last_time: Instant,
    delta_time: FP,
    frame_count: u32,
    frame_time: FP,
}

impl TimeStep {
    // https://gitlab.com/flukejones/diir-doom/blob/master/game/src/main.rs
    // Grabbed this from here
    pub fn new() -> TimeStep {
        TimeStep {
            last_time: Instant::now(),
            delta_time: 0.0,
            frame_count: 0,
            frame_time: 0.0,
        }
    }

    pub fn delta(&mut self) -> FP {
        let current_time = Instant::now();
        let delta = current_time.duration_since(self.last_time).as_micros() as FP * 0.001;
        self.last_time = current_time;
        self.delta_time = delta;
        delta
    }

    // provides the framerate in FPS
    pub fn frame_rate(&mut self) -> Option<u32> {
        self.frame_count += 1;
        self.frame_time += self.delta_time;
        let tmp;
        // per second
        if self.frame_time >= 1000.0 {
            tmp = self.frame_count;
            self.frame_count = 0;
            self.frame_time = 0.0;
            return Some(tmp);
        }
        None
    }
}


pub struct Universe {
    position: (f32, f32),
    velocity: (f32, f32),
    boundary: (f32, f32),
    message_sender: async_channel::Sender<Message>,
    timestep: TimeStep,
}

impl Universe {
    pub fn new(sender: async_channel::Sender<Message>) -> Self {
        Self {
            position: (50.0, 50.0),
            velocity: (50.0, 50.0),
            boundary: (500.0, 400.0),
            message_sender: sender,
            timestep: TimeStep::new()
        }
    }

    pub async fn run(&mut self) {
        // I'm not going for efficiency here...
        loop {
            tokio::time::sleep(Duration::from_millis(16)).await;
            let delta = self.timestep.delta();
            self.position.0 += self.velocity.0 * delta * 0.001;
            self.position.1 += self.velocity.1 * delta * 0.001;
            if self.position.0 <= 0.0 {
                self.position.0 = 0.1;
                self.velocity.0 *= -1.0; 
            } else if self.position.0 >= self.boundary.0 {
                self.position.0 = self.boundary.0;
                self.velocity.0 *= -1.0; 
            }
            if self.position.1 <= 0.0 {
                self.position.1 = 0.1;
                self.velocity.1 *= -1.0; 
            } else if self.position.1 >= self.boundary.1 {
                self.position.1 = self.boundary.1;
                self.velocity.1 *= -1.0; 
            }
            self.message_sender.send(Message::Position(self.position.0, self.position.1)).await.unwrap();
        }
        

    }
}