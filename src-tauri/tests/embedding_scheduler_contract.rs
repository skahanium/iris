use iris_lib::embedding::scheduler::recover_interrupted_generation;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;

#[test]
fn startup_marks_running_generation_interrupted_without_retrying_it() {
    let conn = Connection::open_in_memory().expect("open database");
    migrate_up(&conn).expect("migrate database");
    conn.execute(
        "UPDATE embedding_generation_state
         SET phase = 'running', indexed_items = 2, total_items = 8",
        [],
    )
    .expect("seed abandoned job");

    recover_interrupted_generation(&conn).expect("recover interrupted job");

    let state: (String, String, String) = conn
        .query_row(
            "SELECT phase, failure_code, last_error FROM embedding_generation_state",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read recovered state");
    assert_eq!(state.0, "failed");
    assert_eq!(state.1, "interrupted_restart");
    assert_eq!(state.2, "Embedding rebuild interrupted");
}
