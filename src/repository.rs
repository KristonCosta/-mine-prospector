use rusqlite::Connection;

pub struct MCRepository {
    conn: Connection,
}

impl MCRepository {
    pub fn new() -> Self {
        let conn = Connection::open("default.db").expect("Couldn't open db connection");
        MCRepository {
            conn
        }
    }
}
