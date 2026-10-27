//! Deterministic acceptance fixtures: no account, network or model calls.
use crate::{
    gallery::{PendingArtwork, Store},
    groups::PhotoGroup,
    jobs::{Attempt, Item, Job},
    options::Snapshot,
    planning::{self, Content, Draft, PlanItem},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::{params, Connection};
use std::fs;

fn draft(s: &Store, mode: &str, count: Option<usize>) -> Draft {
    s.save_draft(
        None,
        mode.into(),
        "三张猫咪变体".into(),
        Snapshot::default(),
        count,
        String::new(),
        None,
    )
    .unwrap()
}
fn content(n: usize) -> Content {
    Content {
        title: "猫咪".into(),
        goal: "探索同一主题".into(),
        audience: "儿童".into(),
        kind: "variants".into(),
        visual: "蓝色水彩".into(),
        dimensions: "1:1".into(),
        verification: "无事实主张；未核实".into(),
        change_summary: "初始".into(),
        items: (0..n)
            .map(|i| PlanItem {
                id: format!("cat-{i}"),
                position: i,
                title: format!("猫 {i}"),
                copy: "无".into(),
                description: format!("第 {i} 个姿态"),
                requirements: "无额外要求".into(),
            })
            .collect(),
    }
}
fn planned(s: &Store, n: usize) -> Job {
    let d = draft(s, "planned", Some(n));
    let p = s.save_plan(&d, content(n), None).unwrap();
    s.start_job(
        &d.id,
        d.input_revision,
        Some((&p.id, &p.content_hash)),
        &planning::id(),
    )
    .unwrap()
}
fn output(s: &Store, i: &Item) -> PendingArtwork {
    let mut p = PendingArtwork::new(
        &format!(
            "data:image/png;base64,{}",
            STANDARD.encode(include_bytes!("../icons/icon.png"))
        ),
        i.prompt.clone(),
        i.snapshot.clone(),
        format!("actual prompt {}", i.position),
    )
    .unwrap();
    p.artwork.id = i.id.clone();
    p.artwork.group_id = s.job(&i.job_id).unwrap().group_id;
    p.artwork.position = i.position;
    p.artwork.generation_item_id = Some(i.id.clone());
    p.artwork.image_path = format!("images/{}.png", i.id);
    p.artwork.thumbnail_path = format!("thumbnails/{}.webp", i.id);
    p
}
fn complete(s: &Store, i: &Item, a: &Attempt) {
    s.stage_result(i, a, &output(s, i)).unwrap();
    s.save_item(&i.id).unwrap();
}

#[test]
fn quick_creates_exactly_one_item_without_a_plan_and_idempotent_admission() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let d = draft(&s, "quick", Some(12));
    let j = s
        .start_job(&d.id, d.input_revision, None, "one-click")
        .unwrap();
    assert_eq!(j.expected_count, 1);
    assert!(j.plan_id.is_none());
    assert!(s
        .records::<planning::Plan>("generation_plans")
        .unwrap()
        .is_empty());
    assert_eq!(
        s.start_job(&d.id, d.input_revision, None, "one-click")
            .unwrap()
            .id,
        j.id
    );
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    complete(&s, &i, &a);
    assert!(s.next_item(&j.id).unwrap().is_none());
    assert_eq!(s.snapshot_job(&j.id).unwrap().saved_count, 1);
}
#[test]
fn plans_require_current_revision_hash_applied_feedback_and_explicit_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let mut d = draft(&s, "planned", Some(2));
    let first = s.save_plan(&d, content(2), None).unwrap();
    assert!(s
        .start_job(&d.id, d.input_revision, None, "no-confirm")
        .is_err());
    for n in 0..2 {
        d = s.draft(&d.id).unwrap();
        d.feedback_draft = format!("调整 {n}");
        s.put_draft(&d).unwrap();
        assert!(s
            .start_job(
                &d.id,
                d.input_revision,
                Some((&first.id, &first.content_hash)),
                "feedback"
            )
            .is_err());
        let mut bad = content(2);
        bad.items[1].id = bad.items[0].id.clone();
        assert!(s.save_plan(&d, bad, Some(first.id.clone())).is_err());
        assert_eq!(s.draft(&d.id).unwrap().feedback_draft, d.feedback_draft);
        s.save_plan(&d, content(2), d.current_plan_id.clone())
            .unwrap();
    }
    let versions = s.records::<planning::Plan>("generation_plans").unwrap();
    assert_eq!(versions.len(), 3);
    assert!(s.records::<Job>("generation_jobs").unwrap().is_empty());
    d = s.draft(&d.id).unwrap();
    let p = s.plan(d.current_plan_id.as_deref().unwrap()).unwrap();
    assert!(s
        .start_job(&d.id, d.input_revision, Some((&p.id, "wrong hash")), "bad")
        .is_err());
    let copied = s.select_plan(&d.id, &first.id).unwrap();
    assert_eq!(copied.version, 4);
    assert_ne!(copied.id, first.id);
    let j = s
        .start_job(
            &d.id,
            d.input_revision,
            Some((&copied.id, &copied.content_hash)),
            "confirm",
        )
        .unwrap();
    assert_eq!(
        s.start_job(
            &d.id,
            d.input_revision,
            Some((&copied.id, &copied.content_hash)),
            "confirm"
        )
        .unwrap()
        .id,
        j.id
    );
    assert_eq!(s.records::<Job>("generation_jobs").unwrap().len(), 1);
    d.prompt = "different".into();
    d.input_revision += 1;
    s.put_draft(&d).unwrap();
    assert!(s
        .start_job(
            &d.id,
            d.input_revision,
            Some((&p.id, &p.content_hash)),
            "stale"
        )
        .is_err());
    assert!(s.select_plan(&d.id, &first.id).is_err());
}
#[test]
fn planner_validates_count_structure_and_limits() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let mut d = draft(&s, "planned", Some(2));
    assert_eq!(planning::count(&d).unwrap(), Some(2));
    assert!(planning::validate(&content(3), &d).is_err());
    d.requested_count = None;
    assert_eq!(planning::count(&d).unwrap(), Some(3));
    for prompt in [
        "一百张图",
        "零张图",
        "999999999999999999999999张图",
        "Create 13 images",
    ] {
        d.prompt = prompt.into();
        assert!(planning::count(&d).is_err());
    }
    d.prompt = "制作四个知识卡片".into();
    assert_eq!(planning::count(&d).unwrap(), Some(4));
    d.prompt = "画 13 张猫".into();
    assert!(planning::count(&d).is_err());
    d.prompt = "猫咪".into();
    assert_eq!(planning::count(&d).unwrap(), None);
    let mut c = content(3);
    c.items[1].position = 4;
    assert!(planning::validate(&c, &d).is_err());
    c = content(3);
    c.items[1].copy.clear();
    assert!(planning::validate(&c, &d).is_err());
    c = content(3);
    c.items[1].copy = "x".repeat(129 * 1024);
    assert!(planning::validate(&c, &d).is_err());
}

