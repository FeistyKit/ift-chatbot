mod input;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File},
    hash::Hash,
    io::{Read, Write},
    sync::{mpsc::channel, Arc, Mutex},
};
static LOGIN: String = "feistyshade".to_string();
use tokio::sync::mpsc::UnboundedReceiver;
use twitch_irc::{
    login::StaticLoginCredentials,
    message::{PrivmsgMessage, ServerMessage},
    ClientConfig, SecureTCPTransport, TwitchIRCClient,
};

use crate::input::input_thread;

#[allow(unreachable_code, clippy::mutex_atomic)]
#[tokio::main]
async fn main() {
    if !cfg!(debug_assertions) {
        panic!(
            "Don't forget to change to keshy's twitch channel and implement display of winners!"
        );
    }
    let (tx_twitch, rx_twitch) = channel::<(String, String)>();
    let tx_twitch_here = tx_twitch.clone();
    let (input_handle, rx_input) = input_thread();
    let (mut incoming_messages, client) = prepare_client();
    let default_amount = Arc::new(Mutex::new(None));
    let async_side_default_amount = Arc::clone(&default_amount);
    let map = Arc::new(Mutex::new(HashMap::<String, BetDetails>::new()));
    let leave = Arc::new(Mutex::new(false));
    let async_side_leave = Arc::clone(&leave);
    let async_side_map = Arc::clone(&map);
    let message_handle = tokio::spawn(async move {
        while let Some(raw_message) = incoming_messages.recv().await {
            if *async_side_leave.lock().unwrap() {
                break;
            }
            if let ServerMessage::Privmsg(message) = raw_message {
                handle_priv_msg(
                    async_side_default_amount.clone(),
                    message,
                    tx_twitch.clone(),
                    async_side_map.clone(),
                )
            }
        }
    });
    client.join(LOGIN.clone());
    println!("Bot has connected!");
    'checking: loop {
        while let Ok(event) = rx_input.try_recv() {
            match event {
                input::InputtedCommand::Start { amount } => {
                    *default_amount.lock().unwrap() = Some(amount);
                    *map.lock().unwrap() = HashMap::new();
                    client.say(LOGIN.clone(), format!("The event has started! To bet, say in chat '!bet <amount> <choice>', where the amount is how much you are betting, and the choice is which one you want to bet on! The default bank amount for this round is {}!", amount)).await.unwrap();
                }
                input::InputtedCommand::StartFromFile { file, amount } => {
                    *default_amount.lock().unwrap() = Some(amount);
                    *map.lock().unwrap() = parse_hashmap(file).unwrap_or_default();
                    client.say(LOGIN.clone(), format!("The event has started! To bet, say in chat '!bet <amount> <choice>', where the amount is how much you are betting, and the choice is which one you want to bet on! The default bank amount for this round is {}! That means that if you haven't played in the previous rounds, you get {} points!", amount, amount)).await.unwrap();
                }
                input::InputtedCommand::Save { file } => {
                    if let Err(e) = save_map(Arc::clone(&map), file) {
                        eprintln!("An error occurred while saving the data: {:?}", e);
                    }
                }
                input::InputtedCommand::EndRound { correct_answer } => {
                    map.lock().unwrap().iter_mut().for_each(|(_, details)| {
                        details.apply(correct_answer);
                    });
                    client.say(LOGIN.clone(), format!("The round has ended! Everyone who betted correctly will have the amounts added to their scores, and vice versa! The correct answer was {}", correct_answer)).await.unwrap();
                }
                input::InputtedCommand::Exit => {
                    *leave.lock().unwrap() = true;
                    break 'checking;
                }
            }
        }

        while let Ok((login, msg)) = rx_twitch.try_recv() {
            client.say(login, msg).await.unwrap();
        }
    }
    input_handle.join().unwrap();
    message_handle.await.unwrap();
}

fn save_map(
    map: Arc<Mutex<HashMap<String, BetDetails>>>,
    mut file: File,
) -> Result<(), Box<dyn Error>> {
    let ser = serde_json::to_string(
        &map.lock()
            .unwrap()
            .iter()
            .map(|(id, details)| BetDetailsSerializable::from(details.clone(), id.clone()))
            .collect::<Vec<BetDetailsSerializable>>(),
    )?;
    file.write_all(ser.as_bytes())?;
    println!("Saved!");

    Ok(())
}

