mod input;

use std::{
    collections::HashMap,
    fs::{self},
    sync::{
        Arc, Mutex,
    },
};

use crate::input::input_thread;
type ThreadedMap = Arc<Mutex<HashMap<String, BetDetails>>>;

#[derive(Debug, Hash, PartialEq, Eq)]
struct BetDetails {
    name: String,
    bank_amount: usize,
    betted_amount: Option<usize>,
    number_betted_on: Option<u8>,
    times_betted: u8,
    times_right: u8,
}

fn main() {
    let (handle, rx) = input_thread();
    while let Ok(event) = rx.recv() {
        println!("Event received: {:?}", event);
    }
    handle.join().unwrap();
}

fn get_from_file() -> (String, String) {
    let raw = fs::read_to_string("authentication.txt").expect("Could not open authentication.txt!");
    let lines = raw.lines().collect::<Vec<_>>();
    assert!(
        lines[0].contains("oauth:"),
        "First part of lines is not proper oauth identification!"
    );
    (lines[0].replace("oauth:", ""), lines[1].to_string())
}
