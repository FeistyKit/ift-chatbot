use std::{fs::File, io::stdin, sync::mpsc::{Receiver, Sender, channel}, thread::{Builder, JoinHandle}};

#[derive(Debug)]
pub enum InputtedCommand {
    Start { amount: usize },
    StartFromFile { file: File, amount: usize },
    Save { file: File },
    EndRound { correct_answer: u8 },
}
pub fn input_thread() -> (JoinHandle<()>, Receiver<InputtedCommand>) {
    let (tx, rx) = channel();
    let input_thread = Builder::new().name("Input Thread".to_string()).spawn(move || {
        let help_message = "\
        \nCommands: \n\n\
        help                          -  displays this help message.\n\
        start [amount]                -  starts the program with a new user hashmap and the given amount for each user's starting bank amount.\n\
        startff [file_name] [amount]  -  starts the program with a user hashmap to deserialise from the file name, and the given amount for each user's starting bank amount.\n\
        save [file_name]              -  saves the state to be deserialised with the startff command into the file at the file's name.\n\
        endround [correct_answer]     -  ends the round and does the proper changes for the answers. correct_answer must be 1 or 2. Also prints the top three players, with their scores.\n\
        exit                          -  exits the program.\n";
        println!("This is the IFT chatbot program! Type 'help' into the terminal for a help message!");
        let mut has_asked_to_exit = false;
        loop {
            let raw_inp = input_safe("");
            let inp = raw_inp.trim().split_whitespace().collect::<Vec<_>>();
            match inp[0].to_lowercase().as_str() {
                "help" => println!("{}", help_message),
                "start" => {if start_command(&inp, &tx) {
                        continue;
                    }},
                "startff" => {
                    if startff(&inp, &tx) {
                        continue;
                    }
                },
                "save" => {
                    if save(&inp, &tx) {
                        continue;
                    }
                },
                "endround" => {
                    if endround(inp, &tx) {
                        continue;
                    }
                },
                "exit" => {
                    if !has_asked_to_exit {
                        println!("Are you sure you have saved and want to exit? If so, type 'exit' again.");
                        has_asked_to_exit = true;
                        continue;
                    } else {
                        break;
                    }
                }
                _ => println!("That's not a valid command! For the valid commands, type 'help' into here!")
            }
            has_asked_to_exit = false;
        }
    }).unwrap();
    (input_thread, rx)
}

fn endround(inp: Vec<&str>, tx: &Sender<InputtedCommand>) -> bool {
    if inp.len() < 2 {
        println!("Not enough arguments!");
        return true;
    }
    if let Ok(amt) = inp[1].parse::<u8>() {
       if amt == 1 || amt == 2 {
           tx.send(InputtedCommand::EndRound{correct_answer: amt}).unwrap();
       } else {
           println!("{} is not 1 or 2!", inp[1]);
       }
                       }  else {
       println!("{} is not a valid argument!", inp[1]);
                       }
    false
}

fn save(inp: &[&str], tx: &Sender<InputtedCommand>) -> bool {
    if inp.len() < 2 {
        println!("Not enough arguments!");
        return true;
    }
    if let Ok(file) = File::create(inp[1])  {
        tx.send(InputtedCommand::Save{file}).unwrap();
    } else {
        println!("Couldn't create a file with name {} here!", inp[1]);
    }
    false
}

fn startff(inp: &[&str], tx: &Sender<InputtedCommand>) -> bool {
    if inp.len() < 3 {
        println!("Not enough arguments!");
        return true;
    }
    if let Ok(amt) = inp[2].parse::<usize>() {
        if let Ok(file) = File::open(inp[1]) {
            tx.send(InputtedCommand::StartFromFile{file, amount: amt}).unwrap();
        } else {
            println!("Could not open file with name {} here!", inp[1]);
        }
    } else {
        println!("Invalid number argument!")
    }
    false
}

fn start_command(inp: &[&str], tx: &Sender<InputtedCommand>) -> bool {
    if inp.len() < 2 {
        println!("Not enough arguments!");
        return true;
    }
    if let Ok(amt) = inp[1].parse::<usize>() {
        println!("Started!");
        tx.send(InputtedCommand::Start { amount: amt }).unwrap();
    } else {
        println!("{} is an invalid argument!", inp[1]);
    }
    false
}

fn input(m: &str) -> Result<String, std::io::Error> {
    if !m.is_empty() {
        println!("{}", m);
    }
    let mut s = String::new();
    stdin().read_line(&mut s)?;
    Ok(s)
}

fn input_safe(m: &str) -> String {
    match input(m) {
        Ok(yay) => yay,
        Err(_) => input_safe("That's an invalid input!"),
    }
}