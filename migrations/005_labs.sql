-- Lab runtime of an `instance` challenge: one container per player (or team in team events).
CREATE TABLE challs_runtime (
    chall_uuid      CHAR(36)     NOT NULL PRIMARY KEY,
    image           VARCHAR(255) NOT NULL,
    port            INT          NOT NULL,
    protocol        ENUM('http','tcp') NOT NULL DEFAULT 'http',
    cpu_millis      INT          NOT NULL DEFAULT 500,
    memory_mb       INT          NOT NULL DEFAULT 256,
    ttl_minutes     INT          NOT NULL DEFAULT 60,
    env             TEXT         NOT NULL,
    readonly_rootfs BOOLEAN      NOT NULL DEFAULT FALSE,
    CONSTRAINT fk_runtime_chall FOREIGN KEY (chall_uuid) REFERENCES challs (uuid) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE instances (
    uuid         CHAR(36)     NOT NULL PRIMARY KEY,
    tenant_uuid  CHAR(36)     NOT NULL,
    chall_uuid   CHAR(36)     NOT NULL,
    user_uuid    CHAR(36)     NOT NULL,
    team_uuid    CHAR(36)     NULL,
    event_uuid   CHAR(36)     NULL,
    container_id VARCHAR(80)  NOT NULL DEFAULT '',
    network_id   VARCHAR(80)  NOT NULL DEFAULT '',
    host_port    INT          NOT NULL DEFAULT 0,
    flag         VARCHAR(255) NOT NULL DEFAULT '',
    status       ENUM('starting','running','stopped','failed') NOT NULL DEFAULT 'starting',
    error        VARCHAR(255) NOT NULL DEFAULT '',
    expires_at   DATETIME     NOT NULL,
    created_at   DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    stopped_at   DATETIME     NULL,
    scope_key    CHAR(36)     AS (COALESCE(team_uuid, user_uuid)) STORED,
    event_key    CHAR(36)     AS (COALESCE(event_uuid, '')) STORED,
    -- at most one live instance per scope: NULL for stopped rows, so they never collide
    active_key   VARCHAR(80)  AS (IF(status IN ('starting','running'), CONCAT(event_key, ':', scope_key), NULL)) STORED,
    UNIQUE KEY uq_instance_active (active_key),
    KEY ix_instances_scope (chall_uuid, event_key, scope_key, created_at),
    KEY ix_instances_expiry (status, expires_at),
    KEY ix_instances_flag (tenant_uuid, flag)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;
