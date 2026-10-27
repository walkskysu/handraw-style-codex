//! Schema changes run before recovery, under Store's lifetime OS lock.
use crate::{gallery::Artwork, groups::PhotoGroup};
use rusqlite::{params, Connection};
use std::{collections::BTreeMap, fs, path::Path};

pub const VERSION: i64 = 1;
type Result<T> = std::result::Result<T, String>;
const ARTWORKS: &str = "CREATE TABLE artworks(id TEXT PRIMARY KEY,title TEXT NOT NULL,prompt TEXT NOT NULL,created_at TEXT NOT NULL,favorite INTEGER NOT NULL DEFAULT 0,deleted INTEGER NOT NULL DEFAULT 0,metadata TEXT NOT NULL);";

fn tables(db: &Connection) -> Result<Vec<String>> {
    db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .and_then(|mut q| q.query_map([], |r| r.get(0))?.collect())
        .map_err(|e| e.to_string())
}
fn columns(db: &Connection, table: &str, expected: &[&str]) -> Result<()> {
    let actual: Vec<String> = db
        .prepare(&format!("PRAGMA table_xinfo({table})"))
        .and_then(|mut q| q.query_map([], |r| r.get(1))?.collect())
        .map_err(|e| e.to_string())?;
    if expected.iter().any(|c| !actual.iter().any(|a| a == c)) || actual.len() != expected.len() {
        return Err(format!("未知数据库结构：{table}；原库未修改。"));
    }
    Ok(())
}

