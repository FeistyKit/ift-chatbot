mod input;

use std::{collections::HashMap, fmt::format, fs::{self}, sync::{Arc, Mutex, mpsc::channel}};

use tokio::sync::mpsc::UnboundedReceiver;
use twitch_irc::{ClientConfig, SecureTCPTransport, TwitchIRCClient, login::StaticLoginCredentials, message::{ServerMessage, PrivmsgMessage}};

use crate::input::input_thread;

#[allow(unreachable_code, clippy::mutex_atomic)]
#[tokio::main]
async fn main() {
    if !cfg!(debug_assertions) {
        panic!("Don't forget to change to keshy's acct!");
    }
    let (tx_twitch, rx_twitch) = channel::<(String, String)>();
    let (input_handle, rx_input) = input_thread();
    let (mut incoming_messages, client) = prepare_client();
    let default_amount = Arc::new(Mutex::new(None));
    let async_side_default_amount = Arc::clone(&default_amount);
    let map = Arc::new(Mutex::new(HashMap::<String, BetDetails>::new()));
    let leave = Arc::new(Mutex::new(false));
    let async_side_leave = Arc::clone(&leave);
    let async_side_map = Arc::clone(&map);
    let message_handle = tokio::spawn(async move {
        'messages: while let Some(raw_message) = incoming_messages.recv().await {
            if !*async_side_leave.lock().unwrap() {
                break;
            }
            if let ServerMessage::Privmsg(message) = raw_message {
                let default_bet_amount;
                if let Some(amount) = *async_side_default_amount.lock().unwrap() {
                   default_bet_amount = amount;
                } else {
                    continue 'messages;
                }
                let split = message.message_text.trim().split_whitespace().collect::<Vec<_>>();
                let login =message.channel_login;
                let name = message.sender.name;
                if split[0].starts_with("!bet") {
                    if split.len() != 3 {
                        tx_twitch.send((login, format!("{}, {:?} is not a valid use of the bet command!", name.clone(), message.message_text))).unwrap();
                        continue 'messages;
                    }
                    let bet;
                    let choice;
                    if let Ok(their_choice) = split[2].parse::<u8>() {
                        if their_choice != 1 && their_choice != 2 {
                            tx_twitch.send((login, format!("{}, {} is not 1 or 2!", name, split[2]))).unwrap();
                            continue 'messages;
                        }
                        choice = their_choice;
                    } else {
                        tx_twitch.send((login, format!("{}, {} is not a number!", name, split[2]))).unwrap();
                        continue 'messages;
                    }
                    let mut map_handle = async_side_map.lock().unwrap();
                    let map_entry = map_handle.entry(message.sender.id).or_insert_with(|| BetDetails::new(name.clone(), default_bet_amount));
                    if let Ok(their_bet) = split[1].parse::<usize>() {
                       if their_bet > map_entry.bank_amount {
                            tx_twitch.send((login, format!("{}, you cannot bet more than your balance! Your current balance is {}", name, map_entry.bank_amount))).unwrap();
                            continue 'messages;
                       } else {
                           bet = their_bet;
                       }
                    } else {
                        tx_twitch.send((login, format!("{}, {} is not a valid bet!", name, split[1]))).unwrap();
                        continue 'messages;
                    }
                    map_entry.bet_amount = Some(bet);
                    map_entry.number_betted_on = Some(choice);
                }
            }
        }
    });
    client.join("feistyshade".to_string());
    while let Ok(event) = rx_input.recv() {
        while let Ok((login, msg)) = rx_twitch.try_recv() {
            client.say(login, msg).await.unwrap();
        }
        match event {
            input::InputtedCommand::Start { amount } => todo!(),
            input::InputtedCommand::StartFromFile { file, amount } => todo!(),
            input::InputtedCommand::Save { file } => todo!(),
            input::InputtedCommand::EndRound { correct_answer } => todo!(),
        }
    }
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






#[derive(Debug, Hash, PartialEq, Eq)]
struct BetDetails {
    name: String,
    bank_amount: usize,
    bet_amount: Option<usize>,
    number_betted_on: Option<u8>,
    times_betted: u8,
    times_right: u8,
}

impl BetDetails {
    fn is_fresh(&self) -> bool {
        self.bet_amount.is_none() && self.number_betted_on.is_none()
    }
    
    fn set_choices(&mut self, amount: usize, bet: u8) {
        assert!(bet == 1 || bet == 2);
        self.bet_amount = Some(amount);
        self.number_betted_on = Some(bet);
    }
    fn apply(&mut self, right_answer: u8) {
        assert!(right_answer == 1 || right_answer == 2);
        if right_answer == self.number_betted_on.unwrap() {
            self.bank_amount += self.bet_amount.unwrap();
            self.times_right += 1;
        } else {
            self.bank_amount -= self.bet_amount.unwrap();
        }
        self.times_betted += 1;
        self.bet_amount = None;
        self.number_betted_on = None;
    }
    fn new(name: String, amount: usize) -> BetDetails {
        BetDetails {
            name,
            bank_amount: amount,
            bet_amount: None,
            number_betted_on: None,
            times_betted: 0,
            times_right: 0
        }
    }
}