#[test]
fn admission_keys_reject_changed_requests_and_discard_is_explicit_and_persistent() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let d = draft(&s, "quick", None);
    let j = s.start_job(&d.id, d.input_revision, None, "key").unwrap();
    assert!(s
        .start_job(&d.id, d.input_revision + 1, None, "key")
        .is_err());
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    let p = output(&s, &i);
    s.stage_result(&i, &a, &p).unwrap();
    assert!(s.discard_result(&i.id).is_err());
    s.fail_item(&i, &a, "save failed".into(), true).unwrap();
    s.discard_result(&i.id).unwrap();
    assert!(s.save_item(&i.id).is_err());
    assert!(s.stage_result(&i, &a, &p).is_err());
    drop(s);
    let s = Store::open(dir.path().into()).unwrap();
    let snap = s.snapshot_job(&j.id).unwrap();
    assert_eq!(snap.generated_count, 1);
    assert_eq!(snap.saved_count, 0);
    assert_eq!(snap.pending_save_count, 0);
    assert_eq!(snap.items[0].save_status, "discarded");
    assert!(s.exportable(&i.id).is_err());
}

#[test]
fn group_cover_and_source_survive_reuse_and_source_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 2);
    let (first, a) = s.next_item(&j.id).unwrap().unwrap();
    complete(&s, &first, &a);
    let (second, a) = s.next_item(&j.id).unwrap().unwrap();
    complete(&s, &second, &a);
    assert_eq!(
        s.set_cover(&j.group_id, &second.id).unwrap().cover_id,
        Some(second.id.clone())
    );
    let mut d = draft(&s, "quick", None);
    d.source_group_id = Some(j.group_id.clone());
    d.source_photo_id = Some(first.id.clone());
    s.put_draft(&d).unwrap();
    let reused = s.start_job(&d.id, d.input_revision, None, "reuse").unwrap();
    assert!(
        s.group_detail(&reused.group_id, false)
            .unwrap()
            .source_available
    );
    s.update(&first.id, "deleted source".into(), vec![], false, true)
        .unwrap();
    s.purge(&first.id).unwrap();
    let detail = s.group_detail(&reused.group_id, false).unwrap();
    assert!(!detail.source_available);
    assert_eq!(detail.group.source_photo_id, Some(first.id));
    assert!(s.set_cover(&reused.group_id, &second.id).is_err());
}
#[test]
fn first_result_is_readable_before_next_finishes_and_failure_retry_keeps_positions() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 3);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    complete(&s, &i, &a);
    let (second, attempt) = s.next_item(&j.id).unwrap().unwrap();
    let midway = s.snapshot_job(&j.id).unwrap();
    assert_eq!(midway.saved_count, 1);
    assert_eq!(midway.items[1].generation_status, "generating");
    assert!(s.image(&i.id).is_ok());
    s.fail_item(&second, &attempt, "无效的 PNG".into(), false)
        .unwrap();
    let (third, a) = s.next_item(&j.id).unwrap().unwrap();
    assert_eq!(third.position, 2);
    complete(&s, &third, &a);
    s.next_item(&j.id).unwrap();
    let partial = s.snapshot_job(&j.id).unwrap();
    assert_eq!(partial.job.status, "partial");
    assert_eq!(partial.saved_count, 2);
    assert_eq!(partial.items[1].position, 1);
    s.resume_job(
        &j.id,
        partial.job.revision,
        Some((&second.id, second.attempt_no)),
    )
    .unwrap();
    let (retry, a) = s.next_item(&j.id).unwrap().unwrap();
    assert_eq!(retry.id, second.id);
    assert_eq!(retry.attempt_no, 2);
    assert!(s
        .stage_result(&second, &attempt, &output(&s, &second))
        .is_err());
    complete(&s, &retry, &a);
    assert!(s.next_item(&j.id).unwrap().is_none());
    let done = s.snapshot_job(&j.id).unwrap();
    assert_eq!(done.saved_count, 3);
    assert_eq!(done.job.expected_count, 3);
    assert_eq!(done.job.status, "completed");
    assert_eq!(s.items(&j.id).unwrap()[0].attempt_no, 1);
}
#[test]
fn stop_finishes_current_image_and_resume_never_regenerates_saved_positions() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 3);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    s.stop_job(&j.id).unwrap();
    complete(&s, &i, &a);
    assert!(s.next_item(&j.id).unwrap().is_none());
    let snap = s.snapshot_job(&j.id).unwrap();
    assert_eq!(snap.saved_count, 1);
    assert_eq!(snap.items[1].generation_status, "cancelled");
    s.resume_job(&j.id, snap.job.revision, None).unwrap();
    assert_eq!(s.next_item(&j.id).unwrap().unwrap().0.position, 1);
}
#[test]
fn save_failure_pauses_and_retries_existing_bytes_without_new_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 3);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    s.stage_result(&i, &a, &output(&s, &i)).unwrap();
    s.db.execute_batch("CREATE TRIGGER fail_save BEFORE INSERT ON artworks BEGIN SELECT RAISE(FAIL,'disk full'); END;").unwrap();
    let error = s.save_item(&i.id).err().unwrap();
    s.fail_item(&i, &a, error, true).unwrap();
    assert!(s.next_item(&j.id).unwrap().is_none());
    assert_eq!(s.job(&j.id).unwrap().status, "paused");
    assert!(s.staged(&i.id).is_ok());
    assert!(s
        .resume_job(&j.id, s.job(&j.id).unwrap().revision, None)
        .is_err());
    s.db.execute_batch("DROP TRIGGER fail_save").unwrap();
    let retained = output(&s, &i);
    // The gallery and workspace use this same path, including idempotent saved retries.
    s.retry_item_save(&i.id, None).unwrap();
    s.retry_item_save(&i.id, Some(&retained)).unwrap();
    assert_eq!(s.item(&i.id).unwrap().attempt_no, 1);
    assert_eq!(s.snapshot_job(&j.id).unwrap().saved_count, 1);
}
#[test]
fn restart_recovers_manifest_after_database_failure_and_leaves_other_work_interrupted() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 3);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    s.db.execute_batch("CREATE TRIGGER fail_stage BEFORE UPDATE ON generation_items BEGIN SELECT RAISE(FAIL,'commit failed'); END;").unwrap();
    assert!(s.stage_result(&i, &a, &output(&s, &i)).is_err());
    assert!(s.staged(&i.id).is_ok());
    s.db.execute_batch("DROP TRIGGER fail_stage").unwrap();
    drop(s);
    let s = Store::open(dir.path().into()).unwrap();
    let snap = s.snapshot_job(&j.id).unwrap();
    assert_eq!(snap.saved_count, 1);
    assert_eq!(snap.items[1].attempt_no, 0);
    assert_eq!(snap.job.status, "interrupted");
    assert_eq!(s.all().unwrap().len(), 1);
    drop(s);
    let s = Store::open(dir.path().into()).unwrap();
    assert_eq!(s.all().unwrap().len(), 1);
    assert!(s.staged(&i.id).is_err());
    s.resume_job(&j.id, s.job(&j.id).unwrap().revision, None)
        .unwrap();
    let (second, _) = s.next_item(&j.id).unwrap().unwrap();
    drop(s);
    let s = Store::open(dir.path().into()).unwrap();
    assert_eq!(s.item(&second.id).unwrap().generation_status, "interrupted");
}
#[test]
fn cleanup_after_committed_save_is_idempotent_and_deletion_does_not_change_history() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 1);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    let p = output(&s, &i);
    complete(&s, &i, &a);
    // Re-create pre-cleanup remnants as if power failed immediately after the commit.
    fs::write(dir.path().join(&a.staging_path), &p.bytes).unwrap();
    fs::write(
        dir.path().join(format!("staging/{}.json", a.id)),
        b"old manifest",
    )
    .unwrap();
    drop(s);
    let s = Store::open(dir.path().into()).unwrap();
    assert_eq!(s.all().unwrap().len(), 1);
    assert!(!dir.path().join(&a.staging_path).exists());
    s.update(&i.id, "deleted".into(), vec![], false, true)
        .unwrap();
    let snap = s.snapshot_job(&j.id).unwrap();
    assert_eq!(snap.saved_count, 1);
    assert_eq!(snap.available_count, 0);
    assert_eq!(snap.job.status, "completed");
}
#[test]
fn unknown_shared_errors_pause_and_active_groups_cannot_be_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 2);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    assert!(s.assert_group_idle(&j.group_id).is_err());
    s.fail_item(&i, &a, "login expired".into(), false).unwrap();
    assert!(s.next_item(&j.id).unwrap().is_none());
    assert_eq!(s.items(&j.id).unwrap()[1].attempt_no, 0);
    assert!(s
        .list_groups("", false, false, false, 1, &[])
        .unwrap()
        .items
        .is_empty());
}

