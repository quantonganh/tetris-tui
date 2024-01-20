use crate::{HighScore, Player, Result};
use rusqlite::{params, Connection, Result as RusqliteResult};
use std::error::Error;
use std::fs;

pub fn open() -> RusqliteResult<Connection, Box<dyn Error>> {
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

    Ok(conn)
}

pub struct HighScoreRepo {
    pub conn: Connection,
}

impl HighScore for HighScoreRepo {
    fn create_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS high_scores (
                id INTEGER PRIMARY KEY,
                player_name TEXT,
                score INTEGER,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            params![],
        )?;
        Ok(())
    }

    fn count(&self) -> Result<i64> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM high_scores", params![], |row| {
                    row.get(0)
                })?;

        Ok(count)
    }

    fn get_player_at_rank(&self, rank: usize) -> Result<Player> {
        let player: Player = self.conn.query_row(
            "SELECT player_name, score FROM high_scores ORDER BY score DESC LIMIT ?1,1",
            params![rank as u32 - 1],
            |row| {
                Ok(Player {
                    name: row.get(0)?,
                    score: row.get(1)?,
                })
            },
        )?;

        Ok(player)
    }

    fn get_top_players(&self) -> Result<Vec<Player>> {
        let mut stmt = self
            .conn
            .prepare("SELECT player_name, score FROM high_scores ORDER BY score DESC LIMIT 5")?;
        let rows = stmt.query_map(params![], |row| {
            Ok(Player {
                name: row.get(0)?,
                score: row.get(1)?,
            })
        })?;
        let players: Result<Vec<Player>> = rows
            .collect::<std::result::Result<_, _>>()
            .map_err(|err| err.into());
        players
    }

    fn insert(&mut self, name: &str, score: usize) -> Result<()> {
        self.conn.execute(
            "INSERT INTO high_scores (player_name, score) VALUES (?1, ?2)",
            params![name, score],
        )?;

        Ok(())
    }
}
