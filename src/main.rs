use core::fmt;
use rand::Rng;
use std::error::Error;
use std::io::{self, Write};
use std::process::exit;
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

const DISTANCE: u16 = 6;

const NEXT_WIDTH: usize = 6;
const NEXT_HEIGHT: usize = 5;

const STATS_WIDTH: u16 = 18;

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

const ENTER_YOUR_NAME_MESSAGE: &str = "Enter your name: ";
const MAX_NAME_LENGTH: usize = 12;

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

struct MultiplayerScore {
    my_score: u8,
    competitor_score: u8,
}

struct Game {
    play_grid: Vec<Vec<Cell>>,
    next_grid: Vec<Vec<Cell>>,
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
    multiplayer_score: MultiplayerScore,
}

impl Game {
    fn new(
        conn: Connection,
        stream: Option<TcpStream>,
        receiver: Option<Receiver<MessageType>>,
    ) -> Result<Self> {
        let (term_width, term_height) = terminal::size()?;
        let grid_width = (PLAY_WIDTH + 2) * BLOCK_WIDTH;
        let grid_height = PLAY_HEIGHT + 2;
        let required_width =
            (STATS_WIDTH + 2 + DISTANCE) * 2 + PLAY_WIDTH as u16 * BLOCK_WIDTH as u16 + 2;
        if term_width < required_width as u16 || term_height < grid_height as u16 {
            eprintln!(
                "The terminal is too small: {}x{}.\nRequired dimensions are  : {}x{}.",
                term_width, term_height, required_width, grid_height
            );
            exit(1);
        }
        let start_x = (term_width - grid_width as u16) / 2;
        let start_y = (term_height - grid_height as u16) / 2;

        let play_grid = create_grid(PLAY_WIDTH, PLAY_HEIGHT);
        let next_grid = create_grid(NEXT_WIDTH, NEXT_HEIGHT);

        let current_tetromino = Tetromino::new(false);
        let next_tetromino = Tetromino::new(true);

        Ok(Game {
            play_grid,
            next_grid,
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
            multiplayer_score: MultiplayerScore {
                my_score: 0,
                competitor_score: 0,
            },
        })
    }

    fn start(&mut self) -> Result<()> {
        terminal::enable_raw_mode()?;

        let mut stdout = io::stdout();

        execute!(stdout.lock(), cursor::Hide)?;

        self.render(&mut stdout)?;

        match self.handle_event(&mut stdout) {
            Ok(_) => {}
            Err(err) => eprintln!("Error: {}", err),
        }

        execute!(io::stdout(), cursor::Show)?;
        terminal::disable_raw_mode()?;

        Ok(())
    }

    fn reset(&mut self) {
        // Reset play and preview grids
        self.play_grid = create_grid(PLAY_WIDTH, PLAY_HEIGHT);
        self.next_grid = create_grid(NEXT_WIDTH, NEXT_HEIGHT);

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

    fn render(&self, stdout: &mut std::io::Stdout) -> Result<()> {
        stdout.execute(Clear(ClearType::All))?;

        for (y, row) in self.play_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16;
                render_cell(stdout, screen_x, screen_y, cell.clone())?;
            }
        }

        let next_start_x = self.start_x + PLAY_WIDTH as u16 * BLOCK_WIDTH as u16 + 3 + DISTANCE;

        render_frame(
            stdout,
            "Next",
            next_start_x,
            self.start_y,
            NEXT_WIDTH as u16 * 3,
            NEXT_HEIGHT as u16 + 1,
        )?;

        let stats_start_x = self.start_x - STATS_WIDTH - DISTANCE + 1;
        print_left_aligned_messages(
            stdout,
            "Stats",
            Some(STATS_WIDTH.into()),
            stats_start_x,
            self.start_y + 1,
            vec![
                "",
                format!("Score: {}", self.score).as_str(),
                format!("Lines: {}", self.lines).as_str(),
                format!("Level: {}", self.level).as_str(),
                "",
            ],
        )?;

        if let Some(_) = &self.stream {
            print_left_aligned_messages(
                stdout,
                "2-Player",
                Some(STATS_WIDTH.into()),
                stats_start_x,
                self.start_y + 9,
                vec![
                    "",
                    format!(
                        "Score: {} - {}",
                        self.multiplayer_score.my_score, self.multiplayer_score.competitor_score,
                    )
                    .as_str(),
                    "",
                ],
            )?;
        }