pub fn migrate(db: &Connection, root: &Path) -> Result<()> {
    db.execute_batch("PRAGMA foreign_keys=ON")
        .map_err(|e| e.to_string())?;
    let version: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if version > VERSION {
        return Err("数据库来自新版应用，请使用新版 Handraw；未修改数据库。".into());
    }
    if version == VERSION {
        return validate_managed(db);
    }
    let existing = tables(db)?;
    if existing
        .iter()
        .any(|t| t != "artworks" && t != "photo_groups")
        || (!existing.is_empty() && !existing.iter().any(|t| t == "artworks"))
    {
        return Err("未知数据库结构；停止升级，原库未修改。".into());
    }
    let mut photos: Vec<Artwork> = vec![];
    let mut groups: BTreeMap<String, PhotoGroup> = BTreeMap::new();
    let mut originals = vec![];
    let mut original_groups = vec![];
    let mut repaired_positions = vec![];
    if !existing.is_empty() {
        columns(
            db,
            "artworks",
            &[
                "id",
                "title",
                "prompt",
                "created_at",
                "favorite",
                "deleted",
                "metadata",
            ],
        )?;
        let mut query = db
            .prepare("SELECT id,metadata FROM artworks")
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (id, json) = row.map_err(|e| e.to_string())?;
            let mut value: serde_json::Value =
                serde_json::from_str(&json).map_err(|e| format!("损坏的图片元数据 {id}: {e}"))?;
            if !value.is_object() {
                return Err(format!("损坏的图片元数据 {id}：需要对象"));
            }
            // Invalid/missing legacy order is repairable; preserve the exact source JSON below.
            if value.get("position").and_then(|p| p.as_u64()).is_none() {
                value["position"] = serde_json::json!(usize::MAX);
            }
            let a: Artwork =
                serde_json::from_value(value).map_err(|e| format!("损坏的图片元数据 {id}: {e}"))?;
            if a.id != id {
                return Err("图片 ID 与元数据不一致".into());
            }
            for relative in [&a.image_path, &a.thumbnail_path] {
                if relative.is_empty()
                    || Path::new(relative)
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                {
                    return Err(format!("历史图片路径无效：{id}；停止迁移"));
                }
            }
            originals.push((id, json));
            photos.push(a);
        }
        if existing.iter().any(|t| t == "photo_groups") {
            columns(db, "photo_groups", &["id", "metadata"])?;
            let mut query = db
                .prepare("SELECT id,metadata FROM photo_groups")
                .map_err(|e| e.to_string())?;
            let rows = query
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (id, json) = row.map_err(|e| e.to_string())?;
                let mut group: PhotoGroup =
                    serde_json::from_str(&json).map_err(|e| format!("损坏的组元数据 {id}: {e}"))?;
                if group.id != id {
                    return Err("组 ID 与元数据不一致".into());
                }
                original_groups.push((id.clone(), json));
                group.mode = "legacy".into();
                if group.status == "generating" {
                    group.status = "interrupted".into();
                }
                groups.insert(id, group);
            }
        }
        // SQLite's backup API includes committed WAL pages. Never copy gallery.db alone.
        let check: String = db
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if check != "ok" {
            return Err("旧库完整性检查失败；停止升级".into());
        }
        check_space(db, root)?;
        let folder = root.join("backups");
        fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let backup = folder.join(format!(
            "gallery-v0-{}-{}.db",
            chrono::Utc::now().format("%Y%m%d-%H%M%S"),
            uuid::Uuid::new_v4()
        ));
        db.backup("main", &backup, None)
            .map_err(|e| format!("备份失败，停止升级：{e}"))?;
        let checked =
            Connection::open_with_flags(&backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())?;
        let check: String = checked
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if check != "ok" {
            return Err("备份校验失败，停止升级".into());
        }
        fs::write(backup.with_extension("json"), serde_json::json!({"app_version":env!("CARGO_PKG_VERSION"),"from":0,"to":VERSION,"artworks":photos.len(),"integrity_check":check}).to_string()).map_err(|e| e.to_string())?;
    }
    for a in &mut photos {
        if a.group_id.is_empty() {
            if groups.contains_key(&a.id) {
                return Err(format!("历史图片补组冲突：{}", a.id));
            }
            a.group_id = a.id.clone();
        }
        if !groups.contains_key(&a.group_id) {
            let mut g = PhotoGroup::new(a.prompt.clone(), a.snapshot.clone(), 1, None);
            g.id = a.group_id.clone();
            g.mode = "legacy".into();
            g.title = a.title.clone();
            g.favorite = a.favorite;
            g.tags = a.tags.clone();
            g.created_at = a.created_at.clone();
            g.cover_id = Some(a.id.clone());
            g.status = "complete".into();
            g.generated_count = 1;
            g.deleted = a.deleted;
            if a.deleted {
                g.deleted_photo_ids.push(a.id.clone());
            }
            groups.insert(g.id.clone(), g);
        }
    }
    photos.sort_by(|a, b| {
        (&a.group_id, a.position, &a.created_at, &a.id).cmp(&(
            &b.group_id,
            b.position,
            &b.created_at,
            &b.id,
        ))
    });
    let mut seen = std::collections::HashSet::new();
    let repairs: std::collections::HashSet<_> = photos
        .iter()
        .filter(|a| a.position == usize::MAX || !seen.insert((a.group_id.clone(), a.position)))
        .map(|a| a.group_id.clone())
        .collect();
    let mut positions = BTreeMap::new();
    for a in &mut photos {
        if !repairs.contains(&a.group_id) {
            continue;
        }
        let next = positions.entry(a.group_id.clone()).or_insert(0);
        if a.position != *next {
            repaired_positions.push(a.id.clone());
        }
        a.position = *next;
        *next += 1;
    }
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    if existing.is_empty() {
        db.execute_batch(ARTWORKS).map_err(|e| e.to_string())?;
    }
    db.execute_batch(include_str!("schema.sql"))
        .map_err(|e| e.to_string())?;
    for (id, json) in originals {
        db.execute(
            "INSERT INTO migration_originals(id,metadata) VALUES(?,?)",
            params![id, json],
        )
        .map_err(|e| e.to_string())?;
    }
    for (id, json) in original_groups {
        db.execute(
            "INSERT INTO migration_original_groups(id,metadata) VALUES(?,?)",
            params![id, json],
        )
        .map_err(|e| e.to_string())?;
    }
    for group in groups.values() {
        db.execute("INSERT INTO photo_groups(id,metadata) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET metadata=excluded.metadata",params![group.id,serde_json::to_string(group).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
    }
    for a in &photos {
        db.execute(
            "UPDATE artworks SET metadata=? WHERE id=?",
            params![serde_json::to_string(a).map_err(|e| e.to_string())?, a.id],
        )
        .map_err(|e| e.to_string())?;
    }
    let count: i64 = db
        .query_row("SELECT count(*) FROM artworks", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let broken: i64 = db.query_row("SELECT count(*) FROM artworks a LEFT JOIN photo_groups g ON a.group_id=g.id WHERE g.id IS NULL",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let fk: i64 = db
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    if count != photos.len() as i64 || broken != 0 || fk != 0 {
        return Err("迁移关联校验失败".into());
    }
    db.execute(
        "INSERT INTO schema_migrations VALUES(1,'creation jobs',?,?)",
        params![env!("CARGO_PKG_VERSION"), chrono::Utc::now().to_rfc3339()],
    )
    .map_err(|e| e.to_string())?;
    db.execute("INSERT INTO migration_summary VALUES(?)",[serde_json::json!({"artworks":photos.len(),"groups":groups.len(),"repaired_positions":repaired_positions,"backup_directory":root.join("backups")}).to_string()]).map_err(|e|e.to_string())?;
    db.pragma_update(None, "user_version", VERSION)
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn validate_managed(db: &Connection) -> Result<()> {
    // Check the actual managed schema before any recovery writes, not just its table names.
    let expected = Connection::open_in_memory().map_err(|e| e.to_string())?;
    expected
        .execute_batch(ARTWORKS)
        .and_then(|_| expected.execute_batch(include_str!("schema.sql")))
        .map_err(|e| e.to_string())?;
    for table in tables(&expected)? {
        let names: Vec<String> = expected
            .prepare(&format!("PRAGMA table_xinfo({table})"))
            .and_then(|mut q| q.query_map([], |r| r.get(1))?.collect())
            .map_err(|e| e.to_string())?;
        columns(
            db,
            &table,
            &names.iter().map(String::as_str).collect::<Vec<_>>(),
        )?;
    }
    for (kind, name) in [
        ("trigger", "group_insert"),
        ("trigger", "group_update"),
        ("trigger", "artwork_insert"),
        ("trigger", "artwork_update"),
        ("index", "artwork_item"),
        ("index", "artwork_group"),
        ("index", "group_order"),
        ("index", "job_state"),
        ("index", "planning_state"),
    ] {
        let actual: String = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type=? AND name=?",
                [kind, name],
                |r| r.get(0),
            )
            .map_err(|_| format!("数据库缺少一致性约束：{name}"))?;
        let expected: String = expected
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type=? AND name=?",
                [kind, name],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if actual != expected {
            return Err(format!("未知数据库约束：{name}"));
        }
    }
    let bad: i64 = db
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    if bad != 0 {
        return Err("数据库关联检查失败；停止写入".into());
    }
    // Preflight every record before recover_jobs can change any status or remove staged files.
    // A valid JSON object can still be missing required application metadata.
    validate_records::<Artwork>(db, "artworks")?;
    validate_records::<PhotoGroup>(db, "photo_groups")?;
    validate_records::<crate::planning::Draft>(db, "creation_drafts")?;
    validate_records::<crate::planning::Plan>(db, "generation_plans")?;
    validate_records::<crate::planning::PlanningJob>(db, "planning_jobs")?;
    validate_records::<crate::jobs::Job>(db, "generation_jobs")?;
    validate_records::<crate::jobs::Item>(db, "generation_items")?;
    validate_records::<crate::jobs::Attempt>(db, "generation_attempts")?;
    Ok(())
}

fn validate_records<T: serde::de::DeserializeOwned>(db: &Connection, table: &str) -> Result<()> {
    let mut query = db
        .prepare(&format!("SELECT id,metadata FROM {table}"))
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, json) = row.map_err(|e| e.to_string())?;
        let value: serde_json::Value = serde_json::from_str(&json)
            .map_err(|e| format!("损坏的元数据 {table}/{id}：{e}；停止恢复"))?;
        if value.get("id").and_then(|v| v.as_str()) != Some(id.as_str()) {
            return Err(format!("元数据 ID 不一致 {table}/{id}；停止恢复"));
        }
        serde_json::from_value::<T>(value)
            .map_err(|e| format!("损坏的元数据 {table}/{id}：{e}；停止恢复"))?;
    }
    Ok(())
}

fn check_space(db: &Connection, root: &Path) -> Result<()> {
    let pages: i64 = db
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let size: i64 = db
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    // Backup plus transaction/WAL headroom. Image bytes are never copied during migration.
    let required = (pages as u64)
        .saturating_mul(size as u64)
        .saturating_mul(3)
        .saturating_add(16 * 1024 * 1024);
    if available_space(root)? < required {
        return Err(format!(
            "升级空间不足，需要至少 {required} 字节空闲空间；原库未修改"
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn available_space(root: &Path) -> Result<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            path: *const u16,
            available: *mut u64,
            total: *mut u64,
            free: *mut u64,
        ) -> i32;
    }
    let path: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    // Valid, NUL-terminated path and writable u64; optional output pointers may be null.
    if unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(format!(
            "无法检查升级空间：{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(available)
}
#[cfg(not(windows))]
fn available_space(root: &Path) -> Result<u64> {
    let output = std::process::Command::new("df")
        .args(["-Pk"])
        .arg(root)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("无法检查升级空间".into());
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .last()
        .and_then(|l| l.split_whitespace().nth(3))
        .and_then(|s| s.parse::<u64>().ok())
        .map(|n| n.saturating_mul(1024))
        .ok_or("无法解析升级空间".into())
}
