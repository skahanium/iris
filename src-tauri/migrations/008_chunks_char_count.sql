-- Up: Rename token_count to char_count for semantic accuracy
ALTER TABLE chunks RENAME COLUMN token_count TO char_count;
