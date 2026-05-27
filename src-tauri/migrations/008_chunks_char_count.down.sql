-- Down: Revert char_count back to token_count
ALTER TABLE chunks RENAME COLUMN char_count TO token_count;
