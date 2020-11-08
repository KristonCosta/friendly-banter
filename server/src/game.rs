use std::{collections::HashMap, time::{Duration, Instant}};

use common::message::{GameState, Message, Object, ObjectInfo, Tree};
use rand::Rng;


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
    message_sender: async_channel::Sender<Message>,
    timestep: TimeStep,
    state: HashMap<u32, Object>,
}

impl Universe {
    fn make_tree(id: u32) -> Object {
        let mut rng = rand::thread_rng();
        Object {
            id, 
            object_info: ObjectInfo::Tree(
                Tree {
                    position: (rng.gen_range(0.0, 500.0), rng.gen_range(0.0, 400.0)),
                    size: rng.gen_range(3.0, 10.0),
                }
            )
        }
    }

    pub fn new(sender: async_channel::Sender<Message>) -> Self {
        let mut state = HashMap::new();
        for i in 0..20 {
            state.insert(i, Universe::make_tree(i));
        }
        Self {
            message_sender: sender,
            timestep: TimeStep::new(),
            state
        }
    }

    pub async fn run(&mut self) {
        // I'm not going for efficiency here...
        loop {
            tokio::time::sleep(Duration::from_millis(16)).await;
            for obj in self.state.values() {
                self.message_sender.send(Message::State(obj.clone())).await.unwrap();
            }
            
        }
        

    }
}