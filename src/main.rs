use rand::Rng;
use std::io::{self, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{self, MoveTo},
    event::{poll, read, Event, KeyCode, KeyEvent, KeyEventKind},
    execute,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType},
    ExecutableCommand,
};

use dirs;
use rusqlite::{params, Connection, Result};

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
    name: String,
    score: u64,
}

const GAME_OVER_MESSAGE: &str = "GAME OVER";
const HIGH_SCORES_MESSAGE: &str = "HIGH SCORES";
const RESTART_MESSAGE: &str = "(R)estart | (Q)uit";
const PAUSED_MESSAGE: &str = "PAUSED";
const CONTINUE_MESSAGE: &str = "(C)ontinue | (Q)uit";

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
}

impl Game {
    fn new(conn: Connection) -> Self {
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
        }
    }

    fn render(&self, stdout: &mut std::io::Stdout) {
        stdout.execute(Clear(ClearType::All)).unwrap();

        for (y, row) in self.play_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16;
                self.render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        for (y, row) in self.preview_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + (x + PLAY_WIDTH + 3) as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16;
                self.render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        for (y, row) in create_grid(PREVIEW_WIDTH, SCORE_HEIGHT).iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + (x + PLAY_WIDTH + 3) as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + 8;
                self.render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        for (y, row) in create_grid(PREVIEW_WIDTH, HELP_HEIGHT).iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + (x + PLAY_WIDTH + 3) as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + 14;
                self.render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        let preview_start_x = self.start_x + (PLAY_WIDTH + 4) as u16 * BLOCK_WIDTH as u16;
        execute!(
            stdout,
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
            ResetColor
        )
        .unwrap();
    }

    fn render_cell(&self, stdout: &mut std::io::Stdout, x: u16, y: u16, cell: Cell) {
        execute!(
            stdout,
            MoveTo(x, y),
            SetForegroundColor(cell.color),
            SetBackgroundColor(Color::Black),
            Print(cell.symbols),
            ResetColor
        )
        .unwrap();
    }

    fn handle_event(&mut self, stdout: &mut std::io::Stdout) {
        let mut drop_timer = Instant::now();
        let mut soft_drop_timer = Instant::now();
        let mut name = String::new();
        let mut cursor_position: usize = 0;

        loop {
            if self.paused {
                self.handle_pause_event(stdout);
            } else {
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

                if poll(Duration::from_millis(10)).unwrap() {
                    let event = read().unwrap();
                    match event {
                        Event::Key(KeyEvent {
                            code,
                            state: _,
                            kind,
                            modifiers: _,
                        }) => {
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
                                    if kind == KeyEventKind::Press {
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
                                }
                                KeyCode::Char('j') | KeyCode::Down => {
                                    tetromino.hard_drop(self, stdout);
                                    self.lock_and_move_to_next(&tetromino, stdout);
                                }
                                KeyCode::Char('p') => {
                                    self.paused = !self.paused;
                                }
                                KeyCode::Char('q') => {
                                    stdout.execute(Clear(ClearType::All)).unwrap();
                                    std::process::exit(0);
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    self.render_tetromino(stdout);
                }

                if self.is_game_over() {
                    let player: Player = self
                        .conn
                        .query_row(
                            "SELECT player_name, score FROM high_scores ORDER BY score DESC LIMIT 4,1",
                            params![],
                            |row| {
                                Ok(Player {
                                    name: row.get(0)?,
                                    score: row.get(1)?,
                                })
                            },
                        )
                        .unwrap();

                    if (self.score as u64) <= player.score {
                        self.show_high_scores(stdout);
                    } else {
                        let new_high_score_grid = create_grid(PLAY_WIDTH + 2, 4);

                        for (y, row) in new_high_score_grid.iter().enumerate() {
                            for (x, &ref cell) in row.iter().enumerate() {
                                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16
                                    - BLOCK_WIDTH as u16;
                                let screen_y = self.start_y + y as u16 + 8;
                                self.render_cell(stdout, screen_x, screen_y, cell.clone());
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

                        loop {
                            stdout
                                .write(format!("Enter your name: {}", name).as_bytes())
                                .unwrap();
                            stdout.flush().unwrap();

                            let (_, term_height) = terminal::size().unwrap();
                            stdout
                                .execute(MoveTo(
                                    self.start_x + BLOCK_WIDTH as u16 + 1,
                                    (term_height - 3) / 2 + 2,
                                ))
                                .unwrap();

                            if poll(Duration::from_millis(10)).unwrap() {
                                let event = read().unwrap();
                                match event {
                                    Event::Key(KeyEvent {
                                        code,
                                        state: _,
                                        kind: _,
                                        modifiers: _,
                                    }) => match code {
                                        KeyCode::Backspace => {
                                            // Handle Backspace key to remove characters.
                                            if !name.is_empty() && cursor_position > 0 {
                                                name.remove(cursor_position - 1);
                                                cursor_position -= 1;
                                            }
                                        }
                                        KeyCode::Enter => {
                                            self.conn.execute(
                                        "INSERT INTO high_scores (player_name, score) VALUES (?1, ?2)",
                                        params![name, self.score],
                                    ).unwrap();

                                            self.show_high_scores(stdout);

                                            break;
                                        }
                                        KeyCode::Left => {
                                            // Move the cursor left.
                                            if cursor_position > 0 {
                                                cursor_position -= 1;
                                            }
                                        }
                                        KeyCode::Right => {
                                            // Move the cursor right.
                                            if cursor_position < name.len() {
                                                cursor_position += 1;
                                            }
                                        }
                                        KeyCode::Char(c) => {
                                            name.insert(cursor_position, c);
                                            cursor_position += 1;
                                        }
                                        _ => {}
                                    },
                                    _ => {}
                                }
                            }
                        }
                    }

                    loop {
                        if let Ok(event) = read() {
                            match event {
                                Event::Key(KeyEvent {
                                    code,
                                    modifiers: _,
                                    kind: _,
                                    state: _,
                                }) => {
                                    if code == KeyCode::Char('q') {
                                        stdout.execute(Clear(ClearType::All)).unwrap();
                                        std::process::exit(0);
                                    } else if code == KeyCode::Char('r') {
                                        self.reset_game();
                                        break;
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

    fn handle_pause_event(&mut self, stdout: &mut io::Stdout) {
        let paused_grid = create_grid(8, 2);

        for (y, row) in paused_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16 + BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + 9;
                self.render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        execute!(
            stdout,
            MoveTo(
                self.start_x
                    + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16 - PAUSED_MESSAGE.len() as u16)
                        / 2,
                self.start_y + 10
            ),
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            Print(PAUSED_MESSAGE),
            ResetColor
        )
        .unwrap();

        execute!(
            stdout,
            MoveTo(
                self.start_x
                    + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16
                        - CONTINUE_MESSAGE.len() as u16)
                        / 2,
                self.start_y + 11
            ),
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            Print(CONTINUE_MESSAGE),
            ResetColor
        )
        .unwrap();

        if let Ok(event) = read() {
            match event {
                Event::Key(KeyEvent {
                    code,
                    modifiers: _,
                    kind: _,
                    state: _,
                }) => match code {
                    KeyCode::Enter | KeyCode::Char('c') => {
                        self.render(stdout);
                        self.paused = false;
                    }
                    KeyCode::Char('q') => {
                        stdout.execute(Clear(ClearType::All)).unwrap();
                        std::process::exit(0);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
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
                    stdout
                        .execute(MoveTo(
                            self.start_x + grid_x as u16 * BLOCK_WIDTH as u16,
                            self.start_y + grid_y as u16,
                        ))
                        .unwrap()
                        .execute(SetBackgroundColor(Color::Black))
                        .unwrap()
                        .execute(Print(SPACE))
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
                if cell.symbols != SPACE {
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
            if self.play_grid[row_index]
                .iter()
                .all(|cell| cell.symbols != SPACE)
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

        match filled_rows.len() {
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
                            MoveTo(
                                self.start_x + grid_x as u16 * BLOCK_WIDTH as u16,
                                self.start_y + grid_y as u16
                            ),
                            SetForegroundColor(cell.color),
                            SetBackgroundColor(Color::Black),
                            Print(cell.symbols),
                            ResetColor,
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
                            MoveTo(
                                self.start_x
                                    + (PLAY_WIDTH + 2 + grid_x) as u16 * BLOCK_WIDTH as u16,
                                self.start_y + grid_y as u16
                            ),
                            SetForegroundColor(cell.color),
                            SetBackgroundColor(Color::Black),
                            Print(cell.symbols),
                            ResetColor,
                        )
                        .unwrap();
                    }
                }
            }
        }
    }

    fn is_game_over(&mut self) -> bool {
        for row in &self.play_grid[2..3] {
            if !self.paused && row.iter().any(|cell| cell.symbols == SQUARE_BRACKETS) {
                return !self.paused & true;
            }
        }
        false
    }

    fn show_high_scores(&self, stdout: &mut io::Stdout) {
        let game_over_grid = create_grid(PLAY_WIDTH + 2, PLAY_WIDTH);
        let game_over_start_row = 5;

        for (y, row) in game_over_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16 - BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + game_over_start_row;
                self.render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        execute!(
            stdout,
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
            ResetColor
        )
        .unwrap();

        let high_scores_grid = create_grid(PLAY_WIDTH, 6);

        for (y, row) in high_scores_grid.iter().enumerate() {
            for (x, &ref cell) in row.iter().enumerate() {
                let screen_x = self.start_x + x as u16 * BLOCK_WIDTH as u16;
                let screen_y = self.start_y + y as u16 + game_over_start_row + 2;
                self.render_cell(stdout, screen_x, screen_y, cell.clone());
            }
        }

        execute!(
            stdout,
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
            ResetColor
        )
        .unwrap();

        let mut stmt = self
            .conn
            .prepare("SELECT player_name, score FROM high_scores ORDER BY score DESC LIMIT 5")
            .unwrap();
        let players = stmt
            .query_map([], |row| {
                Ok((row.get_unwrap::<_, String>(0), row.get_unwrap::<_, i64>(1)))
            })
            .unwrap();

        for (index, player) in players.enumerate() {
            let (name, score) = player.unwrap();

            execute!(
                stdout,
                MoveTo(
                    self.start_x + BLOCK_WIDTH as u16 * 2,
                    self.start_y + index as u16 + game_over_start_row + 4,
                ),
                SetForegroundColor(Color::White),
                SetBackgroundColor(Color::Black),
                Print(format!("{:<15}{:>9}", name, score)),
                ResetColor
            )
            .unwrap();
        }

        execute!(
            stdout,
            MoveTo(
                self.start_x
                    + ((PLAY_WIDTH as u16 + 2) * BLOCK_WIDTH as u16 - RESTART_MESSAGE.len() as u16)
                        / 2,
                self.start_y + game_over_start_row + 10
            ),
            SetForegroundColor(Color::White),
            SetBackgroundColor(Color::Black),
            Print(RESTART_MESSAGE),
            ResetColor
        )
        .unwrap();
    }

    fn reset_game(&mut self) {
        let home_dir = dirs::home_dir().unwrap();

        let conn = open(&home_dir).unwrap();

        let mut game = Game::new(conn);
        let mut stdout = io::stdout();

        game.render(&mut stdout);
        game.handle_event(&mut stdout);
    }
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

struct CleanupGuard;

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        execute!(io::stdout(), cursor::Show).unwrap();
        terminal::disable_raw_mode().unwrap();
    }
}

fn main() -> io::Result<()> {
    let home_dir = match dirs::home_dir() {
        Some(path) => path,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Failed to get the user's home directory.",
            ))
        }
    };

    let conn = match open(&home_dir) {
        Ok(conn) => conn,
        Err(sqlite_err) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("SQLite error: {}", sqlite_err),
            ));
        }
    };

    terminal::enable_raw_mode()?;
    let _cleanup_guard = CleanupGuard;

    let mut game = Game::new(conn);
    let mut stdout = io::stdout();

    execute!(stdout.lock(), cursor::Hide)?;

    game.render(&mut stdout);

    loop {
        if !game.paused {
            game.handle_event(&mut stdout);
        }
    }
}

fn open(home_dir: &Path) -> Result<Connection, rusqlite::Error> {
    let db_path = home_dir.join(".tetris").join("high_scores.db");
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

fn print_message(stdout: &mut io::Stdout, grid_width: u16, grid_height: u16, n: u16, msg: String) {
    let (term_width, term_height) = terminal::size().unwrap();
    let start_x = (term_width - msg.len() as u16) / 2;
    let start_y = (term_height - grid_height) / 2;
    execute!(
        stdout,
        MoveTo(start_x, start_y + n),
        SetForegroundColor(Color::White),
        SetBackgroundColor(Color::Black),
        Print(msg),
        ResetColor
    )
    .unwrap();
}
