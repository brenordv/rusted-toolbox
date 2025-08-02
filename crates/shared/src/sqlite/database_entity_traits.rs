use rusqlite::Row;

pub trait FromRow: Sized {
    fn from_row(row: &Row) -> Result<Self, rusqlite::Error>;
}