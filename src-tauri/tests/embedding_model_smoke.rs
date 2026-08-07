#[test]
#[ignore = "requires the verified bundled embedding model"]
fn bundled_embedding_model_initializes_and_embeds_a_fixed_text() {
    iris_lib::embedding::engine::ensure_embedding_model_available()
        .expect("verified bundled embedding model must initialize");
    let vector = iris_lib::embedding::engine::embed_text("Iris release embedding smoke test")
        .expect("verified bundled embedding model must embed a fixed text");

    assert_eq!(
        vector.len(),
        iris_lib::embedding::engine::EMBEDDING_DIMENSION
    );
    assert!(vector.iter().all(|value| value.is_finite()));
}

#[test]
#[ignore = "release packaging sqlite-vec load gate"]
fn bundled_sqlite_vec_loads_and_applies_v3_index_migration() {
    #[cfg(not(feature = "sqlite-vec"))]
    panic!("release packaging requires the default sqlite-vec feature");

    #[cfg(feature = "sqlite-vec")]
    verify_bundled_sqlite_vec_loads_and_applies_v3_index_migration();
}

#[cfg(feature = "sqlite-vec")]
fn verify_bundled_sqlite_vec_loads_and_applies_v3_index_migration() {
    let database = iris_lib::storage::db::Database::open_in_memory()
        .expect("bundled sqlite-vec database must initialize");

    assert!(
        database.vector_index_ready(),
        "bundled sqlite-vec extension must load before release packaging"
    );
    database
        .with_read_conn(|conn| {
            let version: String = conn.query_row("SELECT vec_version()", [], |row| row.get(0))?;
            assert!(version.starts_with('v'));
            let migration_applied: bool = conn.query_row(
                "SELECT EXISTS(
                     SELECT 1 FROM _migrations WHERE name = '061_sqlite_vec_v3'
                 )",
                [],
                |row| row.get(0),
            )?;
            assert!(migration_applied, "sqlite-vec v3 migration must be applied");
            Ok(())
        })
        .expect("verify bundled sqlite-vec release database");
}
