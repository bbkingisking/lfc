CREATE TABLE IF NOT EXISTS fetches (
    id INTEGER PRIMARY KEY,
    fetched_at TEXT DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS articles (
    id INTEGER PRIMARY KEY,
    fetch_id INTEGER,
    url TEXT,
    og_title TEXT,
    published_time TEXT,
    og_image TEXT,
    author TEXT,
    text TEXT,
    source TEXT,
    FOREIGN KEY(fetch_id) REFERENCES fetches(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS summaries (
    id INTEGER PRIMARY KEY,
    fetch_id INTEGER UNIQUE, -- 1 summary per fetch
    generated_at TEXT DEFAULT CURRENT_TIMESTAMP,
    mood_text TEXT,
    sent BOOLEAN NOT NULL DEFAULT 0,
    FOREIGN KEY(fetch_id) REFERENCES fetches(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS bullets (
    id INTEGER PRIMARY KEY,
    fetch_id INTEGER,
    text TEXT,
    accepted BOOLEAN DEFAULT NULL, -- NULL = not yet filtered, TRUE/FALSE = LLM decision
    FOREIGN KEY(fetch_id) REFERENCES fetches(id) ON DELETE CASCADE
);
