#![feature(async_closure)]
mod client;
mod processor;
mod runtime;
mod utils;

use wasm_bindgen::prelude::*;
#[wasm_bindgen]
pub fn init_panic_hook() {
    utils::set_hooks();
}


