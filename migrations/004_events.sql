-- Time-boxed CTF events, solo or in teams. All times are UTC.
CREATE TABLE events (
    uuid          CHAR(36)     NOT NULL PRIMARY KEY,
    tenant_uuid   CHAR(36)     NOT NULL,
    slug          VARCHAR(80)  NOT NULL,
    title         VARCHAR(120) NOT NULL,
    description   MEDIUMTEXT   NOT NULL,
    starts_at     DATETIME     NOT NULL,
    ends_at       DATETIME     NOT NULL,
    freeze_at     DATETIME     NULL,
    team_mode     BOOLEAN      NOT NULL DEFAULT TRUE,
    max_team_size INT          NOT NULL DEFAULT 4,
    -- open: anyone signs up; code: needs the event code; closed: no new sign-ups
    registration  ENUM('open','code','closed') NOT NULL DEFAULT 'open',
    join_code     VARCHAR(64)  NOT NULL DEFAULT '',
    -- what happens to the challenges when the event ends
    after_end     ENUM('hide','practice') NOT NULL DEFAULT 'practice',
    status        ENUM('draft','published') NOT NULL DEFAULT 'draft',
    -- set once the end-of-event job ran (challenges moved to practice, labs stopped)
    closed_at     DATETIME     NULL,
    created_at    DATETIME     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uq_events_slug (tenant_uuid, slug),
    KEY ix_events_time (tenant_uuid, status, starts_at, ends_at),
    CONSTRAINT fk_events_tenant FOREIGN KEY (tenant_uuid) REFERENCES tenants (uuid) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

CREATE TABLE events_challs (
    event_uuid      CHAR(36) NOT NULL,
    chall_uuid      CHAR(36) NOT NULL,
    points_override INT      NULL,
    PRIMARY KEY (event_uuid, chall_uuid),
    KEY ix_ec_chall (chall_uuid),
    CONSTRAINT fk_ec_event FOREIGN KEY (event_uuid) REFERENCES events (uuid) ON DELETE CASCADE,
    CONSTRAINT fk_ec_chall FOREIGN KEY (chall_uuid) REFERENCES challs (uuid) ON DELETE CASCADE
) ENGINE=InnoDB;

CREATE TABLE teams (
    uuid         CHAR(36)    NOT NULL PRIMARY KEY,
    tenant_uuid  CHAR(36)    NOT NULL,
    event_uuid   CHAR(36)    NOT NULL,
    name         VARCHAR(48) NOT NULL,
    invite_code  CHAR(16)    NOT NULL,
    captain_uuid CHAR(36)    NOT NULL,
    banned_at    DATETIME    NULL,
    created_at   DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE KEY uq_teams_name (event_uuid, name),
    UNIQUE KEY uq_teams_code (invite_code),
    CONSTRAINT fk_teams_event FOREIGN KEY (event_uuid) REFERENCES events (uuid) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_0900_ai_ci;

-- One team per player per event (unique key), whatever the team.
CREATE TABLE teams_members (
    team_uuid  CHAR(36) NOT NULL,
    user_uuid  CHAR(36) NOT NULL,
    event_uuid CHAR(36) NOT NULL,
    joined_at  DATETIME(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    PRIMARY KEY (team_uuid, user_uuid),
    UNIQUE KEY uq_member_event (event_uuid, user_uuid),
    CONSTRAINT fk_tm_team FOREIGN KEY (team_uuid) REFERENCES teams (uuid) ON DELETE CASCADE,
    CONSTRAINT fk_tm_user FOREIGN KEY (user_uuid) REFERENCES users (uuid) ON DELETE CASCADE
) ENGINE=InnoDB;

-- Solo events. In the scoreboard query a solo player is the scope; banned_at disqualifies.
CREATE TABLE events_players (
    event_uuid    CHAR(36) NOT NULL,
    user_uuid     CHAR(36) NOT NULL,
    banned_at     DATETIME NULL,
    registered_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (event_uuid, user_uuid),
    CONSTRAINT fk_ep_event FOREIGN KEY (event_uuid) REFERENCES events (uuid) ON DELETE CASCADE,
    CONSTRAINT fk_ep_user FOREIGN KEY (user_uuid) REFERENCES users (uuid) ON DELETE CASCADE
) ENGINE=InnoDB;
