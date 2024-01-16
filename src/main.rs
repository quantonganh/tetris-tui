use core::fmt;
use std::error::Error;
use std::process::exit;
use std::result;

use crossterm::{style::Color, terminal};

use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use clap::Parser;
use local_ip_address::local_ip;

use tetris_tui::{
    open, receive_message, Game, MessageType, RandomTetromino, RealTerminal,
    SqliteHighScoreRepository,
};

const PLAY_WIDTH: usize = 10;
const PLAY_HEIGHT: usize = 20;

const CELL_WIDTH: usize = 3;

const DISTANCE: usize = 6;

const STATS_WIDTH: usize = 18;

const MAX_LEVEL: usize = 20;

#[derive(Clone, Debug, PartialEq)]
struct Cell<'a> {
    symbols: &'a str,
    color: Color,
}

#[derive(Debug)]
struct GameError {
    message: String,
}

impl fmt::Display for GameError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for GameError {}

type Result<T> = result::Result<T, Box<dyn Error>>;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = false)]
    multiplayer: bool,

    #[arg(short, long)]
    server_address: Option<String>,

    /// The number of lines already filled
    #[arg(short, long, default_value_t = 0, verbatim_doc_comment)]
    number_of_lines_already_filled: usize,

    /// Start at level
    #[arg(short, long, default_value_t = 0, verbatim_doc_comment)]
    level: usize,
}

fn main() -> Result<()> {
    let (term_width, term_height) = terminal::size()?;
    let play_width = PLAY_WIDTH * CELL_WIDTH + 2;
    let required_width = (STATS_WIDTH + 2 + DISTANCE) * 2 + play_width;
    let required_height = PLAY_HEIGHT + 2;
    if term_width < required_width as u16 || term_height < required_height as u16 {
        eprintln!(
            "The terminal is too small: {}x{}.\nRequired dimensions are  : {}x{}.",
            term_width, term_height, required_width, required_height
        );
        exit(1);
    }

    let terminal = Box::new(RealTerminal);
    let start_x = (term_width as usize - play_width) / 2;
    let start_y = (term_height as usize - required_height) / 2;

    let conn = open()?;
    let sqlite_highscore_repository = Box::new(SqliteHighScoreRepository { conn });

    let args = Args::parse();
    let number_of_lines_already_filled = args.number_of_lines_already_filled;
    if number_of_lines_already_filled > 10 {
        eprintln!("The number of lines already filled must be less than or equal 10.");
        exit(1);
    }

    let start_at_level = args.level;
    if start_at_level > MAX_LEVEL {
        eprintln!("Level must be between 0 and {}.", MAX_LEVEL);
        exit(1);
    }

    let tetromino_spawner = Box::new(RandomTetromino);
    if args.multiplayer {
        if args.server_address == None {
            let listener = TcpListener::bind("0.0.0.0:8080")?;
            let my_local_ip = local_ip()?;
            println!(
                "Server started. Please invite your competitor to connect to {}.",
                format!("{}:8080", my_local_ip)
            );

            let (stream, _) = listener.accept()?;
            println!("Player 2 connected.");

            let mut stream_clone = stream.try_clone()?;
            let (sender, receiver): (Sender<MessageType>, Receiver<MessageType>) = channel();
            let mut game = Game::new(
                terminal,
                tetromino_spawner,
                sqlite_highscore_repository,
                start_x,
                start_y,
                args.number_of_lines_already_filled,
                args.level,
                Some(stream),
                Some(receiver),
                None,
            )?;

            thread::spawn(move || {
                receive_message(&mut stream_clone, sender);
            });

            game.start()?;
        } else {
            if let Some(server_address) = args.server_address {
                let stream = TcpStream::connect(server_address)?;

                let mut stream_clone = stream.try_clone()?;
                let (sender, receiver): (Sender<MessageType>, Receiver<MessageType>) = channel();
                let mut game = Game::new(
                    terminal,
                    tetromino_spawner,
                    sqlite_highscore_repository,
                    start_x,
                    start_y,
                    number_of_lines_already_filled,
                    start_at_level,
                    Some(stream),
                    Some(receiver),
                    None,
                )?;

                thread::spawn(move || {
                    receive_message(&mut stream_clone, sender);
                });

                game.start()?;
            }
        }
    } else {
        let mut game = Game::new(
            terminal,
            tetromino_spawner,
            sqlite_highscore_repository,
            start_x,
            start_y,
            number_of_lines_already_filled,
            start_at_level,
            None,
            None,
            None,
        )?;
        game.start()?;
    }

    Ok(())
}
