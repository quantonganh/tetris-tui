use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use crossterm::style::Color;
use rusqlite::Connection;
use std::error::Error;
use std::result;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;
use tetris_tui::{
    sqlite::HighScoreRepo, tetromino_width, Cell, Game, Position, Terminal, Tetromino,
    TetrominoSpawner, EMPTY_CELL, I_CELL, NEXT_WIDTH, PLAY_WIDTH,
};

type Result<T> = result::Result<T, Box<dyn Error>>;

struct MockTerminal {
    mock_key_code: Option<Receiver<KeyCode>>,
}

impl MockTerminal {
    pub fn new(mock_key_code: Option<Receiver<KeyCode>>) -> Self {
        MockTerminal { mock_key_code }
    }
}

impl Terminal for MockTerminal {
    fn enable_raw_mode(&self) -> Result<()> {
        Ok(())
    }

    fn enter_alternate_screen(&self) -> Result<()> {
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        Ok(())
    }

    fn write(&self, _foreground_color: Color, _col: u16, _row: u16, _msg: &str) -> Result<()> {
        Ok(())
    }

    fn poll_event(&self, duration: Duration) -> Result<bool> {
        thread::sleep(duration);
        Ok(true)
    }

    fn read_event(&self) -> Result<Event> {
        if let Some(mock_key_code) = &self.mock_key_code {
            if let Ok(code) = mock_key_code.recv() {
                println!("Received: {:?}", code);
                return Ok(Event::Key(KeyEvent {
                    code,
                    modifiers: KeyModifiers::empty(),
                    kind: KeyEventKind::Press,
                    state: KeyEventState::empty(),
                }));
            }
        }

        Ok(Event::Key(KeyEvent {
            code: KeyCode::Null,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }))
    }

    fn leave_alternate_screen(&self) -> Result<()> {
        Ok(())
    }

    fn disable_raw_mode(&self) -> Result<()> {
        Ok(())
    }
}

struct ITetromino;

impl TetrominoSpawner for ITetromino {
    fn spawn(&self, is_next: bool) -> Tetromino {
        let i_tetromino_states: Vec<Vec<Vec<Cell>>> = vec![
            vec![
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![I_CELL, I_CELL, I_CELL, I_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, EMPTY_CELL, I_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, I_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, I_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, I_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![I_CELL, I_CELL, I_CELL, I_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, I_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, I_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, I_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, I_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
        ];

        let tetromino_with = tetromino_width(&i_tetromino_states[0]);

        let mut row = 0;
        let mut col = (PLAY_WIDTH - tetromino_with) as isize / 2;
        if is_next {
            row = 2;
            col = (NEXT_WIDTH - tetromino_with) as isize / 2;
        }

        Tetromino {
            states: i_tetromino_states,
            current_state: 0,
            position: Position { row, col },
        }
    }
}

#[test]
fn clear_lines() -> Result<()> {
    let tetromino_spawner = Box::new(ITetromino);
    let conn = Connection::open_in_memory()?;
    let sqlite_highscore_repository = Box::new(HighScoreRepo { conn });

    let (tx, rx): (Sender<KeyCode>, Receiver<KeyCode>) = channel();
    let (play_grid_tx, play_grid_rx): (Sender<Vec<Vec<Cell>>>, Receiver<Vec<Vec<Cell>>>) =
        channel();
    let mut game = Game::new(
        Box::new(MockTerminal::new(Some(rx))),
        tetromino_spawner,
        sqlite_highscore_repository,
        40,
        20,
        0,
        0,
        None,
        None,
        Some(play_grid_tx),
    )?;

    let receiver = thread::spawn(move || {
        game.start().unwrap();
    });

    // Clear a line by placing 4 I tetrominoes like this ____||____
    // Move the first I tetromino to the left border
    tx.send(KeyCode::Char('h')).unwrap();
    tx.send(KeyCode::Char('h')).unwrap();
    tx.send(KeyCode::Char('h')).unwrap();
    tx.send(KeyCode::Char('j')).unwrap();
    if let Ok(play_grid) = play_grid_rx.recv() {
        for col in 0..4 {
            assert_eq!(play_grid[19][col], I_CELL);
        }
    }

    // // Move the 2nd I tetromino to the right border
    tx.send(KeyCode::Char('l')).unwrap();
    tx.send(KeyCode::Char('l')).unwrap();
    tx.send(KeyCode::Char('l')).unwrap();
    tx.send(KeyCode::Char('j')).unwrap();
    if let Ok(play_grid) = play_grid_rx.recv() {
        for col in 6..10 {
            assert_eq!(play_grid[19][col], I_CELL);
        }
    }

    // Rotate the 3rd I tetromino, move left one column, then hard drop
    tx.send(KeyCode::Char(' ')).unwrap();
    tx.send(KeyCode::Char('h')).unwrap();
    tx.send(KeyCode::Char('j')).unwrap();
    if let Ok(play_grid) = play_grid_rx.recv() {
        for row in 16..20 {
            assert_eq!(play_grid[row][4], I_CELL);
        }
    }

    // Rotate the 4th I tetromino, then hard drop to fill a line
    tx.send(KeyCode::Char(' ')).unwrap();
    tx.send(KeyCode::Char('j')).unwrap();
    if let Ok(play_grid) = play_grid_rx.recv() {
        for col in 0..4 {
            assert_eq!(play_grid[19][col], EMPTY_CELL);
        }
        for col in 6..10 {
            assert_eq!(play_grid[19][col], EMPTY_CELL);
        }
        for row in 18..20 {
            for col in 4..6 {
                assert_eq!(play_grid[row][col], I_CELL);
            }
        }
    }

    tx.send(KeyCode::Char('q')).unwrap();
    tx.send(KeyCode::Char('y')).unwrap();

    receiver.join().unwrap();

    Ok(())
}
