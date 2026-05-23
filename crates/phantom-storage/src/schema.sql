-- phantom-storage schema (SQLite)
-- Applied via execute_batch on first open.

CREATE TABLE IF NOT EXISTS recordings (
    id                      TEXT PRIMARY KEY,
    title                   TEXT NOT NULL,
    file_path               TEXT NOT NULL,
    thumb_path              TEXT,
    duration_secs           INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL,
    tags                    TEXT NOT NULL DEFAULT '',
    share_token             TEXT,
    share_password_hash     TEXT,
    share_download_enabled  INTEGER NOT NULL DEFAULT 0,
    share_expires_at        TEXT,
    summary                 TEXT
);

CREATE TABLE IF NOT EXISTS transcripts (
    id              TEXT PRIMARY KEY,
    recording_id    TEXT NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
    content         TEXT NOT NULL,   -- JSON array of TranscriptSegment
    language        TEXT NOT NULL DEFAULT 'en',
    created_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS chapters (
    id              TEXT PRIMARY KEY,
    recording_id    TEXT NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    start_ms        INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS action_items (
    id              TEXT PRIMARY KEY,
    recording_id    TEXT NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
    text            TEXT NOT NULL,
    owner           TEXT,
    done            INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS bookmarks (
    id              TEXT PRIMARY KEY,
    recording_id    TEXT NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
    label           TEXT NOT NULL,
    timestamp_ms    INTEGER NOT NULL
);

-- Full-text search virtual table.
CREATE VIRTUAL TABLE IF NOT EXISTS recordings_fts USING fts5(
    recording_id UNINDEXED,
    title,
    summary,
    content='recordings',
    content_rowid='rowid'
);

-- Keep FTS in sync via triggers.
CREATE TRIGGER IF NOT EXISTS recordings_fts_insert AFTER INSERT ON recordings BEGIN
    INSERT INTO recordings_fts(recording_id, title, summary)
    VALUES (new.id, new.title, coalesce(new.summary,''));
END;

CREATE TRIGGER IF NOT EXISTS recordings_fts_update AFTER UPDATE ON recordings BEGIN
    UPDATE recordings_fts SET title=new.title, summary=coalesce(new.summary,'')
    WHERE recording_id=new.id;
END;

CREATE TRIGGER IF NOT EXISTS recordings_fts_delete AFTER DELETE ON recordings BEGIN
    DELETE FROM recordings_fts WHERE recording_id=old.id;
END;
