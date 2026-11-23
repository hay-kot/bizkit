-- A stand-in for the main app database: sign-in reads a user and writes a
-- session in one write transaction.
CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY, email TEXT NOT NULL UNIQUE) STRICT;
CREATE TABLE IF NOT EXISTS sessions (
  id INTEGER PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES users(id),
  created_at INTEGER NOT NULL
) STRICT;
INSERT OR IGNORE INTO users (id, email) VALUES (1, 'demo@bizkit.test');
