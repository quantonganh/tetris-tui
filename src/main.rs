use core::fmt;
use rand::Rng;
use std::error::Error;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use std::{fs, result};

use crossterm::{
    cursor::{self, MoveLeft, MoveRight, MoveTo, RestorePosition, SavePosition},
    event::{poll, read, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    ExecutableCommand,
};

use dirs;
use rusqlite::{params, Connection, Result as RusqliteResult};
use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use clap::Parser;
use local_ip_address::local_ip;

const PLAY_WIDTH: usize = 10;
const PLAY_HEIGHT: usize = 20;

const PREVIEW_WIDTH: usize = 6;
const PREVIEW_HEIGHT: usize = 5;

const SCORE_HEIGHT: usize = 3;
const HELP_HEIGHT: usize = 6;

const MAX_LEVEL: usize = 20;
const LINES_PER_LEVEL: usize = 20;

#[derive(Clone, Debug, PartialEq)]
struct Cell {
    symbols: &'static str,
    color: Color,
}

const SPACE: &str = "   ";
const SQUARE_BRACKETS: &str = "[ ]";
const BLOCK_WIDTH: usize = 3;

const EMPTY_CELL: Cell = Cell {
    symbols: SPACE,
    color: Color::Reset,
};

const BORDER_CELL: Cell = Cell {
    symbols: "---",
    color: Color::White,
};

const LEFT_BORDER_CELL: Cell = Cell {
    symbols: "  |",
    color: Color::White,
};

const RIGHT_BORDER_CELL: Cell = Cell {
    symbols: "|  ",
    color: Color::White,
};

const I_CELL: Cell = Cell {
    symbols: SQUARE_BRACKETS,
    color: Color::Cyan,
};

const O_CELL: Cell = Cell {
    symbols: SQUARE_BRACKETS,
    color: Color::Yellow,
};

const T_CELL: Cell = Cell {
    symbols: SQUARE_BRACKETS,
    color: Color::Rgb {
        r: 207,
        g: 159,
        b: 255,
    },
};

const S_CELL: Cell = Cell {
    symbols: SQUARE_BRACKETS,
    color: Color::Green,
};

const Z_CELL: Cell = Cell {
    symbols: SQUARE_BRACKETS,
    color: Color::Red,
};

const J_CELL: Cell = Cell {
    symbols: SQUARE_BRACKETS,
    color: Color::Blue,
};

const L_CELL: Cell = Cell {
    symbols: SQUARE_BRACKETS,
    color: Color::Rgb {
        r: 255,
        g: 165,
        b: 0,
    },
};

#[derive(Clone)]
struct Position {
    row: usize,
    col: usize,
}

struct Tetromino {
    states: Vec<Vec<Vec<Cell>>>,
    current_state: usize,
    position: Position,
}

impl Clone for Tetromino {
    fn clone(&self) -> Tetromino {
        Tetromino {
            states: self.states.clone(), // Clone the states field
            current_state: self.current_state,
            position: self.position.clone(),
        }
    }
}

struct Player {
    score: u64,
}

const GAME_OVER_MESSAGE: &str = "GAME OVER";
const HIGH_SCORES_MESSAGE: &str = "HIGH SCORES";
const RESTART_COMMAND: &str = "(R)estart | (Q)uit";
const PAUSED_MESSAGE: &str = "PAUSED";
const CONTINUE_COMMAND: &str = "(C)ontinue | (Q)uit";

const MAX_LENGTH_NAME: usize = 12;

const YOU_WIN_MESSAGE: &str = "YOU WIN!";
const RESTART_CONTINUE_COMMAND: &str = "(R)estart | (C)ontinue | (Q)uit";

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

struct Game {
    play_grid: Vec<Vec<Cell>>,
    preview_grid: Vec<Vec<Cell>>,
    current_tetromino: Tetromino,
    next_tetromino: Tetromino,
    start_x: u16,
    start_y: u16,
    lines: u32,
    level: u32,
    score: u32,
    drop_interval: u64,
    conn: Connection,
    paused: bool,
    stream: Option<TcpStream>,
    receiver: Option<Receiver<MessageType>>,
}

impl Game {
    fn new(
        conn: Connection,
        stream: Option<TcpStream>,
        receiver: Option<Receiver<MessageType>>,
    ) -> Self {
        let (term_width, term_height) = terminal::size().unwrap();
        let grid_width = (PLAY_WIDTH + 2) * BLOCK_WIDTH;
        let grid_height = PLAY_HEIGHT + 2;
        let start_x = (term_width - grid_width as u16) / 2;
        let start_y = (term_height - grid_height as u16) / 2;

        let play_grid = create_grid(PLAY_WIDTH, PLAY_HEIGHT);
        let preview_grid = create_grid(PREVIEW_WIDTH, PREVIEW_HEIGHT);

        let current_tetromino = Tetromino::new(false);
        let next_tetromino = Tetromino::new(true);

        Game {
            play_grid,
            preview_grid,
            current_tetromino,
            next_tetromino,
            start_x,
            start_y,
            lines: 0,
            level: 0,
            score: 0,
            drop_interval: 500,
            conn,
            paused: false,
            stream,
            receiver,
        }
    }

    fn start(&mut self) {
        terminal::enable_raw_mode().unwrap();

        let mut stdout = io::stdout();

        execute!(stdout.lock(), cursor::Hide).unwrap();

        self.render(&mut stdout);

        match self.handle_event(&mut stdout) {
            Ok(_) => {}
            Err(err) => eprintln!("Error: {}", err),
        }

        execute!(io::stdout(), cursor::Show).unwrap();
        terminal::disable_raw_mode().unwrap();
    }

    fn reset(&mut self) {
        // Reset play and preview grids
        self.play_grid = create_grid(PLAY_WIDTH, PLAY_HEIGHT);
        self.preview_grid = create_grid(PREVIEW_WIDTH, PREVIEW_HEIGHT);

        // Reset tetrominos
        self.current_tetromino = Tetromino::new(false);
        self.next_tetromino = Tetromino::new(true);

        // Reset game statistics
        self.lines = 0;
        self.level = 0;
        self.score = 0;
        self.drop_interval = 500;

        // Clear any existing messages in the receiver
        if let Some(ref mut receiver) = self.receiver {
            while let Ok(_) = receiver.try_recv() {}
        }

        // Resume the game
        self.paused = false;
    }

    fn render(&self, stdout: &mut std::io::Stdout) {
        stdout.execute(Clear(ClearType::All)).unwrap();

        for (y, row) in self.play_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16;
                render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        for (y, row) in self.preview_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + (x + PLAY_WIDTH + 3) as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16;
                render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        for (y, row) in create_grid(PREVIEW_WIDTH, SCORE_HEIGHT).iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + (x + PLAY_WIDTH + 3) as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + 8;
                render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        for (y, row) in create_grid(PREVIEW_WIDTH, HELP_HEIGHT).iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + (x + PLAY_WIDTH + 3) as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + 14;
                render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        let preview_start_x = self.start_x + (PLAY_WIDTH + 4) as u16 * BLOCK_WIDTH as u16;
        execute!(
            stdout,
            SavePosition,
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 4
            ),
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            Print(format!("Score: {}", self.score)),
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 5
            ),
            Print(format!("Lines: {}", self.lines)),
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 6
            ),
            Print(format!("Level: {}", self.level)),
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 10
            ),
            Print(format!(
                "{:<9}: {:<}",
                String::from("Left"),
                String::from("h, ←")
            )),
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 11
            ),
            Print(format!(
                "{:<9}: {}",
                String::from("Right"),
                String::from("l, →")
            )),
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 12
            ),
            Print(format!(
                "{:<9}: {}",
                String::from("Rotate"),
                String::from("Space")
            )),
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 13
            ),
            Print(format!(
                "{:<9}: {}",
                String::from("Soft Drop"),
                String::from("s, ↑")
            )),
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 14
            ),
            Print(format!(
                "{:<9}: {}",
                String::from("Hard Drop"),
                String::from("j, ↓")
            )),
            MoveTo(
                preview_start_x + 1,
                self.start_y + PREVIEW_HEIGHT as u16 + 15
            ),
            Print(format!(
                "{:<9}: {}",
                String::from("Pause"),
                String::from("p")
            )),
            ResetColor,
            RestorePosition
        )
        .unwrap();
    }

    fn handle_event(&mut self, stdout: &mut std::io::Stdout) -> Result<()> {
        let mut drop_timer = Instant::now();
        let mut soft_drop_timer = Instant::now();

        let mut reset_needed = false;
        loop {
            if self.paused {
                self.handle_pause_event(stdout)?;
            } else {
                if let Some(receiver) = &self.receiver {
                    for message in receiver.try_iter() {
                        match message {
                            MessageType::ClearedRows(rows) => {
                                let cells =
                                    vec![I_CELL, O_CELL, T_CELL, S_CELL, Z_CELL, T_CELL, L_CELL];
                                let mut rng = rand::thread_rng();
                                let random_cell_index = rng.gen_range(0..cells.len());
                                let random_cell = cells[random_cell_index].clone();

                                let mut new_row = vec![LEFT_BORDER_CELL; 1]
                                    .into_iter()
                                    .chain(vec![random_cell; PLAY_WIDTH])
                                    .into_iter()
                                    .chain(vec![RIGHT_BORDER_CELL; 1].into_iter())
                                    .collect::<Vec<Cell>>();
                                let random_column = rng.gen_range(1..=PLAY_WIDTH);
                                new_row[random_column] = EMPTY_CELL;

                                for _ in 0..rows {
                                    self.play_grid.remove(1);
                                    self.play_grid.insert(PLAY_HEIGHT, new_row.clone());
                                }

                                self.render(stdout);
                            }
                            MessageType::Notification(msg) => {
                                self.paused = !self.paused;

                                for (y, row) in create_grid(12, 2).iter().enumerate() {
                                    for (x, &ref cell) in row.iter().enumerate() {
                                        let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16
                                            - BLOCK_WIDTH as u16;
                                        let screen_y = self.start_y + y as u16 + 9;
                                        render_cell(stdout, screen_x, screen_y, cell.clone());
                                    }
                                }

                                execute!(
                                    stdout,
                                    SetForegroundColor(Color::White),
                                    SetBackgroundColor(Color::Black),
                                    SavePosition,
                                    MoveTo(
                                        self.start_x
                                            + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16
                                                - msg.len() as u16)
                                                / 2,
                                        self.start_y + 10
                                    ),
                                    Print(msg),
                                    MoveTo(
                                        self.start_x
                                            + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16
                                                - RESTART_CONTINUE_COMMAND.len() as u16)
                                                / 2,
                                        self.start_y + 11
                                    ),
                                    Print(RESTART_CONTINUE_COMMAND),
                                    ResetColor,
                                    RestorePosition
                                )?;

                                loop {
                                    if poll(Duration::from_millis(10))? {
                                        let event = read()?;
                                        match event {
                                            Event::Key(KeyEvent {
                                                code,
                                                modifiers: _,
                                                kind,
                                                state: _,
                                            }) => {
                                                if kind == KeyEventKind::Press {
                                                    match code {
                                                        KeyCode::Enter | KeyCode::Char('c') => {
                                                            self.render(stdout);
                                                            self.paused = false;
                                                            break;
                                                        }
                                                        KeyCode::Char('r') => {
                                                            reset_needed = true;
                                                            break;
                                                        }
                                                        KeyCode::Char('q') => {
                                                            quit(stdout)?;
                                                        }
                                                        _ => {}
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if reset_needed {
                    reset_game(self, stdout);
                }

                if self.level <= MAX_LEVEL as u32
                    && self.lines >= LINES_PER_LEVEL as u32 * (self.level + 1)
                {
                    self.level += 1;
                    self.drop_interval -= self.drop_interval / 10;
                }

                if drop_timer.elapsed() >= Duration::from_millis(self.drop_interval) {
                    let mut tetromino = self.current_tetromino.clone();
                    let can_move_down = self.can_move(
                        &tetromino,
                        tetromino.position.row as i16 + 1,
                        tetromino.position.col as i16,
                    );

                    if can_move_down {
                        tetromino.move_down(self, stdout);
                        self.current_tetromino = tetromino;
                    } else {
                        self.lock_and_move_to_next(&tetromino, stdout);
                    }

                    self.render_tetromino(stdout);

                    drop_timer = Instant::now();
                }

                if poll(Duration::from_millis(10))? {
                    let event = read()?;
                    match event {
                        Event::Key(KeyEvent {
                            code,
                            state: _,
                            kind,
                            modifiers: _,
                        }) => {
                            if kind == KeyEventKind::Press {
                                let mut tetromino = self.current_tetromino.clone();
                                match code {
                                    KeyCode::Char('h') | KeyCode::Left => {
                                        tetromino.move_left(self, stdout);
                                        self.current_tetromino = tetromino;
                                    }
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        tetromino.move_right(self, stdout);
                                        self.current_tetromino = tetromino;
                                    }
                                    KeyCode::Char(' ') => {
                                        tetromino.rotate(self, stdout);
                                        self.current_tetromino = tetromino;
                                    }
                                    KeyCode::Char('s') | KeyCode::Up => {
                                        if soft_drop_timer.elapsed()
                                            >= (Duration::from_millis(self.drop_interval / 4))
                                        {
                                            let mut tetromino = self.current_tetromino.clone();
                                            if self.can_move(
                                                &tetromino,
                                                tetromino.position.row as i16 + 1,
                                                tetromino.position.col as i16,
                                            ) {
                                                tetromino.move_down(self, stdout);
                                                self.current_tetromino = tetromino;
                                            } else {
                                                self.lock_and_move_to_next(&tetromino, stdout);
                                            }

                                            self.render_tetromino(stdout);

                                            soft_drop_timer = Instant::now();
                                        }
                                    }
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        tetromino.hard_drop(self, stdout);
                                        self.lock_and_move_to_next(&tetromino, stdout);
                                    }
                                    KeyCode::Char('p') => {
                                        self.paused = !self.paused;
                                    }
                                    KeyCode::Char('q') => {
                                        quit(stdout)?;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    self.render_tetromino(stdout);
                }

                if self.is_game_over() {
                    self.handle_game_over(stdout)?;
                }
            }
        }
    }

    fn handle_pause_event(&mut self, stdout: &mut io::Stdout) -> Result<()> {
        let paused_grid = create_grid(8, 2);

        for (y, row) in paused_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16 + BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + 9;
                render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        execute!(
            stdout,
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            SavePosition,
            MoveTo(
                self.start_x
                    + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16 - PAUSED_MESSAGE.len() as u16)
                        / 2,
                self.start_y + 10
            ),
            Print(PAUSED_MESSAGE),
            MoveTo(
                self.start_x
                    + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16
                        - CONTINUE_COMMAND.len() as u16)
                        / 2,
                self.start_y + 11
            ),
            Print(CONTINUE_COMMAND),
            ResetColor,
            RestorePosition
        )
        .unwrap();

        loop {
            if poll(Duration::from_millis(10))? {
                let event = read()?;
                match event {
                    Event::Key(KeyEvent {
                        code,
                        modifiers: _,
                        kind,
                        state: _,
                    }) => {
                        if kind == KeyEventKind::Press {
                            match code {
                                KeyCode::Enter | KeyCode::Char('c') => {
                                    self.render(stdout);
                                    self.paused = false;
                                    break;
                                }
                                KeyCode::Char('q') => {
                                    quit(stdout)?;
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn can_move(&mut self, tetromino: &Tetromino, new_row: i16, new_col: i16) -> bool {
        for (t_row, row) in tetromino.get_cells().iter().enumerate() {
            for (t_col, &ref cell) in row.iter().enumerate() {
                if cell.symbols == SQUARE_BRACKETS {
                    let grid_x = new_col + t_col as i16;
                    let grid_y = new_row + t_row as i16;

                    if grid_x < 1
                        || grid_x > PLAY_WIDTH as i16
                        || grid_y > PLAY_HEIGHT as i16
                        || self.play_grid[grid_y as usize][grid_x as usize].symbols
                            == SQUARE_BRACKETS
                    {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn clear_tetromino(&mut self, stdout: &mut std::io::Stdout) {
        let tetromino = &self.current_tetromino;
        for (row_index, row) in tetromino.states[tetromino.current_state].iter().enumerate() {
            for (col_index, &ref cell) in row.iter().enumerate() {
                let grid_x = tetromino.position.col + col_index as usize;
                let grid_y = tetromino.position.row + row_index as usize;

                if cell.symbols != SPACE {
                    execute!(
                        stdout,
                        SetBackgroundColor(Color::Black),
                        SavePosition,
                        MoveTo(
                            self.start_x + grid_x as u16 * BLOCK_WIDTH as u16,
                            self.start_y + grid_y as u16,
                        ),
                        Print(SPACE),
                        ResetColor,
                        RestorePosition
                    )
                    .unwrap();
                }
            }
        }
    }

    fn lock_and_move_to_next(&mut self, tetromino: &Tetromino, stdout: &mut io::Stdout) {
        self.lock_tetromino(tetromino, stdout);
        self.move_to_next();
    }

    fn lock_tetromino(&mut self, tetromino: &Tetromino, stdout: &mut io::Stdout) {
        for (ty, row) in tetromino.get_cells().iter().enumerate() {
            for (tx, &ref cell) in row.iter().enumerate() {
                if cell.symbols == SQUARE_BRACKETS {
                    let grid_x = tetromino.position.col + tx;
                    let grid_y = tetromino.position.row + ty;

                    self.play_grid[grid_y][grid_x] = cell.clone();
                }
            }
        }

        self.clear_filled_rows(stdout);
    }

    fn move_to_next(&mut self) {
        self.current_tetromino = self.next_tetromino.clone();
        self.current_tetromino.position.row = 1;
        self.current_tetromino.position.col =
            (PLAY_WIDTH + 2 - tetromino_width(&self.current_tetromino.states[0])) / 2;
        self.next_tetromino = Tetromino::new(true);
    }

    fn clear_filled_rows(&mut self, stdout: &mut io::Stdout) {
        let mut filled_rows: Vec<usize> = Vec::new();

        for row_index in (1..=PLAY_HEIGHT).rev() {
            if self.play_grid[row_index][1..=PLAY_WIDTH]
                .iter()
                .all(|cell| cell.symbols == SQUARE_BRACKETS)
            {
                filled_rows.push(row_index);
            }
        }

        let new_row = vec![LEFT_BORDER_CELL; 1]
            .into_iter()
            .chain(vec![EMPTY_CELL; PLAY_WIDTH])
            .into_iter()
            .chain(vec![RIGHT_BORDER_CELL; 1].into_iter())
            .collect::<Vec<Cell>>();
        for &row_index in filled_rows.iter().rev() {
            self.play_grid.remove(row_index);
            self.play_grid.insert(1, new_row.clone());

            self.lines += 1;
        }

        let num_filled_rows = filled_rows.len();
        match num_filled_rows {
            1 => {
                self.score += 100 * (self.level + 1);
            }
            2 => {
                self.score += 300 * (self.level + 1);
            }
            3 => {
                self.score += 500 * (self.level + 1);
            }
            4 => {
                self.score += 800 * (self.level + 1);
            }
            _ => (),
        }

        if let Some(stream) = &mut self.stream {
            if num_filled_rows > 0 {
                send_message(stream, MessageType::ClearedRows(num_filled_rows));
            }
        }

        self.render(stdout);
    }

    fn render_tetromino(&self, stdout: &mut std::io::Stdout) {
        let current_tetromino = &self.current_tetromino;
        for (row_index, row) in current_tetromino.states[current_tetromino.current_state]
            .iter()
            .enumerate()
        {
            for (col_index, &ref cell) in row.iter().enumerate() {
                let grid_x = current_tetromino.position.col + col_index;
                let grid_y = current_tetromino.position.row + row_index;

                if cell.symbols != SPACE {
                    if grid_x >= 1
                        && grid_x <= PLAY_WIDTH
                        && grid_y >= 1
                        && grid_y <= PLAY_HEIGHT
                        && self.play_grid[grid_y][grid_x].symbols == SPACE
                    {
                        execute!(
                            stdout,
                            SavePosition,
                            MoveTo(
                                self.start_x + grid_x as u16 * BLOCK_WIDTH as u16,
                                self.start_y + grid_y as u16
                            ),
                            SetForegroundColor(cell.color),
                            SetBackgroundColor(Color::Black),
                            Print(cell.symbols),
                            ResetColor,
                            RestorePosition,
                        )
                        .unwrap();
                    }
                }
            }
        }

        let next_tetromino = &self.next_tetromino;
        for (row_index, row) in next_tetromino.states[next_tetromino.current_state]
            .iter()
            .enumerate()
        {
            for (col_index, &ref cell) in row.iter().enumerate() {
                let grid_x = next_tetromino.position.col + col_index;
                let grid_y = next_tetromino.position.row + row_index;

                if cell.symbols != SPACE {
                    if grid_x >= 1
                        && grid_x <= PREVIEW_WIDTH
                        && grid_y >= 1
                        && grid_y <= PREVIEW_HEIGHT
                        && self.preview_grid[grid_y][grid_x].symbols == SPACE
                    {
                        execute!(
                            stdout,
                            SavePosition,
                            MoveTo(
                                self.start_x
                                    + (PLAY_WIDTH + 2 + grid_x) as u16 * BLOCK_WIDTH as u16,
                                self.start_y + grid_y as u16
                            ),
                            SetForegroundColor(cell.color),
                            SetBackgroundColor(Color::Black),
                            Print(cell.symbols),
                            ResetColor,
                            RestorePosition,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }

    fn is_game_over(&mut self) -> bool {
        for row in &self.play_grid[1..2] {
            if !self.paused && row.iter().any(|cell| cell.symbols == SQUARE_BRACKETS) {
                return true;
            }
        }
        false
    }

    fn handle_game_over(&mut self, stdout: &mut io::Stdout) -> Result<()> {
        if let Some(stream) = &mut self.stream {
            send_message(
                stream,
                MessageType::Notification(YOU_WIN_MESSAGE.to_string()),
            );
        }

        if self.score == 0 {
            self.show_high_scores(stdout)?;
        } else {
            let count: i64 =
                self.conn
                    .query_row("SELECT COUNT(*) FROM high_scores", params![], |row| {
                        row.get(0)
                    })?;

            if count < 5 {
                self.new_high_score(stdout)?;
            } else {
                let player: Player = self.conn.query_row(
                    "SELECT player_name, score FROM high_scores ORDER BY score DESC LIMIT 4,1",
                    params![],
                    |row| Ok(Player { score: row.get(1)? }),
                )?;

                if (self.score as u64) <= player.score {
                    self.show_high_scores(stdout)?;
                } else {
                    self.new_high_score(stdout)?;
                }
            }
        }

        Ok(())
    }

    fn show_high_scores(&mut self, stdout: &mut io::Stdout) -> Result<()> {
        let game_over_grid = create_grid(PLAY_WIDTH + 2, PLAY_WIDTH);
        let game_over_start_row = 5;

        for (y, row) in game_over_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16 - BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + game_over_start_row;
                render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        execute!(
            stdout,
            SavePosition,
            MoveTo(
                self.start_x
                    + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16
                        - GAME_OVER_MESSAGE.len() as u16)
                        / 2,
                self.start_y + game_over_start_row + 1
            ),
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            Print(GAME_OVER_MESSAGE),
            ResetColor,
            RestorePosition,
        )?;

        let high_scores_grid = create_grid(PLAY_WIDTH, 6);

        for (y, row) in high_scores_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + game_over_start_row + 2;
                render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        execute!(
            stdout,
            SavePosition,
            MoveTo(
                self.start_x
                    + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16
                        - HIGH_SCORES_MESSAGE.len() as u16)
                        / 2,
                self.start_y + game_over_start_row + 3
            ),
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            Print(HIGH_SCORES_MESSAGE),
            ResetColor,
            RestorePosition,
        )?;

        {
            let mut stmt = self.conn.prepare(
                "SELECT player_name, score FROM high_scores ORDER BY score DESC LIMIT 5",
            )?;
            let players = stmt.query_map([], |row| {
                Ok((row.get_unwrap::<_, String>(0), row.get_unwrap::<_, i64>(1)))
            })?;

            for (index, player) in players.enumerate() {
                let (name, score) = player.unwrap();

                execute!(
                    stdout,
                    SavePosition,
                    MoveTo(
                        self.start_x + BLOCK_WIDTH as u16 * 2,
                        self.start_y + index as u16 + game_over_start_row + 4,
                    ),
                    SetForegroundColor(Color::White),
                    SetBackgroundColor(Color::Black),
                    Print(format!(
                        "{:<width$}{:>9}",
                        name,
                        score,
                        width = MAX_LENGTH_NAME + 3
                    )),
                    ResetColor,
                    RestorePosition,
                )?;
            }

            execute!(
                stdout,
                SavePosition,
                MoveTo(
                    self.start_x
                        + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16
                            - RESTART_COMMAND.len() as u16)
                            / 2,
                    self.start_y + game_over_start_row + 10
                ),
                SetForegroundColor(Color::White),
                SetBackgroundColor(Color::Black),
                Print(RESTART_COMMAND),
                ResetColor,
                RestorePosition,
            )?;
        }

        loop {
            if poll(Duration::from_millis(10))? {
                let event = read()?;
                match event {
                    Event::Key(KeyEvent {
                        code,
                        modifiers: _,
                        kind,
                        state: _,
                    }) => {
                        if kind == KeyEventKind::Press {
                            match code {
                                KeyCode::Char('q') => {
                                    quit(stdout)?;
                                }
                                KeyCode::Char('r') => {
                                    reset_game(self, stdout);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn new_high_score(&mut self, stdout: &mut std::io::Stdout) -> Result<()> {
        let new_high_score_grid = create_grid(PLAY_WIDTH + 2, 4);

        for (y, row) in new_high_score_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16 - BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + 8;
                render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        print_message(
            stdout,
            PLAY_WIDTH as u16,
            4,
            0,
            String::from("NEW HIGH SCORE!"),
        );
        print_message(stdout, PLAY_WIDTH as u16, 4, 1, format!("{}", self.score));

        let mut name = String::new();
        let mut cursor_position: usize = 0;

        let (_, term_height) = terminal::size()?;
        stdout.execute(MoveTo(
            self.start_x + BLOCK_WIDTH as u16 + 1,
            (term_height - 3) / 2 + 2,
        ))?;
        stdout.write(format!("Enter your name: {}", name).as_bytes())?;
        stdout.execute(cursor::Show)?;
        stdout.flush()?;

        loop {
            if poll(Duration::from_millis(10))? {
                let event = read()?;
                match event {
                    Event::Key(KeyEvent {
                        code,
                        state: _,
                        kind,
                        modifiers: _,
                    }) => {
                        if kind == KeyEventKind::Press {
                            match code {
                                KeyCode::Backspace => {
                                    // Handle Backspace key to remove characters.
                                    if !name.is_empty() && cursor_position > 0 {
                                        name.remove(cursor_position - 1);
                                        cursor_position -= 1;

                                        stdout.execute(MoveLeft(1))?;
                                        stdout.write(b" ")?;
                                        stdout.flush()?;
                                        print!("{}", &name[cursor_position..]);
                                        stdout.execute(MoveLeft(
                                            name.len() as u16 - cursor_position as u16 + 1,
                                        ))?;
                                        stdout.flush()?;
                                    }
                                }
                                KeyCode::Enter => {
                                    self.conn.execute(
                                        "INSERT INTO high_scores (player_name, score) VALUES (?1, ?2)",
                                        params![name, self.score],
                                    )?;

                                    execute!(stdout.lock(), cursor::Hide)?;
                                    self.show_high_scores(stdout)?;
                                }
                                KeyCode::Left => {
                                    // Move the cursor left.
                                    if cursor_position > 0 {
                                        stdout.execute(MoveLeft(1))?;
                                        cursor_position -= 1;
                                    }
                                }
                                KeyCode::Right => {
                                    // Move the cursor right.
                                    if cursor_position < name.len() {
                                        stdout.execute(MoveRight(1))?;
                                        cursor_position += 1;
                                    }
                                }
                                KeyCode::Char(c) => {
                                    if name.len() <= MAX_LENGTH_NAME {
                                        name.insert(cursor_position, c);
                                        cursor_position += 1;
                                        print!("{}", &name[cursor_position - 1..]);
                                        stdout.flush()?;
                                        for _ in cursor_position..name.len() {
                                            stdout.execute(MoveLeft(1))?;
                                        }
                                        stdout.flush()?;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn reset_game(game: &mut Game, stdout: &mut io::Stdout) {
    game.reset();
    game.render(stdout);

    match game.handle_event(stdout) {
        Ok(_) => {}
        Err(err) => eprintln!("Error: {}", err),
    }
}

fn quit(stdout: &mut io::Stdout) -> Result<()> {
    execute!(stdout, Clear(ClearType::All))?;
    execute!(stdout, cursor::Show)?;
    terminal::disable_raw_mode()?;
    std::process::exit(0);
}

fn create_grid(width: usize, height: usize) -> Vec<Vec<Cell>> {
    let mut grid = vec![vec![EMPTY_CELL; width]; height];
    grid.insert(0, vec![BORDER_CELL; width]);
    grid.push(vec![BORDER_CELL; width]);
    for row in grid.iter_mut() {
        row.insert(0, LEFT_BORDER_CELL);
        row.push(RIGHT_BORDER_CELL);
    }

    grid
}

fn render_cell(stdout: &mut std::io::Stdout, x: u16, y: u16, cell: Cell) {
    execute!(
        stdout,
        SavePosition,
        MoveTo(x, y),
        SetForegroundColor(cell.color),
        SetBackgroundColor(Color::Black),
        Print(cell.symbols),
        ResetColor,
        RestorePosition,
    )
    .unwrap();
}

impl Tetromino {
    fn new(is_next: bool) -> Self {
        let i_tetromino_states: Vec<Vec<Vec<Cell>>> = vec![
            vec![
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![I_CELL, I_CELL, I_CELL, I_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, I_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, I_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, I_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, I_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
        ];

        let o_tetromino_states: Vec<Vec<Vec<Cell>>> =
            vec![vec![vec![O_CELL, O_CELL], vec![O_CELL, O_CELL]]];

        let t_tetromino_states: Vec<Vec<Vec<Cell>>> = vec![
            vec![
                vec![EMPTY_CELL, T_CELL, EMPTY_CELL],
                vec![T_CELL, T_CELL, T_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, T_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, T_CELL, T_CELL],
                vec![EMPTY_CELL, T_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![T_CELL, T_CELL, T_CELL],
                vec![EMPTY_CELL, T_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, T_CELL, EMPTY_CELL],
                vec![T_CELL, T_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, T_CELL, EMPTY_CELL],
            ],
        ];

        let s_tetromino_states: Vec<Vec<Vec<Cell>>> = vec![
            vec![
                vec![EMPTY_CELL, S_CELL, S_CELL],
                vec![S_CELL, S_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, S_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, S_CELL, S_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, S_CELL],
            ],
        ];

        let z_tetromino_states: Vec<Vec<Vec<Cell>>> = vec![
            vec![
                vec![Z_CELL, Z_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, Z_CELL, Z_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, Z_CELL, EMPTY_CELL],
                vec![Z_CELL, Z_CELL, EMPTY_CELL],
                vec![Z_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
        ];

        let j_tetromino_states: Vec<Vec<Vec<Cell>>> = vec![
            vec![
                vec![EMPTY_CELL, J_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, J_CELL, EMPTY_CELL],
                vec![J_CELL, J_CELL, EMPTY_CELL],
            ],
            vec![
                vec![J_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![J_CELL, J_CELL, J_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![J_CELL, J_CELL, EMPTY_CELL],
                vec![J_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![J_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![J_CELL, J_CELL, J_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, J_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
        ];

        let l_tetromino_states: Vec<Vec<Vec<Cell>>> = vec![
            vec![
                vec![L_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![L_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![L_CELL, L_CELL, EMPTY_CELL],
            ],
            vec![
                vec![L_CELL, L_CELL, L_CELL],
                vec![L_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
            ],
            vec![
                vec![L_CELL, L_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, L_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, L_CELL, EMPTY_CELL],
            ],
            vec![
                vec![EMPTY_CELL, EMPTY_CELL, EMPTY_CELL],
                vec![EMPTY_CELL, EMPTY_CELL, L_CELL],
                vec![L_CELL, L_CELL, L_CELL],
            ],
        ];

        let tetromino_states = vec![
            i_tetromino_states.clone(),
            o_tetromino_states.clone(),
            t_tetromino_states.clone(),
            s_tetromino_states.clone(),
            z_tetromino_states.clone(),
            j_tetromino_states.clone(),
            l_tetromino_states.clone(),
        ];

        let mut rng = rand::thread_rng();
        let random_tetromino_index = rng.gen_range(0..tetromino_states.len());

        let states = tetromino_states[random_tetromino_index].clone();
        let tetromino_with = tetromino_width(&states[0]);

        let mut row = 1;
        let mut col = (PLAY_WIDTH + 2 - tetromino_with) / 2;
        if is_next {
            row = 2;
            col = (PREVIEW_WIDTH + 4 - tetromino_with) / 2;
        }

        Tetromino {
            states,
            current_state: 0,
            position: Position { row, col },
        }
    }

    fn get_cells(&self) -> &Vec<Vec<Cell>> {
        &self.states[self.current_state]
    }

    fn move_left(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) {
        if game.can_move(self, self.position.row as i16, self.position.col as i16 - 1) {
            game.clear_tetromino(stdout);
            self.position.col -= 1;
        }
    }

    fn move_right(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) {
        if game.can_move(self, self.position.row as i16, self.position.col as i16 + 1) {
            game.clear_tetromino(stdout);
            self.position.col += 1;
        }
    }

    fn rotate(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) {
        let next_state = (self.current_state + 1) % (self.states.len());

        let mut temp_tetromino = self.clone();
        temp_tetromino.current_state = next_state;

        if game.can_move(
            &temp_tetromino,
            self.position.row as i16,
            self.position.col as i16,
        ) {
            game.clear_tetromino(stdout);
            self.current_state = next_state;
        }
    }

    fn move_down(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) {
        if game.can_move(self, self.position.row as i16 + 1, self.position.col as i16) {
            game.clear_tetromino(stdout);
            self.position.row += 1;
        }
    }

    fn hard_drop(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) {
        while game.can_move(self, self.position.row as i16 + 1, self.position.col as i16) {
            game.clear_tetromino(stdout);
            self.position.row += 1;
        }
    }
}

fn tetromino_width(tetromino: &Vec<Vec<Cell>>) -> usize {
    let mut max_width = 0;

    for row in tetromino.iter() {
        let row_width = row.iter().filter(|&cell| cell.symbols != SPACE).count();
        if row_width > max_width {
            max_width = row_width;
        }
    }

    max_width
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = false)]
    multiplayer: bool,

    #[arg(short, long)]
    server_address: Option<String>,
}

enum MessageType {
    ClearedRows(usize),
    Notification(String),
}

const PREFIX_CLEARED_ROWS: &str = "ClearedRows: ";
const PREFIX_NOTIFICATION: &str = "Notification: ";

fn main() -> Result<()> {
    let conn = open()?;

    let args = Args::parse();
    if args.multiplayer {
        if args.server_address == None {
            let listener = TcpListener::bind("0.0.0.0:8080")?;
            let my_local_ip = local_ip().unwrap();
            println!(
                "Server started. Please invite your competitor to connect to {}.",
                format!("{}:8080", my_local_ip)
            );

            let (stream, _) = listener.accept()?;
            println!("Player 2 connected.");

            let mut stream_clone = stream.try_clone()?;
            let (sender, receiver): (Sender<MessageType>, Receiver<MessageType>) = channel();
            let mut game = Game::new(conn, Some(stream), Some(receiver));

            thread::spawn(move || {
                receive_message(&mut stream_clone, sender);
            });

            game.start();
        } else {
            if let Some(server_address) = args.server_address {
                let stream = TcpStream::connect(server_address)?;

                let mut stream_clone = stream.try_clone()?;
                let (sender, receiver): (Sender<MessageType>, Receiver<MessageType>) = channel();
                let mut game = Game::new(conn, Some(stream), Some(receiver));

                thread::spawn(move || {
                    receive_message(&mut stream_clone, sender);
                });

                game.start();
            }
        }
    } else {
        let mut game = Game::new(conn, None, None);
        game.start();
    }

    Ok(())
}

fn open() -> RusqliteResult<Connection, Box<dyn Error>> {
    let home_dir = match dirs::home_dir() {
        Some(path) => path,
        None => {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to get the user's home directory.",
            )));
        }
    };

    let db_dir = home_dir.join(".tetris");
    if let Err(err) = fs::create_dir_all(db_dir.clone()) {
        return Err(Box::new(err));
    }

    let db_path = db_dir.join("high_scores.db");
    let conn = Connection::open(&db_path)?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS high_scores (
            id INTEGER PRIMARY KEY,
            player_name TEXT,
            score INTEGER,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
        params![],
    )?;

    Ok(conn)
}

fn print_message(stdout: &mut io::Stdout, _grid_width: u16, grid_height: u16, n: u16, msg: String) {
    let (term_width, term_height) = terminal::size().unwrap();
    let start_x = (term_width - msg.len() as u16) / 2;
    let start_y = (term_height - grid_height) / 2;
    execute!(
        stdout,
        SavePosition,
        MoveTo(start_x, start_y + n),
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::Black),
        Print(msg),
        ResetColor,
        RestorePosition,
    )
    .unwrap();
}

fn send_message(stream: &mut TcpStream, message: MessageType) {
    let message_string = match message {
        MessageType::ClearedRows(rows) => format!("{}{}", PREFIX_CLEARED_ROWS, rows),
        MessageType::Notification(msg) => format!("{}{}", PREFIX_NOTIFICATION, msg),
    };

    if let Err(err) = stream.write_all(message_string.as_bytes()) {
        eprintln!("Error sending message: {}", err);
    }
}

fn receive_message(stream: &mut TcpStream, sender: Sender<MessageType>) {
    let mut buffer = [0u8; 256];
    loop {
        match stream.read(&mut buffer) {
            Ok(n) if n > 0 => {
                let msg = String::from_utf8_lossy(&buffer[0..n]);
                if msg.starts_with(PREFIX_CLEARED_ROWS) {
                    if let Ok(rows) = msg.trim_start_matches(PREFIX_CLEARED_ROWS).parse() {
                        sender.send(MessageType::ClearedRows(rows)).unwrap();
                    }
                } else if msg.starts_with(PREFIX_NOTIFICATION) {
                    let msg = msg.trim_start_matches(PREFIX_NOTIFICATION).to_string();
                    sender.send(MessageType::Notification(msg)).unwrap();
                }
            }
            Ok(_) | Err(_) => {
                break;
            }
        }
    }
}