fn old_database(
    root: &std::path::Path,
    grouped: bool,
) -> (Connection, Vec<crate::gallery::Artwork>) {
    let db = Connection::open(root.join("gallery.db")).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE artworks(id TEXT PRIMARY KEY,title TEXT NOT NULL,prompt TEXT NOT NULL,created_at TEXT NOT NULL,favorite INTEGER NOT NULL DEFAULT 0,deleted INTEGER NOT NULL DEFAULT 0,metadata TEXT NOT NULL);").unwrap();
    if grouped {
        db.execute_batch("CREATE TABLE photo_groups(id TEXT PRIMARY KEY,metadata TEXT NOT NULL)")
            .unwrap();
    }
    let mut group = PhotoGroup::new("legacy cats".into(), Snapshot::default(), 3, None);
    group.id = "existing-group".into();
    group.title = "edited group".into();
    group.favorite = true;
    group.tags = vec!["group-tag".into()];
    group.deleted_photo_ids = vec!["prior".into()];
    group.source_photo_id = Some("gone".into());
    if grouped {
        db.execute(
            "INSERT INTO photo_groups VALUES(?,?)",
            params![group.id, planning::json(&group).unwrap()],
        )
        .unwrap();
    }
    let mut photos = vec![];
    for n in 0..3 {
        let p = PendingArtwork::new(
            &format!(
                "data:image/png;base64,{}",
                STANDARD.encode(include_bytes!("../icons/icon.png"))
            ),
            "original".into(),
            Snapshot::default(),
            "full conditions".into(),
        )
        .unwrap();
        let mut a = p.artwork;
        a.id = format!("legacy-{n}");
        a.favorite = true;
        a.tags = vec!["tag".into()];
        a.deleted = n == 1;
        a.group_id = if grouped {
            group.id.clone()
        } else {
            String::new()
        };
        a.position = 0;
        let original = root.join(&a.image_path);
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(original, &p.bytes).unwrap();
        db.execute(
            "INSERT INTO artworks VALUES(?,?,?,?,?,?,?)",
            params![
                a.id,
                a.title,
                a.prompt,
                a.created_at,
                a.favorite,
                a.deleted,
                planning::json(&a).unwrap()
            ],
        )
        .unwrap();
        photos.push(a);
    }
    (db, photos)
}
#[test]
fn both_legacy_schemas_migrate_without_losing_metadata_and_wal_backup_has_commits() {
    for grouped in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (db, photos) = old_database(dir.path(), grouped);
        // Keep WAL writer alive while backup reads its committed pages.
        crate::migrations::migrate(&db, dir.path()).unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        let backup = fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "db"))
            .unwrap();
        let b = Connection::open(backup).unwrap();
        assert_eq!(
            b.query_row("SELECT count(*) FROM artworks", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            b.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(db);
        let s = Store::open(dir.path().into()).unwrap();
        for a in &photos {
            let got = s.get(&a.id).unwrap();
            assert_eq!(got.image_path, a.image_path);
            assert_eq!(got.snapshot.catalog_version, a.snapshot.catalog_version);
            assert_eq!(got.submitted_prompt, a.submitted_prompt);
            assert_eq!(got.tags, a.tags);
            assert_eq!(got.deleted, a.deleted);
            assert!(got.favorite);
            let g = s.group(&got.group_id).unwrap();
            assert_eq!(g.mode, "legacy");
            if grouped {
                assert_eq!(g.id, "existing-group");
                assert_eq!(g.title, "edited group");
                assert_eq!(g.source_photo_id.as_deref(), Some("gone"));
                assert_eq!(g.deleted_photo_ids, vec!["prior"]);
                assert_eq!(g.status, "interrupted");
            } else {
                assert_eq!(got.group_id, a.id);
                assert_eq!(g.deleted, a.deleted);
                assert_eq!(g.tags, a.tags);
            }
            assert!(dir.path().join(&got.thumbnail_path).is_file());
        }
        assert_eq!(
            s.db.query_row("SELECT count(*) FROM schema_migrations", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(s);
        assert!(Store::open(dir.path().into()).is_ok());
        assert_eq!(
            fs::read_dir(dir.path().join("backups"))
                .unwrap()
                .filter(|e| e
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "db"))
                .count(),
            1
        );
    }
}
#[test]
fn migration_rolls_back_on_midway_failure_then_retries_without_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let (db, _) = old_database(dir.path(), false);
    db.execute_batch("CREATE TRIGGER fail_migration BEFORE UPDATE ON artworks BEGIN SELECT RAISE(FAIL,'injected'); END;").unwrap();
    assert!(crate::migrations::migrate(&db, dir.path()).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(db.prepare("SELECT * FROM generation_jobs").is_err());
    assert_eq!(
        db.query_row("SELECT count(*) FROM artworks", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    db.execute_batch("DROP TRIGGER fail_migration").unwrap();
    crate::migrations::migrate(&db, dir.path()).unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM photo_groups", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        3
    );
}
#[test]
fn higher_unknown_and_corrupt_databases_are_not_mutated() {
    for sql in [
        "PRAGMA user_version=9;CREATE TABLE keep(value TEXT);",
        "CREATE TABLE artworks(id TEXT);",
        "CREATE TABLE strange(id TEXT);",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = Connection::open(dir.path().join("gallery.db")).unwrap();
        db.execute_batch(sql).unwrap();
        drop(db);
        let before = fs::read(dir.path().join("gallery.db")).unwrap();
        assert!(Store::open(dir.path().into()).is_err());
        assert_eq!(fs::read(dir.path().join("gallery.db")).unwrap(), before);
    }
    let dir = tempfile::tempdir().unwrap();
    let (db, _) = old_database(dir.path(), false);
    db.execute("UPDATE artworks SET metadata='bad JSON'", [])
        .unwrap();
    assert!(crate::migrations::migrate(&db, dir.path()).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn conflicting_files_and_saved_ids_are_never_overwritten_or_removed() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 1);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    let p = output(&s, &i);
    let original = dir.path().join(&p.artwork.image_path);
    fs::write(&original, b"quarantined bytes").unwrap();
    s.stage_result(&i, &a, &p).unwrap();
    assert!(s.save_item(&i.id).is_err());
    assert_eq!(fs::read(&original).unwrap(), b"quarantined bytes");
    assert!(s.staged(&i.id).is_ok());
    fs::remove_file(&original).unwrap();
    s.save_item(&i.id).unwrap();
    let mut different = output(&s, &i);
    different.bytes.push(0);
    assert!(s.save(&different).is_err());
    assert_eq!(fs::read(original).unwrap(), p.bytes);
}

#[test]
fn retry_one_paused_failure_does_not_dispatch_other_unstarted_items() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 3);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    s.fail_item(&i, &a, "unknown service failure".into(), false)
        .unwrap();
    s.resume_job(
        &j.id,
        s.job(&j.id).unwrap().revision,
        Some((&i.id, i.attempt_no)),
    )
    .unwrap();
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    complete(&s, &i, &a);
    assert!(s.next_item(&j.id).unwrap().is_none());
    assert_eq!(s.items(&j.id).unwrap()[1].attempt_no, 0);
    assert_eq!(s.snapshot_job(&j.id).unwrap().job.status, "partial");
}

