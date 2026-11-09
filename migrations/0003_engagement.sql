-- Engagement per open (one open = one view session). Filled by shell heartbeats.
ALTER TABLE opens ADD COLUMN visible_seconds INTEGER NOT NULL DEFAULT 0;
ALTER TABLE opens ADD COLUMN active_seconds  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE opens ADD COLUMN max_scroll_pct  INTEGER NOT NULL DEFAULT 0;
ALTER TABLE opens ADD COLUMN last_heartbeat_at INTEGER;
