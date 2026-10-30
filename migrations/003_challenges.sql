-- Challenges. `practice` = listed in the permanent practice area (event-only
-- challenges are hidden there until their event ends).
CREATE TABLE challs (
    uuid         CHAR(36)     NOT NULL PRIMARY KEY,
    tenant_uuid  CHAR(36)     NOT NULL,
    slug         VARCHAR(80)  NOT NULL,
    name         VARCHAR(120) NOT NULL,
    description  MEDIUMTEXT   NOT NULL,
    category     VARCHAR(32)  NOT NULL DEFAULT 'misc',
    difficulty   ENUM('easy','medium','hard','insane') NOT NULL DEFAULT 'easy',
    points       INT          NOT NULL DEFAULT 100,
    ctype        ENUM('file','static','instance') NOT NULL DEFAULT 'file',
    static_url   VARCHAR(512) NOT NULL DEFAULT '',
    flag_type    ENUM('static','case_insensitive','regex','per_instance') NOT NULL DEFAULT 'static',
    flag         VARCHAR(512) NOT NULL DEFAULT '',
    status       ENUM('draft','published','archived') NOT NULL DEFAULT 'draft',
    practice     BOOLEAN      NOT NULL DEFAULT TRUE,
    published_at DATETIME     NULL,
    created_at   DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at   DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    UNIQUE KEY uq_challs_slug (tenant_uuid, slug),
    KEY ix_challs_list (tenant_uuid, status, practice, category),
    CONSTRAINT fk_challs_tenant FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE challs_authors (
    chall_uuid CHAR(36) NOT NULL,
    user_uuid  CHAR(36) NOT NULL,
    role       ENUM('owner','coauthor') NOT NULL DEFAULT 'owner',
    PRIMARY KEY (chall_uuid, user_uuid),
    KEY ix_authors_user (user_uuid),
    CONSTRAINT fk_authors_chall FOREIGN KEY (chall_uuid) REFERENCES challs (uuid) ON DELETE CASCADE,
    CONSTRAINT fk_authors_user FOREIGN KEY (user_uuid) REFERENCES users (uuid) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE challs_files (
    uuid        CHAR(36)     NOT NULL PRIMARY KEY,
    chall_uuid  CHAR(36)     NOT NULL,
    filename    VARCHAR(255) NOT NULL,
    storage_key VARCHAR(255) NOT NULL,
    sha256      CHAR(64)     NOT NULL,
    size        BIGINT       NOT NULL,
    created_at  DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY ix_files_chall (chall_uuid),
    CONSTRAINT fk_files_chall FOREIGN KEY (chall_uuid) REFERENCES challs (uuid) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

-- Every attempt, kept for audit and anti-cheat (only the SHA-256 of the value).
CREATE TABLE challs_submissions (
    id          BIGINT      NOT NULL AUTO_INCREMENT PRIMARY KEY,
    tenant_uuid CHAR(36)    NOT NULL,
    chall_uuid  CHAR(36)    NOT NULL,
    user_uuid   CHAR(36)    NOT NULL,
    team_uuid   CHAR(36)    NULL,
    event_uuid  CHAR(36)    NULL,
    value_hash  BINARY(32)  NOT NULL,
    correct     BOOLEAN     NOT NULL,
    ip          VARCHAR(45) NOT NULL DEFAULT '',
    created_at  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    KEY ix_sub_chall (chall_uuid, created_at),
    KEY ix_sub_user (user_uuid, created_at),
    KEY ix_sub_hash (tenant_uuid, value_hash)
) ENGINE=InnoDB;

-- One solve per (event, team-or-user, challenge). MySQL lets several NULLs through a
-- UNIQUE key, so the key uses generated columns. event_key '' = practice.
CREATE TABLE challs_solved (
    tenant_uuid CHAR(36)    NOT NULL,
    chall_uuid  CHAR(36)    NOT NULL,
    user_uuid   CHAR(36)    NOT NULL,
    team_uuid   CHAR(36)    NULL,
    event_uuid  CHAR(36)    NULL,
    solved_at   DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    scope_key   CHAR(36)    AS (COALESCE(team_uuid, user_uuid)) STORED,
    event_key   CHAR(36)    AS (COALESCE(event_uuid, '')) STORED,
    UNIQUE KEY uq_solve (event_key, scope_key, chall_uuid),
    KEY ix_solve_chall (chall_uuid, event_key, solved_at),
    KEY ix_solve_feed (tenant_uuid, event_key, solved_at),
    KEY ix_solve_user (user_uuid),
    CONSTRAINT fk_solve_chall FOREIGN KEY (chall_uuid) REFERENCES challs (uuid) ON DELETE CASCADE,
    -- no CASCADE: MySQL forbids it on a base column of a generated column
    CONSTRAINT fk_solve_user FOREIGN KEY (user_uuid) REFERENCES users (uuid)
) ENGINE=InnoDB;
