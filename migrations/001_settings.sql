CREATE TABLE settings (
    `key`      VARCHAR(64) PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO settings (`key`, value) VALUES
    ('site_name', 'Live Hack'),
    ('flag_format', 'LH{...}'),
    ('registration', 'open');