#[test]
fn persistent_pending_result_can_be_exported_after_restart_without_model_call() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 2);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    s.stage_result(&i, &a, &output(&s, &i)).unwrap();
    s.db.execute_batch("CREATE TRIGGER fail_save BEFORE INSERT ON artworks BEGIN SELECT RAISE(FAIL,'disk full'); END;").unwrap();
    drop(s);
    let s = Store::open(dir.path().into()).unwrap();
    let (artwork, bytes) = s.exportable(&i.id).unwrap();
    assert_eq!(artwork.id, i.id);
    assert!(!bytes.is_empty());
    assert_eq!(s.item(&i.id).unwrap().attempt_no, 1);
    assert_eq!(s.job(&j.id).unwrap().status, "paused");
}

#[test]
fn staging_disk_failure_keeps_result_unsaved_and_blocks_further_generation() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 2);
    let (i, a) = s.next_item(&j.id).unwrap().unwrap();
    let p = output(&s, &i);
    fs::remove_dir(dir.path().join("staging")).unwrap();
    fs::write(dir.path().join("staging"), b"unwritable").unwrap();
    let error = s.stage_result(&i, &a, &p).err().unwrap();
    s.fail_item(&i, &a, error, true).unwrap();
    assert!(s.next_item(&j.id).unwrap().is_none());
    assert_eq!(s.snapshot_job(&j.id).unwrap().saved_count, 0);
    fs::remove_file(dir.path().join("staging")).unwrap();
    fs::create_dir(dir.path().join("staging")).unwrap();
    s.stage_result(&i, &a, &p).unwrap();
    s.save_item(&i.id).unwrap();
    assert_eq!(s.item(&i.id).unwrap().attempt_no, 1);
}

