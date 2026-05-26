-- Add version kind for B+ timeline semantics; fix legacy storage_path values.
ALTER TABLE versions ADD COLUMN kind TEXT NOT NULL DEFAULT 'manual';

UPDATE versions
SET kind = CASE
    WHEN is_finalized = 1 THEN 'finalize'
    ELSE 'manual'
END;

UPDATE versions
SET storage_path = CAST(file_id AS TEXT) || '/' || version_no || '.md';
