-- Convoy database schema v1

PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER NOT NULL
);

-- sessions --------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS sessions (
    id               TEXT    PRIMARY KEY,
    agent            TEXT    NOT NULL,
    pid              INTEGER NOT NULL,
    branch           TEXT,
    worktree_path    TEXT,
    nickname         TEXT    NOT NULL,
    started_at       TEXT    NOT NULL,
    last_heartbeat   TEXT    NOT NULL,
    last_seen_alive  TEXT    NOT NULL,
    ended_at         TEXT
);

-- messages -------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS messages (
    id           TEXT PRIMARY KEY,
    from_session TEXT NOT NULL,
    to_session   TEXT,
    kind         TEXT NOT NULL,
    in_reply_to  TEXT,
    body         TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    read_at      TEXT
);

-- file_locks -----------------------------------------------------------------
CREATE TABLE IF NOT EXISTS file_locks (
    abs_path   TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    reason     TEXT,
    claimed_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

-- status_updates -------------------------------------------------------------
CREATE TABLE IF NOT EXISTS status_updates (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT    NOT NULL,
    summary    TEXT    NOT NULL,
    created_at TEXT    NOT NULL
);

-- waits ----------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS waits (
    id             TEXT PRIMARY KEY,
    session_id     TEXT NOT NULL,
    condition_json TEXT NOT NULL,
    hint           TEXT,
    created_at     TEXT NOT NULL,
    expires_at     TEXT NOT NULL,
    satisfied_at   TEXT,
    outcome        TEXT
);

-- events / audit log ---------------------------------------------------------
CREATE TABLE IF NOT EXISTS events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL,
    created_at TEXT NOT NULL
);