#[test]
fn malformed_managed_schema_is_rejected_before_recovery_mutates_jobs() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(dir.path().into()).unwrap();
    let j = planned(&s, 1);
    s.db.execute_batch("ALTER TABLE generation_items ADD COLUMN unknown TEXT")
        .unwrap();
    drop(s);
    assert!(Store::open(dir.path().into()).is_err());
    let db = Connection::open(dir.path().join("gallery.db")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT status FROM generation_jobs WHERE id=?",
            [&j.id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "queued"
    );
}

#[test]
fn managed_metadata_and_unique_index_are_checked_before_any_recovery_write() {
    for damage in ["metadata", "index"] {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open(dir.path().into()).unwrap();
        let j = planned(&s, 2);
        let (i, a) = s.next_item(&j.id).unwrap().unwrap();
        complete(&s, &i, &a);
        let before = s.job(&j.id).unwrap();
        if damage == "metadata" {
            s.db.execute(
                "UPDATE artworks SET metadata=json_remove(metadata,'$.snapshot') WHERE id=?",
                [&i.id],
            )
            .unwrap();
        } else {
            s.db.execute_batch("DROP INDEX artwork_item").unwrap();
        }
        drop(s);
        assert!(Store::open(dir.path().into()).is_err(), "{damage}");
        let db = Connection::open(dir.path().join("gallery.db")).unwrap();
        let (status, revision): (String, i64) = db
            .query_row(
                "SELECT status,revision FROM generation_jobs WHERE id=?",
                [&j.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, before.status);
        assert_eq!(revision, before.revision as i64);
        assert_eq!(
            db.query_row("SELECT count(*) FROM artworks", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}

#[test]
fn legacy_invalid_order_is_repaired_but_valid_gaps_and_original_json_are_preserved() {
    for invalid in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let (db, photos) = old_database(dir.path(), true);
        for (n, a) in photos.iter().enumerate() {
            let mut value = serde_json::to_value(a).unwrap();
            value["position"] = if invalid {
                serde_json::json!(-1)
            } else {
                serde_json::json!(n * 2)
            };
            db.execute(
                "UPDATE artworks SET metadata=? WHERE id=?",
                params![value.to_string(), a.id],
            )
            .unwrap();
        }
        crate::migrations::migrate(&db, dir.path()).unwrap();
        let mut q = db
            .prepare("SELECT position FROM artworks ORDER BY position")
            .unwrap();
        let positions: Vec<i64> = q
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            positions,
            if invalid {
                vec![0, 1, 2]
            } else {
                vec![0, 2, 4]
            }
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM migration_original_groups", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
    }
}
