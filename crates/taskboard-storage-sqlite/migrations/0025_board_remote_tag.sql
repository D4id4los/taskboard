-- D-BF4 (boards-over-files dispatch chunk 3): the files substrate's
-- board remote id is the domain UUID itself, carried in its own TEXT
-- column beside the Deck INTEGER one. A bound row carries exactly one
-- non-NULL of the pair — an invariant of the codec, not a trigger
-- (every write goes through the single upsert).
ALTER TABLE boards ADD COLUMN remote_files_id TEXT NULL;