        print_left_aligned_messages(
            stdout,
            "Help",
            None,
            next_start_x,
            self.start_y + NEXT_HEIGHT as u16 + 7,
            vec![
                "",
                "Left: h, ←",
                "Right: l, →",
                "Rotate: Space",
                "Soft Drop: s, ↑",
                "Hard Drop: j, ↓",
                "Pause: p",
                "Quit: q",
                "",
            ],
        )?;

        Ok(())
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

                                self.render(stdout)?;
                            }
                            MessageType::Notification(msg) => {
                                self.paused = !self.paused;

                                print_centered_messages(
                                    stdout,
                                    None,
                                    vec![&msg, "", "(R)estart | (C)ontinue | (Q)uit"],
                                )?;

                                self.multiplayer_score.my_score += 1;

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
                                                            self.render(stdout)?;
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
                    reset_game(self, stdout)?;
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
                        tetromino.move_down(self, stdout)?;
                        self.current_tetromino = tetromino;
                    } else {
                        self.lock_and_move_to_next(&tetromino, stdout)?;
                    }

                    self.render_tetromino(stdout)?;

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
                                        tetromino.move_left(self, stdout)?;
                                        self.current_tetromino = tetromino;
                                    }
                                    KeyCode::Char('l') | KeyCode::Right => {
                                        tetromino.move_right(self, stdout)?;
                                        self.current_tetromino = tetromino;
                                    }
                                    KeyCode::Char(' ') => {
                                        tetromino.rotate(self, stdout)?;
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
                                                tetromino.move_down(self, stdout)?;
                                                self.current_tetromino = tetromino;
                                            } else {
                                                self.lock_and_move_to_next(&tetromino, stdout)?;
                                            }

                                            self.render_tetromino(stdout)?;

                                            soft_drop_timer = Instant::now();
                                        }
                                    }
                                    KeyCode::Char('j') | KeyCode::Down => {
                                        tetromino.hard_drop(self, stdout)?;
                                        self.lock_and_move_to_next(&tetromino, stdout)?;
                                    }
                                    KeyCode::Char('p') => {
                                        self.paused = !self.paused;
                                    }
                                    KeyCode::Char('q') => {
                                        self.handle_quit_event(stdout)?;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                    self.render_tetromino(stdout)?;
                }

                if self.is_game_over() {
                    self.handle_game_over(stdout)?;
                }
            }
        }
    }

    fn handle_pause_event(&mut self, stdout: &mut io::Stdout) -> Result<()> {
        print_centered_messages(stdout, None, vec!["PAUSED", "", "(C)ontinue | (Q)uit"])?;

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
                                    self.render(stdout)?;
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

    fn handle_quit_event(&mut self, stdout: &mut io::Stdout) -> Result<()> {
        print_centered_messages(stdout, None, vec!["QUIT?", "", "(Y)es | (N)o"])?;

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
                                KeyCode::Enter | KeyCode::Char('y') => {
                                    quit(stdout)?;
                                }
                                KeyCode::Esc | KeyCode::Char('n') => {
                                    self.render(stdout)?;
                                    self.paused = false;
                                    break;
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

    fn clear_tetromino(&mut self, stdout: &mut std::io::Stdout) -> Result<()> {
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
                    )?;
                }
            }
        }

        Ok(())
    }

    fn lock_and_move_to_next(
        &mut self,
        tetromino: &Tetromino,
        stdout: &mut io::Stdout,
    ) -> Result<()> {
        self.lock_tetromino(tetromino, stdout)?;
        self.move_to_next();

        Ok(())
    }

    fn lock_tetromino(&mut self, tetromino: &Tetromino, stdout: &mut io::Stdout) -> Result<()> {
        for (ty, row) in tetromino.get_cells().iter().enumerate() {
            for (tx, &ref cell) in row.iter().enumerate() {
                if cell.symbols == SQUARE_BRACKETS {
                    let grid_x = tetromino.position.col + tx;
                    let grid_y = tetromino.position.row + ty;

                    self.play_grid[grid_y][grid_x] = cell.clone();
                }
            }
        }

        self.clear_filled_rows(stdout)?;

        Ok(())
    }

    fn move_to_next(&mut self) {
        self.current_tetromino = self.next_tetromino.clone();
        self.current_tetromino.position.row = 1;
        self.current_tetromino.position.col =
            (PLAY_WIDTH + 2 - tetromino_width(&self.current_tetromino.states[0])) / 2;
        self.next_tetromino = Tetromino::new(true);
    }

    fn clear_filled_rows(&mut self, stdout: &mut io::Stdout) -> Result<()> {
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

        self.render(stdout)?;

        Ok(())
    }

    fn render_tetromino(&self, stdout: &mut std::io::Stdout) -> Result<()> {
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
                        )?;
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
                        && grid_x <= NEXT_WIDTH
                        && grid_y >= 1
                        && grid_y <= NEXT_HEIGHT
                        && self.next_grid[grid_y][grid_x].symbols == SPACE
                    {
                        execute!(
                            stdout,
                            SavePosition,
                            MoveTo(
                                self.start_x
                                    + (PLAY_WIDTH + grid_x) as u16 * BLOCK_WIDTH as u16
                                    + 4
                                    + tetromino_width(
                                        &next_tetromino.states[next_tetromino.current_state]
                                    ) as u16
                                        % 2,
                                self.start_y + grid_y as u16
                            ),
                            SetForegroundColor(cell.color),
                            SetBackgroundColor(Color::Black),
                            Print(cell.symbols),
                            ResetColor,
                            RestorePosition,
                        )?;
                    }
                }
            }
        }

        Ok(())
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
            send_message(stream, MessageType::Notification("YOU WIN!".to_string()));
            self.multiplayer_score.competitor_score += 1;
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
        print_centered_messages(
            stdout,
            Some((PLAY_WIDTH + 2) * BLOCK_WIDTH).into(),
            vec![
                "GAME OVER",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "(R)estart | (Q)uit",
            ],
        )?;

        {
            let mut stmt = self.conn.prepare(
                "SELECT player_name, score FROM high_scores ORDER BY score DESC LIMIT 5",
            )?;
            let players = stmt.query_map([], |row| {
                Ok((row.get_unwrap::<_, String>(0), row.get_unwrap::<_, i64>(1)))
            })?;

            let mut players_str: Vec<String> = Vec::new();
            for (_, player) in players.enumerate() {
                let (name, score) = player?;
                let formatted_str =
                    format!("{:<width$}{:>9}", name, score, width = MAX_NAME_LENGTH + 3);

                players_str.push(formatted_str)
            }

            players_str.insert(0, "HIGH SCORES".to_string());

            print_centered_messages(
                stdout,
                None,
                players_str.iter().map(|s| s.as_str()).collect(),
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
                                    reset_game(self, stdout)?;
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
        print_centered_messages(
            stdout,
            None,
            vec![
                "NEW HIGH SCORE!",
                &self.score.to_string(),
                &format!("{}{}", ENTER_YOUR_NAME_MESSAGE, " ".repeat(MAX_NAME_LENGTH)),
            ],
        )?;

        let mut name = String::new();
        let mut cursor_position: usize = 0;

        let (term_width, term_height) = terminal::size()?;
        stdout.execute(MoveTo(
            (term_width - ENTER_YOUR_NAME_MESSAGE.len() as u16 - MAX_NAME_LENGTH as u16) / 2
                + ENTER_YOUR_NAME_MESSAGE.len() as u16,
            term_height / 2 - 3 / 2 + 2,
        ))?;
        stdout.write(format!("{}", name).as_bytes())?;
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
                                    if name.len() < MAX_NAME_LENGTH {
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

fn reset_game(game: &mut Game, stdout: &mut io::Stdout) -> Result<()> {
    game.reset();
    game.render(stdout)?;

    game.handle_event(stdout)?;

    Ok(())
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

fn render_frame(
    stdout: &mut io::Stdout,
    title: &str,
    start_x: u16,
    start_y: u16,
    width: u16,
    height: u16,
) -> Result<()> {
    // Print the top border
    let left = (width - title.len() as u16 - 2) / 2;
    execute!(
        stdout,
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::Black),
        SavePosition,
        MoveTo(start_x, start_y),
        Print(format!(
            "|{} {} {}|",
            "-".repeat(left as usize),
            title,
            "-".repeat(width as usize - left as usize - title.len() - 2)
        )),
        ResetColor,
        RestorePosition,
    )?;

    // Print the left and right borders
    for index in 1..height {
        execute!(
            stdout,
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            SavePosition,
            MoveTo(start_x, start_y + index as u16),
            Print(format!("|{}|", " ".repeat(width as usize))),
            ResetColor,
            RestorePosition,
        )?;
    }

    // Print the bottom border
    execute!(
        stdout,
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::Black),
        SavePosition,
        MoveTo(start_x, start_y + height),
        Print(format!("{}{}{}", "|", ("-").repeat(width as usize), "|")),
        ResetColor,
        RestorePosition,
    )?;

    stdout.flush()?;

    Ok(())
}

fn render_cell(stdout: &mut std::io::Stdout, x: u16, y: u16, cell: Cell) -> Result<()> {
    execute!(
        stdout,
        SavePosition,
        MoveTo(x, y),
        SetForegroundColor(cell.color),
        SetBackgroundColor(Color::Black),
        Print(cell.symbols),
        ResetColor,
        RestorePosition,
    )?;

    Ok(())
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
            col = (NEXT_WIDTH + 4 - tetromino_with) / 2;
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

    fn move_left(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) -> Result<()> {
        if game.can_move(self, self.position.row as i16, self.position.col as i16 - 1) {
            game.clear_tetromino(stdout)?;
            self.position.col -= 1;
        }

        Ok(())
    }

    fn move_right(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) -> Result<()> {
        if game.can_move(self, self.position.row as i16, self.position.col as i16 + 1) {
            game.clear_tetromino(stdout)?;
            self.position.col += 1;
        }

        Ok(())
    }

    fn rotate(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) -> Result<()> {
        let next_state = (self.current_state + 1) % (self.states.len());

        let mut temp_tetromino = self.clone();
        temp_tetromino.current_state = next_state;

        if game.can_move(
            &temp_tetromino,
            self.position.row as i16,
            self.position.col as i16,
        ) {
            game.clear_tetromino(stdout)?;
            self.current_state = next_state;
        }

        Ok(())
    }

    fn move_down(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) -> Result<()> {
        if game.can_move(self, self.position.row as i16 + 1, self.position.col as i16) {
            game.clear_tetromino(stdout)?;
            self.position.row += 1;
        }

        Ok(())
    }

    fn hard_drop(&mut self, game: &mut Game, stdout: &mut std::io::Stdout) -> Result<()> {
        while game.can_move(self, self.position.row as i16 + 1, self.position.col as i16) {
            game.clear_tetromino(stdout)?;
            self.position.row += 1;
        }

        Ok(())
    }
}

fn tetromino_width(tetromino: &Vec<Vec<Cell>>) -> usize {
    let mut width = 0;

    for col in 0..tetromino[0].len() {
        let col_width = tetromino
            .iter()
            .filter(|row| row[col].symbols != SPACE)
            .count();

        if col_width > 0 {
            width += 1
        }
    }

    width
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
            let my_local_ip = local_ip()?;
            println!(
                "Server started. Please invite your competitor to connect to {}.",
                format!("{}:8080", my_local_ip)
            );

            let (stream, _) = listener.accept()?;
            println!("Player 2 connected.");

            let mut stream_clone = stream.try_clone()?;
            let (sender, receiver): (Sender<MessageType>, Receiver<MessageType>) = channel();
            let mut game = Game::new(conn, Some(stream), Some(receiver))?;

            thread::spawn(move || {
                receive_message(&mut stream_clone, sender);
            });

            game.start()?;
        } else {
            if let Some(server_address) = args.server_address {
                let stream = TcpStream::connect(server_address)?;

                let mut stream_clone = stream.try_clone()?;
                let (sender, receiver): (Sender<MessageType>, Receiver<MessageType>) = channel();
                let mut game = Game::new(conn, Some(stream), Some(receiver))?;

                thread::spawn(move || {
                    receive_message(&mut stream_clone, sender);
                });

                game.start()?;
            }
        }
    } else {
        let mut game = Game::new(conn, None, None)?;
        game.start()?;
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

const MARGIN: usize = BLOCK_WIDTH;

fn print_centered_messages(
    stdout: &mut io::Stdout,
    width: Option<usize>,
    messages: Vec<&str>,
) -> Result<()> {
    let (term_width, term_height) = terminal::size()?;
    let start_y = term_height / 2 - messages.len() as u16 / 2;

    let longest_length = find_longest_message_length(&messages);

    let frame_width: usize;
    if let Some(value) = width {
        frame_width = value;
    } else {
        frame_width = longest_length + MARGIN * 2;
    }

    let start_x = (term_width - frame_width as u16 - 2) / 2;

    // Print the top border
    execute!(
        stdout,
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::Black),
        SavePosition,
        MoveTo(start_x, start_y - 1),
        Print(format!("{}{}{}", "|", ("-").repeat(frame_width), "|")),
        ResetColor,
        RestorePosition,
    )?;

    // Print the messages with borders
    for (index, message) in messages.iter().enumerate() {
        let left = (frame_width - message.len()) / 2;
        execute!(
            stdout,
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            SavePosition,
            MoveTo(start_x, start_y + index as u16),
            Print(format!(
                "|{}{}{}|",
                " ".repeat(left as usize),
                message,
                " ".repeat(frame_width - left - message.len())
            )),
            ResetColor,
            RestorePosition,
        )?;
    }

    // Print the bottom border
    let bottom_border_y = start_y + messages.len() as u16;
    execute!(
        stdout,
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::Black),
        SavePosition,
        MoveTo(start_x, bottom_border_y),
        Print(format!("{}{}{}", "|", ("-").repeat(frame_width), "|")),
        ResetColor,
        RestorePosition,
    )?;

    stdout.flush()?;

    Ok(())
}

