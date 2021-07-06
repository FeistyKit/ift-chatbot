mod input;
use std::{
    cell::RefCell,
    collections::HashMap,
    error::Error,
    fs::{self, File},
    hash::Hash,
    io::Write,
    rc::Rc,
    sync::{mpsc::channel, Arc, Mutex},
};
static LOGIN: &str = "feistyshade";
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
    let (
        tx_twitch,
        rx_twitch,
        input_handle,
        rx_input,
        mut incoming_messages,
        client,
        default_amount,
        async_side_default_amount,
        map,
        leave,
        async_side_leave,
        async_side_map,
    ) = prepare_stuff();
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
    client.join("keshysushi".to_string());
    let client = Rc::new(RefCell::new(client));
    println!("Bot has connected!");
    loop {
        if check(
            &rx_input,
            Arc::clone(&default_amount),
            Arc::clone(&map),
            Rc::clone(&client),
            Arc::clone(&leave),
            &rx_twitch,
        )
        .await
        {
            break;
        }
    }
    input_handle.join().unwrap();
    message_handle.await.unwrap();
}

async fn check(
    rx_input: &std::sync::mpsc::Receiver<input::InputtedCommand>,
    default_amount: Arc<Mutex<Option<usize>>>,
    map: Arc<Mutex<HashMap<String, BetDetails>>>,
    client: Rc<
        RefCell<
            TwitchIRCClient<
                twitch_irc::transport::tcp::TCPTransport<twitch_irc::transport::tcp::TLS>,
                StaticLoginCredentials,
            >,
        >,
    >,
    leave: Arc<Mutex<bool>>,
    rx_twitch: &std::sync::mpsc::Receiver<(String, String)>,
) -> bool {
    while let Ok(event) = rx_input.try_recv() {
        match event {
            input::InputtedCommand::Start { amount } => {
                *default_amount.lock().unwrap() = Some(amount);
                *map.lock().unwrap() = HashMap::new();
                client.borrow().say(LOGIN.to_string(), format!("The event has started! To bet, say in chat '!bet <amount> <choice>', where the amount is how much you are betting, and the choice is which one you want to bet on! The default bank amount for this round is {}!", amount)).await.unwrap();
                println!("Started!");
            }
            input::InputtedCommand::Save { file } => {
                if let Err(e) = save_map(Arc::clone(&map), file, Arc::clone(&default_amount)) {
                    eprintln!("An error occurred while saving the data: {:?}", e);
                }
            }
            input::InputtedCommand::EndRound { correct_answer } => {
                map.lock().unwrap().iter_mut().for_each(|(_, details)| {
                    details.apply(correct_answer);
                });
                client.borrow().say(LOGIN.to_string(), format!("The round has ended! Everyone who betted correctly will have the amounts added to their scores, and vice versa! The correct answer was {}.", correct_answer)).await.unwrap();
                println!("Round ended!");
            }
            input::InputtedCommand::Exit => {
                *leave.lock().unwrap() = true;
                return true;
            }
        }
    }
    while let Ok((login, msg)) = rx_twitch.try_recv() {
        client.borrow().say(login, msg).await.unwrap();
    }
    false
}
#[allow(
    clippy::type_complexity,
    clippy::too_many_arguments,
    clippy::mutex_atomic
)]
fn prepare_stuff() -> (
    std::sync::mpsc::Sender<(String, String)>,
    std::sync::mpsc::Receiver<(String, String)>,
    std::thread::JoinHandle<()>,
    std::sync::mpsc::Receiver<input::InputtedCommand>,
    UnboundedReceiver<ServerMessage>,
    TwitchIRCClient<
        twitch_irc::transport::tcp::TCPTransport<twitch_irc::transport::tcp::TLS>,
        StaticLoginCredentials,
    >,
    Arc<Mutex<Option<usize>>>,
    Arc<Mutex<Option<usize>>>,
    Arc<Mutex<HashMap<String, BetDetails>>>,
    Arc<Mutex<bool>>,
    Arc<Mutex<bool>>,
    Arc<Mutex<HashMap<String, BetDetails>>>,
) {
    let (tx_twitch, rx_twitch) = channel::<(String, String)>();
    let (input_handle, rx_input) = input_thread();
    let (incoming_messages, client) = prepare_client();
    let default_amount = Arc::new(Mutex::new(None));
    let async_side_default_amount = Arc::clone(&default_amount);
    let map = Arc::new(Mutex::new(HashMap::<String, BetDetails>::new()));
    let leave = Arc::new(Mutex::new(false));
    let async_side_leave = Arc::clone(&leave);
    let async_side_map = Arc::clone(&map);
    (
        tx_twitch,
        rx_twitch,
        input_handle,
        rx_input,
        incoming_messages,
        client,
        default_amount,
        async_side_default_amount,
        map,
        leave,
        async_side_leave,
        async_side_map,
    )
}

fn save_map(
    map: Arc<Mutex<HashMap<String, BetDetails>>>,
    mut file: File,
    default_amount: Arc<Mutex<Option<usize>>>,
) -> Result<(), Box<dyn Error>> {
    let string = map
        .lock()
        .unwrap()
        .values()
        .map(|x| {
            if x.bank_amount > default_amount.lock().unwrap().unwrap() {
                format!(
                    "{} gained {} points!\n",
                    x.name.clone(),
                    x.bank_amount - default_amount.lock().unwrap().unwrap()
                )
            } else {
                format!(
                    "{} lost {} points!\n",
                    x.name.clone(),
                    default_amount.lock().unwrap().unwrap() - x.bank_amount
                )
            }
        })
        .collect::<String>();
    file.write_all(string.as_bytes())?;
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
