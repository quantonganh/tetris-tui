use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc::Sender;

pub enum MessageType {
    ClearedRows(usize),
    Notification(String),
}

pub const PREFIX_CLEARED_ROWS: &str = "ClearedRows: ";
pub const PREFIX_NOTIFICATION: &str = "Notification: ";

pub fn send_to_other_player(stream: &mut TcpStream, message: MessageType) {
    let message_string = match message {
        MessageType::ClearedRows(rows) => format!("{}{}", PREFIX_CLEARED_ROWS, rows),
        MessageType::Notification(msg) => format!("{}{}", PREFIX_NOTIFICATION, msg),
    };

    if let Err(err) = stream.write_all(message_string.as_bytes()) {
        eprintln!("Error writing message: {}", err);
    }
}

pub fn forward_to_main_thread(stream: &mut TcpStream, sender: Sender<MessageType>) {
    let mut buffer = [0u8; 256];
    loop {
        match stream.read(&mut buffer) {
            Ok(n) if n > 0 => {
                let msg = String::from_utf8_lossy(&buffer[0..n]);
                if msg.starts_with(PREFIX_CLEARED_ROWS) {
                    if let Ok(rows) = msg.trim_start_matches(PREFIX_CLEARED_ROWS).parse() {
                        if let Err(err) = sender.send(MessageType::ClearedRows(rows)) {
                            eprintln!("Error sending number of cleared rows: {}", err)
                        }
                    }
                } else if msg.starts_with(PREFIX_NOTIFICATION) {
                    let msg = msg.trim_start_matches(PREFIX_NOTIFICATION).to_string();
                    if let Err(err) = sender.send(MessageType::Notification(msg)) {
                        eprintln!("Error sending notification message: {}", err)
                    }
                }
            }
            Ok(_) | Err(_) => {
                break;
            }
        }
    }
}
