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
