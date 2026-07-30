ALTER TABLE skill_activation_index
    ADD COLUMN embedding_source_hash TEXT NOT NULL DEFAULT '';

ALTER TABLE skill_activation_index
    ADD COLUMN embedding_model TEXT;

ALTER TABLE skill_activation_index
    ADD COLUMN embedding_dimensions INTEGER
        CHECK (embedding_dimensions IS NULL OR embedding_dimensions > 0);
