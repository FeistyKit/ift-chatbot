mod input;

use std::{
    collections::HashMap,
    fs::{self},
    sync::{
        Arc, Mutex,
    },
};

use tokio::sync::mpsc::UnboundedReceiver;
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient, login::StaticLoginCredentials, message::ServerMessage};

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

#[tokio::main]
async fn main() {
    if !cfg!(debug_assertions) {
        panic!("Don't forget to change to keshy's acct!");
    }
    let (input_handle, _rx) = input_thread();
    let (mut incoming_messages, client) = prepare_client();
    let message_handle = tokio::spawn(async move {
        while let Some(message) = incoming_messages.recv().await {
            println!("{:?}", message);
        }
    });
    client.join("feistyshade".to_string());
    input_handle.join().unwrap();
    message_handle.await.unwrap();
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


fn prepare_client() -> (UnboundedReceiver<ServerMessage>, TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>) {
    let raw_credentials = get_from_file();
    let creds = StaticLoginCredentials::new("iftBot".to_string(), Some(raw_credentials.0));
    let config = ClientConfig::new_simple(creds);
    TwitchIRCClient::new(config)
}