fn handle_priv_msg(
    async_side_default_amount: Arc<Mutex<Option<usize>>>,
    message: PrivmsgMessage,
    tx_twitch: std::sync::mpsc::Sender<(String, String)>,
    async_side_map: Arc<Mutex<HashMap<String, BetDetails>>>,
) {
    let default_bet_amount;
    if let Some(amount) = *async_side_default_amount.lock().unwrap() {
        default_bet_amount = amount;
    } else {
        return;
    }
    let split = message
        .message_text
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>();
    let login = message.channel_login;
    let name = message.sender.name;
    if split[0].starts_with("!bet") {
        if split.len() != 3 {
            tx_twitch
                .send((
                    login,
                    format!(
                        "{}, {:?} is not a valid use of the bet command!",
                        name, message.message_text
                    ),
                ))
                .unwrap();
            return;
        }
        let bet;
        let choice;
        if let Ok(their_choice) = split[2].parse::<u8>() {
            if their_choice != 1 && their_choice != 2 {
                tx_twitch
                    .send((login, format!("{}, {} is not 1 or 2!", name, split[2])))
                    .unwrap();
                return;
            }
            choice = their_choice;
        } else {
            tx_twitch
                .send((login, format!("{}, {} is not a number!", name, split[2])))
                .unwrap();
            return;
        }
        let mut map_handle = async_side_map.lock().unwrap();
        let map_entry = map_handle
            .entry(message.sender.id)
            .or_insert_with(|| BetDetails::new(name.clone(), default_bet_amount));
        if !map_entry.is_fresh() {
            return;
        }
        if let Ok(their_bet) = split[1].parse::<usize>() {
            if their_bet > map_entry.bank_amount {
                tx_twitch
                    .send((
                        login,
                        format!(
                            "{}, you cannot bet more than your balance! Your current balance is {}",
                            name, map_entry.bank_amount
                        ),
                    ))
                    .unwrap();
                return;
            } else {
                bet = their_bet;
            }
        } else {
            tx_twitch
                .send((login, format!("{}, {} is not a valid bet!", name, split[1])))
                .unwrap();
            return;
        }
        map_entry.bet_amount = Some(bet);
        map_entry.number_betted_on = Some(choice);
    }
}

fn parse_hashmap(mut file: File) -> Result<HashMap<String, BetDetails>, Box<dyn Error>> {
    let mut buf = String::new();
    file.read_to_string(&mut buf)?;
    let vec: Vec<BetDetailsSerializable> = serde_json::from_str(&buf)?;
    Ok(vec.into_iter().map(|x| x.into()).collect())
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

fn prepare_client() -> (
    UnboundedReceiver<ServerMessage>,
    TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>,
) {
    let raw_credentials = get_from_file();
    let creds = StaticLoginCredentials::new("iftBot".to_string(), Some(raw_credentials.0));
    let config = ClientConfig::new_simple(creds);
    TwitchIRCClient::new(config)
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
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
            times_right: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct BetDetailsSerializable {
    id: String,
    name: String,
    bank_amount: usize,
    bet_amount: Option<usize>,
    number_betted_on: Option<u8>,
    times_betted: u8,
    times_right: u8,
}

impl BetDetailsSerializable {
    fn from(details: BetDetails, id: String) -> BetDetailsSerializable {
        BetDetailsSerializable {
            id,
            name: details.name,
            bank_amount: details.bank_amount,
            bet_amount: details.bet_amount,
            number_betted_on: details.number_betted_on,
            times_betted: details.times_betted,
            times_right: details.times_right,
        }
    }

    fn into(self) -> (String, BetDetails) {
        (
            self.id,
            BetDetails {
                name: self.name,
                bank_amount: self.bank_amount,
                bet_amount: self.bet_amount,
                number_betted_on: self.number_betted_on,
                times_betted: self.times_betted,
                times_right: self.times_right,
            },
        )
    }
}
