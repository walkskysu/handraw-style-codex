CREATE TABLE IF NOT EXISTS photo_groups(id TEXT PRIMARY KEY, metadata TEXT NOT NULL);
ALTER TABLE photo_groups ADD COLUMN mode TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE photo_groups ADD COLUMN title TEXT NOT NULL DEFAULT '';
ALTER TABLE photo_groups ADD COLUMN created_at TEXT NOT NULL DEFAULT '';
ALTER TABLE photo_groups ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;
ALTER TABLE photo_groups ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
ALTER TABLE photo_groups ADD COLUMN cover_id TEXT;
ALTER TABLE artworks ADD COLUMN group_id TEXT REFERENCES photo_groups(id);
ALTER TABLE artworks ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE artworks ADD COLUMN generation_item_id TEXT REFERENCES generation_items(id);
ALTER TABLE artworks ADD COLUMN availability TEXT NOT NULL DEFAULT 'available';
CREATE TABLE migration_originals(id TEXT PRIMARY KEY, metadata TEXT NOT NULL);
CREATE TABLE migration_original_groups(id TEXT PRIMARY KEY, metadata TEXT NOT NULL);
CREATE TABLE migration_summary(metadata TEXT NOT NULL);
CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,name TEXT NOT NULL,app_version TEXT NOT NULL,applied_at TEXT NOT NULL);
-- Read-only stored projections make scalar fields queryable without allowing a second writer
-- to diverge from the complete persisted record. Relations and dispatch states stay explicit.
CREATE TABLE creation_drafts(id TEXT PRIMARY KEY, input_revision INTEGER NOT NULL, metadata TEXT NOT NULL,
 mode TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.mode')) STORED NOT NULL CHECK(mode IN ('quick','planned')),
 prompt TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.prompt')) STORED NOT NULL,
 snapshot_json TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.snapshot')) STORED NOT NULL,
 requested_count INTEGER GENERATED ALWAYS AS (json_extract(metadata,'$.requested_count')) STORED,
 count_mode TEXT GENERATED ALWAYS AS (CASE WHEN requested_count IS NULL THEN 'auto' ELSE 'specified' END) STORED,
 current_plan_id TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.current_plan_id')) STORED REFERENCES generation_plans(id),
 feedback_draft TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.feedback_draft')) STORED NOT NULL,
 updated_at TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.updated_at')) STORED NOT NULL);
CREATE TABLE generation_plans(id TEXT PRIMARY KEY,draft_id TEXT NOT NULL REFERENCES creation_drafts(id),version INTEGER NOT NULL,metadata TEXT NOT NULL,
 parent_plan_id TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.parent_plan_id')) STORED REFERENCES generation_plans(id),
 input_revision INTEGER GENERATED ALWAYS AS (json_extract(metadata,'$.input_revision')) STORED NOT NULL,
 input_hash TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.input_hash')) STORED NOT NULL,
 content_hash TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.content_hash')) STORED NOT NULL,
 content_json TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.content')) STORED NOT NULL,
 feedback TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.feedback')) STORED NOT NULL,
 change_summary TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.content.change_summary')) STORED NOT NULL,
 confirmed_at TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.confirmed_at')) STORED,
 UNIQUE(draft_id,version));
CREATE TABLE planning_jobs(id TEXT PRIMARY KEY,draft_id TEXT NOT NULL REFERENCES creation_drafts(id),status TEXT NOT NULL,revision INTEGER NOT NULL,metadata TEXT NOT NULL,
 base_plan_id TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.base_plan_id')) STORED REFERENCES generation_plans(id),
 input_revision INTEGER GENERATED ALWAYS AS (json_extract(metadata,'$.input_revision')) STORED NOT NULL,
 error TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.error')) STORED,
 output_plan_id TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.output_plan_id')) STORED REFERENCES generation_plans(id),
 updated_at TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.updated_at')) STORED NOT NULL);
CREATE TABLE generation_jobs(id TEXT PRIMARY KEY,group_id TEXT NOT NULL UNIQUE REFERENCES photo_groups(id),plan_id TEXT REFERENCES generation_plans(id),status TEXT NOT NULL,revision INTEGER NOT NULL,idempotency_key TEXT NOT NULL UNIQUE,metadata TEXT NOT NULL,
 draft_id TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.draft_id')) STORED REFERENCES creation_drafts(id),
 mode TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.mode')) STORED NOT NULL CHECK(mode IN ('quick','planned')),
 stop_requested INTEGER GENERATED ALWAYS AS (json_extract(metadata,'$.stop_requested')) STORED NOT NULL,
 expected_count INTEGER GENERATED ALWAYS AS (json_extract(metadata,'$.expected_count')) STORED NOT NULL CHECK(expected_count BETWEEN 1 AND 12 AND (mode!='quick' OR expected_count=1)),
 updated_at TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.updated_at')) STORED NOT NULL);
CREATE TABLE generation_items(id TEXT PRIMARY KEY,job_id TEXT NOT NULL REFERENCES generation_jobs(id),position INTEGER NOT NULL,generation_status TEXT NOT NULL,save_status TEXT NOT NULL,metadata TEXT NOT NULL,
 plan_item_id TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.plan_item_id')) STORED NOT NULL,
 title TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.title')) STORED NOT NULL,
 prompt TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.prompt')) STORED NOT NULL,
 attempt_no INTEGER GENERATED ALWAYS AS (json_extract(metadata,'$.attempt_no')) STORED NOT NULL,
 error TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.error')) STORED,
 UNIQUE(job_id,position));
CREATE TABLE generation_attempts(id TEXT PRIMARY KEY,item_id TEXT NOT NULL REFERENCES generation_items(id),attempt_no INTEGER NOT NULL,status TEXT NOT NULL,staging_path TEXT NOT NULL,metadata TEXT NOT NULL,
 started_at TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.started_at')) STORED NOT NULL,
 ended_at TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.ended_at')) STORED,
 error TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.error')) STORED,
 tool_version TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.tool_version')) STORED,
 checksum TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.checksum')) STORED,
 UNIQUE(item_id,attempt_no));
CREATE UNIQUE INDEX artwork_item ON artworks(generation_item_id) WHERE generation_item_id IS NOT NULL;
CREATE INDEX artwork_group ON artworks(group_id,position);
CREATE INDEX group_order ON photo_groups(deleted,created_at,id);
CREATE INDEX job_state ON generation_jobs(status,updated_at);
CREATE INDEX planning_state ON planning_jobs(status,updated_at);
CREATE TRIGGER group_insert AFTER INSERT ON photo_groups BEGIN
 UPDATE photo_groups SET mode=coalesce(json_extract(NEW.metadata,'$.mode'),'legacy'),title=json_extract(NEW.metadata,'$.title'),created_at=json_extract(NEW.metadata,'$.created_at'),favorite=json_extract(NEW.metadata,'$.favorite'),deleted=json_extract(NEW.metadata,'$.deleted'),cover_id=json_extract(NEW.metadata,'$.cover_id') WHERE id=NEW.id;
END;
CREATE TRIGGER group_update AFTER UPDATE OF metadata ON photo_groups BEGIN
 UPDATE photo_groups SET mode=coalesce(json_extract(NEW.metadata,'$.mode'),'legacy'),title=json_extract(NEW.metadata,'$.title'),created_at=json_extract(NEW.metadata,'$.created_at'),favorite=json_extract(NEW.metadata,'$.favorite'),deleted=json_extract(NEW.metadata,'$.deleted'),cover_id=json_extract(NEW.metadata,'$.cover_id') WHERE id=NEW.id;
END;
CREATE TRIGGER artwork_insert AFTER INSERT ON artworks BEGIN
 UPDATE artworks SET group_id=json_extract(NEW.metadata,'$.group_id'),position=coalesce(json_extract(NEW.metadata,'$.position'),0),generation_item_id=json_extract(NEW.metadata,'$.generation_item_id') WHERE id=NEW.id;
END;
CREATE TRIGGER artwork_update AFTER UPDATE OF metadata ON artworks BEGIN
 UPDATE artworks SET group_id=json_extract(NEW.metadata,'$.group_id'),position=coalesce(json_extract(NEW.metadata,'$.position'),0),generation_item_id=json_extract(NEW.metadata,'$.generation_item_id') WHERE id=NEW.id;
END;