fn find_longest_message_length(messages: &[&str]) -> usize {
    messages
        .iter()
        .map(|message| message.len())
        .max()
        .unwrap_or(0)
}

fn print_left_aligned_messages(
    stdout: &mut io::Stdout,
    title: &str,
    width: Option<usize>,
    start_x: u16,
    start_y: u16,
    messages: Vec<&str>,
) -> Result<()> {
    let (longest_key_length, longest_value_length) = find_longest_key_value_length(&messages);
    let frame_width: usize;
    if let Some(value) = width {
        frame_width = value;
    } else {
        frame_width = longest_key_length + longest_value_length + 3;
    }

    // Print the top border
    let left = (frame_width - title.len() - 2) / 2;
    execute!(
        stdout,
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::Black),
        SavePosition,
        MoveTo(start_x, start_y - 1),
        Print(format!(
            "{}{} {} {}{}",
            "|",
            ("-").repeat(left),
            title,
            ("-").repeat(frame_width - left - title.len() - 2),
            "|"
        )),
        ResetColor,
        RestorePosition,
    )?;

    // Print the messages with borders
    for (index, message) in messages.iter().enumerate() {
        if message.len() == 0 {
            execute!(
                stdout,
                SetForegroundColor(Color::White),
                SetBackgroundColor(Color::Black),
                SavePosition,
                MoveTo(start_x, start_y + index as u16),
                Print(format!("|{}|", " ".repeat(frame_width))),
                ResetColor,
                RestorePosition,
            )?;
        } else {
            let parts: Vec<&str> = message.split(':').collect();

            let right_padding_spaces: String;
            if let Some(value) = width {
                right_padding_spaces = " ".repeat(value - 2 - message.chars().count());
            } else {
                right_padding_spaces = " ".repeat(longest_value_length - parts[1].chars().count());
            }
            execute!(
                stdout,
                SetForegroundColor(Color::White),
                SetBackgroundColor(Color::Black),
                SavePosition,
                MoveTo(start_x, start_y + index as u16),
                Print(format!(
                    "| {:<width$}:{} {}|",
                    String::from(parts[0]),
                    String::from(parts[1]),
                    right_padding_spaces,
                    width = longest_key_length,
                )),
                ResetColor,
                RestorePosition,
            )?;
        }
    }

    // Print the bottom border
    let bottom_border_y = start_y + messages.len() as u16;
    execute!(
        stdout,
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::Black),
        SavePosition,
        MoveTo(start_x, bottom_border_y),
        Print(format!("{}{}{}", "|", ("-").repeat(frame_width), "|")),
        ResetColor,
        RestorePosition,
    )?;

    stdout.flush()?;

    Ok(())
}

fn find_longest_key_value_length(messages: &Vec<&str>) -> (usize, usize) {
    let mut longest_key_length = 0;
    let mut longest_value_length = 0;

    for message in messages {
        if message.len() == 0 {
            continue;
        }
        let parts: Vec<&str> = message.split(':').collect();
        longest_key_length = longest_key_length.max(parts[0].len());
        longest_value_length = longest_value_length.max(parts[1].chars().count());
    }

    (longest_key_length, longest_value_length)
}

fn send_message(stream: &mut TcpStream, message: MessageType) {
    let message_string = match message {
        MessageType::ClearedRows(rows) => format!("{}{}", PREFIX_CLEARED_ROWS, rows),
        MessageType::Notification(msg) => format!("{}{}", PREFIX_NOTIFICATION, msg),
    };

    if let Err(err) = stream.write_all(message_string.as_bytes()) {
        eprintln!("Error writing message: {}", err);
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
