use std::process::exit;

use crossterm::terminal;

use clap::Parser;

use tetris_tui::{Args, PLAY_WIDTH, PLAY_HEIGHT, CELL_WIDTH, STATS_WIDTH, DISTANCE, MAX_LEVEL, Result};

fn main() -> Result<()> {
    let args = Args::parse();
    if args.number_of_lines_already_filled > 10 {
        eprintln!("The number of lines already filled must be less than or equal 10.");
        exit(1);
    }

    if args.level > MAX_LEVEL {
        eprintln!("Level must be between 0 and {}.", MAX_LEVEL);
        exit(1);
    }

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

    tetris_tui::start(&args, term_width, term_height)?;

    Ok(())
}
