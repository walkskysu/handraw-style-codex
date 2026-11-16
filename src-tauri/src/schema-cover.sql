CREATE TABLE generation_jobs_cover(id TEXT PRIMARY KEY,group_id TEXT NOT NULL UNIQUE REFERENCES photo_groups(id),plan_id TEXT REFERENCES generation_plans(id),status TEXT NOT NULL,revision INTEGER NOT NULL,idempotency_key TEXT NOT NULL UNIQUE,metadata TEXT NOT NULL,
 draft_id TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.draft_id')) STORED REFERENCES creation_drafts(id),
 mode TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.mode')) STORED NOT NULL CHECK(mode IN ('quick','planned')),
 stop_requested INTEGER GENERATED ALWAYS AS (json_extract(metadata,'$.stop_requested')) STORED NOT NULL,
 expected_count INTEGER GENERATED ALWAYS AS (json_extract(metadata,'$.expected_count')) STORED NOT NULL CHECK(expected_count BETWEEN 1 AND (12 + CASE WHEN mode='planned' AND json_extract(metadata,'$.input.include_cover')=1 THEN 1 ELSE 0 END) AND (mode!='quick' OR expected_count=1)),
 updated_at TEXT GENERATED ALWAYS AS (json_extract(metadata,'$.updated_at')) STORED NOT NULL);
INSERT INTO generation_jobs_cover(id,group_id,plan_id,status,revision,idempotency_key,metadata)
 SELECT id,group_id,plan_id,status,revision,idempotency_key,metadata FROM generation_jobs;
DROP TABLE generation_jobs;
ALTER TABLE generation_jobs_cover RENAME TO generation_jobs;
CREATE INDEX job_state ON generation_jobs(status,updated_at);
