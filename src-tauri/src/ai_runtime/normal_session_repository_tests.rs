use super::normal_session_repository::NormalSessionRepository;
use crate::storage::db::Database;

#[test]
fn normal_session_is_created_and_resolved_without_scene_or_note_binding() {
    let db = Database::open_in_memory().expect("database");

    let created = NormalSessionRepository::create(&db).expect("create normal session");
    let resolved = NormalSessionRepository::get(&db, &created.session_key)
        .expect("get normal session")
        .expect("session exists");

    assert_eq!(resolved, created);
    assert!(created.session_key.starts_with("run_session:"));
    db.with_read_conn(|conn| {
        let (scene, note_path): (String, Option<String>) = conn.query_row(
            "SELECT scene, note_path FROM sessions WHERE id = ?1",
            [created.session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert!(
            scene.is_empty(),
            "new normal session must not encode a scene"
        );
        assert!(
            note_path.is_none(),
            "new normal session must not bind a note"
        );
        Ok(())
    })
    .expect("unbound session facts");
}
