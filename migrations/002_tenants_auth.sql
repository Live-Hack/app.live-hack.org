-- Multi-tenant from day one: every row belongs to a tenant (one platform = one tenant).
-- The tenant with host NULL is the default, used when the Host header matches no tenant.
CREATE TABLE tenants (
    uuid       CHAR(36)     NOT NULL PRIMARY KEY,
    slug       VARCHAR(64)  NOT NULL UNIQUE,
    name       VARCHAR(128) NOT NULL,
    host       VARCHAR(255) NULL UNIQUE,
    created_at DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO tenants (uuid, slug, name, host) VALUES (UUID(), 'default', 'Live Hack', NULL);

-- settings become per tenant
DROP TABLE settings;
CREATE TABLE settings (
    tenant_uuid CHAR(36)    NOT NULL,
    `key`       VARCHAR(64) NOT NULL,
    value       TEXT        NOT NULL,
    updated_at  DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    PRIMARY KEY (tenant_uuid, `key`),
    CONSTRAINT fk_settings_tenant FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

INSERT INTO settings (tenant_uuid, `key`, value)
SELECT uuid, 'site_name', 'Live Hack' FROM tenants WHERE slug = 'default'
UNION ALL SELECT uuid, 'flag_format', 'LH{...}' FROM tenants WHERE slug = 'default'
UNION ALL SELECT uuid, 'registration', 'open' FROM tenants WHERE slug = 'default';

CREATE TABLE users (
    uuid          CHAR(36)     NOT NULL PRIMARY KEY,
    tenant_uuid   CHAR(36)     NOT NULL,
    username      VARCHAR(32)  NOT NULL,
    email         VARCHAR(255) NULL,
    password_hash VARCHAR(255) NOT NULL,
    role          ENUM('player','author','admin') NOT NULL DEFAULT 'player',
    banned_at     DATETIME     NULL,
    created_at    DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uq_users_username (tenant_uuid, username),
    UNIQUE KEY uq_users_email (tenant_uuid, email),
    CONSTRAINT fk_users_tenant FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

-- Only the SHA-256 of the session token is stored.
CREATE TABLE sessions (
    token_hash  BINARY(32)   NOT NULL PRIMARY KEY,
    tenant_uuid CHAR(36)     NOT NULL,
    user_uuid   CHAR(36)     NOT NULL,
    expires_at  DATETIME     NOT NULL,
    user_agent  VARCHAR(255) NOT NULL DEFAULT '',
    ip          VARCHAR(45)  NOT NULL DEFAULT '',
    created_at  DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY ix_sessions_user (user_uuid),
    KEY ix_sessions_expires (expires_at),
    CONSTRAINT fk_sessions_user FOREIGN KEY (user_uuid) REFERENCES users (uuid) ON DELETE CASCADE
) ENGINE=InnoDB;

-- Single-use reset links and multi-use invitations, both created by an admin.
CREATE TABLE user_tokens (
    token_hash  BINARY(32) NOT NULL PRIMARY KEY,
    tenant_uuid CHAR(36)   NOT NULL,
    kind        ENUM('reset','invite') NOT NULL,
    user_uuid   CHAR(36)   NULL,
    uses_left   INT        NOT NULL DEFAULT 1,
    expires_at  DATETIME   NOT NULL,
    created_by  CHAR(36)   NOT NULL,
    created_at  DATETIME   NOT NULL DEFAULT CURRENT_TIMESTAMP,
    KEY ix_user_tokens_tenant (tenant_uuid, kind),
    CONSTRAINT fk_user_tokens_tenant FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
) ENGINE=InnoDB;

-- Fixed-window counters, shared by every app replica.
CREATE TABLE rate_limits (
    bucket VARCHAR(191) NOT NULL PRIMARY KEY,
    win    BIGINT       NOT NULL,
    hits   INT          NOT NULL
) ENGINE=InnoDB;

CREATE TABLE audit_log (
    id          BIGINT      NOT NULL AUTO_INCREMENT PRIMARY KEY,
    tenant_uuid CHAR(36)    NOT NULL,
    actor_uuid  CHAR(36)    NULL,
    action      VARCHAR(64) NOT NULL,
    target_type VARCHAR(32) NOT NULL DEFAULT '',
    target_uuid CHAR(36)    NULL,
    meta        JSON        NULL,
    created_at  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    KEY ix_audit_tenant (tenant_uuid, created_at)
) ENGINE=InnoDB;
