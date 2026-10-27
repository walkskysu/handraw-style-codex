use crate::options::Snapshot;
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::Utc;
use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Serialize, Deserialize)]
pub struct Artwork {
    #[serde(default = "available")]
    pub availability: String,
    #[serde(default)]
    pub generation_item_id: Option<String>,
    #[serde(default = "metadata_version")]
    pub metadata_version: u32,
    pub id: String,
    #[serde(default)]
    pub group_id: String,
    #[serde(default)]
    pub position: usize,
    #[serde(default = "saved_status")]
    pub save_status: String,
    pub title: String,
    pub prompt: String,
    pub snapshot: Snapshot,
    pub submitted_prompt: String,
    pub created_at: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub file_size: usize,
    pub image_path: String,
    pub thumbnail_path: String,
    pub favorite: bool,
    pub tags: Vec<String>,
    pub deleted: bool,
    pub model: Option<String>,
    pub tool_version: Option<String>,
    pub app_version: String,
}

fn saved_status() -> String {
    "saved".into()
}
fn metadata_version() -> u32 {
    1
}
fn available() -> String {
    "available".into()
}
// Relations are read from columns; metadata is the immutable generation snapshot plus mirrors.
const RECORD: &str = "json_set(metadata,'$.group_id',group_id,'$.position',position,'$.generation_item_id',generation_item_id,'$.availability',availability)";

pub struct PendingArtwork {
    pub artwork: Artwork,
    pub bytes: Vec<u8>,
}

fn decode(bytes: &[u8]) -> Result<image::DynamicImage> {
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("图片超过 20 MB。".into());
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    let mut limits = Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    reader.decode().map_err(|e| format!("无效的 PNG 图片：{e}"))
}

impl PendingArtwork {
    pub fn new(
        data_url: &str,
        prompt: String,
        snapshot: Snapshot,
        submitted_prompt: String,
    ) -> Result<Self> {
        let bytes = STANDARD
            .decode(
                data_url
                    .strip_prefix("data:image/png;base64,")
                    .ok_or("无效的图片格式")?,
            )
            .map_err(|e| e.to_string())?;
        let (width, height) = decode(&bytes)?.dimensions();
        let now = Utc::now();
        let id = uuid::Uuid::new_v4().to_string();
        Ok(Self {
            artwork: Artwork {
                availability: available(),
                generation_item_id: None,
                metadata_version: 1,
                group_id: id.clone(),
                position: 0,
                save_status: "saved".into(),
                image_path: format!("images/{}/{id}.png", now.format("%Y-%m")),
                thumbnail_path: format!("thumbnails/{id}.webp"),
                id,
                title: prompt.chars().take(40).collect(),
                prompt,
                snapshot,
                submitted_prompt,
                created_at: now.to_rfc3339(),
                width,
                height,
                format: "png".into(),
                file_size: bytes.len(),
                favorite: false,
                tags: vec![],
                deleted: false,
                model: None,
                tool_version: None,
                app_version: env!("CARGO_PKG_VERSION").into(),
            },
            bytes,
        })
    }
}

pub struct Store {
    root: PathBuf,
    pub(super) db: Connection,
    // An OS lock prevents a second app instance from cleaning another instance's in-flight files.
    _lock: fs::File,
}

impl Store {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join("gallery.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|_| "图库正在被另一个 Handraw 窗口使用，请关闭其他窗口后重试。".to_string())?;
        let db = Connection::open(root.join("gallery.db")).map_err(|e| e.to_string())?;
        db.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        crate::migrations::migrate(&db, &root)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
            CREATE INDEX IF NOT EXISTS gallery_order ON artworks(deleted, created_at DESC, id DESC);")
            .map_err(|e| e.to_string())?;
        for folder in ["images", "thumbnails", "staging"] {
            fs::create_dir_all(root.join(folder)).map_err(|e| e.to_string())?;
        }
        let store = Self {
            root,
            db,
            _lock: lock,
        };
        store.recover_jobs()?;
        store.recover()?;
        Ok(store)
    }

    // Never accept filesystem paths from IPC or follow paths outside the application directory.
    pub(super) fn path(&self, relative: &str) -> Result<PathBuf> {
        let relative = Path::new(relative);
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
            || relative.as_os_str().is_empty()
        {
            return Err("无效的图库相对路径".into());
        }
        let mut path = self.root.clone();
        for component in relative.components() {
            path.push(component);
            if let Ok(meta) = fs::symlink_metadata(&path) {
                if meta.file_type().is_symlink() {
                    return Err("图库路径不能是符号链接".into());
                }
            }
        }
        Ok(path)
    }

    pub(super) fn all(&self) -> Result<Vec<Artwork>> {
        let mut query = self
            .db
            .prepare(&format!("SELECT {RECORD} FROM artworks"))
            .map_err(|e| e.to_string())?;
        let rows = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }

    fn recover(&self) -> Result<()> {
        for artwork in self.all()? {
            let image = self.path(&artwork.image_path)?;
            let thumbnail = self.path(&artwork.thumbnail_path)?;
            if image.is_file() && !thumbnail.is_file() {
                if let Ok(decoded) = self.bytes(&artwork).and_then(|b| decode(&b)) {
                    let mut bytes = Cursor::new(Vec::new());
                    decoded
                        .thumbnail(480, 480)
                        .write_to(&mut bytes, ImageFormat::WebP)
                        .map_err(|e| e.to_string())?;
                    self.persist(&thumbnail, bytes.get_ref())?;
                }
            }
            self.db
                .execute(
                    "UPDATE artworks SET availability=? WHERE id=?",
                    params![
                        if image.is_file() {
                            "available"
                        } else {
                            "missing"
                        },
                        artwork.id
                    ],
                )
                .map_err(|e| e.to_string())?;
        }
        // Unknown files are retained. They can be the only surviving result of a failed commit.
        Ok(())
    }

    pub fn recovery_notices(&self) -> Result<Vec<String>> {
        let mut known = std::collections::HashSet::new();
        for artwork in self.all()? {
            known.insert(artwork.image_path);
            known.insert(artwork.thumbnail_path);
        }
        for attempt in self.records::<crate::jobs::Attempt>("generation_attempts")? {
            known.insert(attempt.staging_path);
            known.insert(format!("staging/{}.json", attempt.id));
        }
        let mut unrecognized = 0;
        for folder in ["images", "thumbnails", "staging"] {
            for entry in fs::read_dir(self.path(folder)?).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                    continue;
                }
                if !known.contains(&format!("{folder}/{}", entry.file_name().to_string_lossy())) {
                    unrecognized += 1;
                }
            }
        }
        let missing: i64 = self
            .db
            .query_row(
                "SELECT count(*) FROM artworks WHERE availability='missing'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut notices = vec![];
        if unrecognized > 0 {
            notices.push(format!("发现 {unrecognized} 个无法确认归属的文件，已原地保留、隔离于普通图库，未自动清理。请备份完整图库目录后检查。"));
        }
        if missing > 0 {
            notices.push(format!(
                "{missing} 张原图文件缺失；元信息已保留，可从原有备份恢复文件。"
            ));
        }
        Ok(notices)
    }

    pub fn save(&self, pending: &PendingArtwork) -> Result<Artwork> {
        let exists: bool = self
            .db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM artworks WHERE id=?)",
                [&pending.artwork.id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists {
            let saved = self.get(&pending.artwork.id)?;
            if self.bytes(&saved)? != pending.bytes {
                return Err("已有图片与本次结果不同，拒绝覆盖".into());
            }
            return Ok(saved);
        }
        self.ensure_group(&pending.artwork)?;
        let image = decode(&pending.bytes)?;
        let mut thumbnail = Cursor::new(Vec::new());
        image
            .thumbnail(480, 480)
            .write_to(&mut thumbnail, ImageFormat::WebP)
            .map_err(|e| e.to_string())?;
        let original = self.path(&pending.artwork.image_path)?;
        let thumb = self.path(&pending.artwork.thumbnail_path)?;
        let had_original = original.exists();
        let had_thumbnail = thumb.exists();
        fs::create_dir_all(original.parent().ok_or("缺少图片目录")?).map_err(|e| e.to_string())?;
        let result = (|| {
            self.persist(&original, &pending.bytes)?;
            self.persist(&thumb, thumbnail.get_ref())?;
            self.db.execute("INSERT INTO artworks(id,title,prompt,created_at,favorite,deleted,metadata) VALUES(?,?,?,?,?,?,?)",
                params![pending.artwork.id, pending.artwork.title, pending.artwork.prompt, pending.artwork.created_at,
                    pending.artwork.favorite, pending.artwork.deleted, serde_json::to_string(&pending.artwork).map_err(|e| e.to_string())?])
                .map_err(|e| e.to_string())?;
            Ok(pending.artwork.clone())
        })();
        if result.is_err() {
            // Never remove a pre-existing (possibly quarantined) file after a conflict.
            if !had_original {
                let _ = fs::remove_file(original);
            }
            if !had_thumbnail {
                let _ = fs::remove_file(thumb);
            }
        }
        result
    }

    pub(super) fn persist(&self, destination: &Path, bytes: &[u8]) -> Result<()> {
        if destination.exists() {
            if fs::read(destination).map_err(|e| e.to_string())? == bytes {
                return Ok(());
            }
            return Err("目标文件已有不同内容，拒绝覆盖".into());
        }
        let mut file =
            tempfile::NamedTempFile::new_in(self.path("staging")?).map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist_noclobber(destination)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Artwork> {
        let json: String = self
            .db
            .query_row(
                &format!("SELECT {RECORD} FROM artworks WHERE id=?"),
                [id],
                |r| r.get(0),
            )
            .map_err(|_| "作品不存在或已永久删除".to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    pub fn image(&self, id: &str) -> Result<String> {
        let artwork = self.get(id)?;
        Ok(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(self.bytes(&artwork)?)
        ))
    }

    pub fn bytes(&self, artwork: &Artwork) -> Result<Vec<u8>> {
        fs::read(self.path(&artwork.image_path)?).map_err(|e| format!("无法读取原图：{e}"))
    }

    pub fn update(
        &self,
        id: &str,
        title: String,
        tags: Vec<String>,
        favorite: bool,
        deleted: bool,
    ) -> Result<Artwork> {
        if title.trim().is_empty()
            || title.chars().count() > 200
            || tags.len() > 30
            || tags.iter().any(|t| t.chars().count() > 80)
        {
            return Err("标题需为 1–200 字；最多 30 个标签，每个不超过 80 字。".into());
        }
        let mut a = self.get(id)?;
        if a.deleted != deleted {
            self.assert_group_idle(&a.group_id)?;
        }
        let restoring = a.deleted && !deleted;
        a.title = title.trim().into();
        a.tags = tags;
        a.favorite = favorite;
        a.deleted = deleted;
        self.db
            .execute(
                "UPDATE artworks SET title=?,favorite=?,deleted=?,metadata=? WHERE id=?",
                params![
                    a.title,
                    a.favorite,
                    a.deleted,
                    serde_json::to_string(&a).map_err(|e| e.to_string())?,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
        self.repair_cover(&a.group_id)?;
        if restoring {
            let mut group = self.group(&a.group_id)?;
            group.deleted = false;
            self.put_group(&group)?;
        }
        Ok(a)
    }

    pub fn purge(&self, id: &str) -> Result<()> {
        let a = self.get(id)?;
        self.assert_group_idle(&a.group_id)?;
        if !a.deleted {
            return Err("请先将作品移入最近删除。".into());
        }
        // Delete the record first. Any files left by an interrupted deletion are recovered next launch.
        self.db
            .execute("DELETE FROM artworks WHERE id=?", [id])
            .map_err(|e| e.to_string())?;
        for relative in [&a.image_path, &a.thumbnail_path] {
            let path = self.path(relative)?;
            if path.exists() {
                fs::remove_file(path).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{self, Selections};

    fn list(
        store: &Store,
        search: &str,
        favorite: bool,
        deleted: bool,
        page: usize,
    ) -> Result<crate::groups::GroupPage> {
        store.list_groups(search, false, favorite, deleted, page, &[])
    }

    fn pending(prompt: &str) -> PendingArtwork {
        let snapshot = options::snapshot(
            &Selections {
                style: Some("FA-001".into()),
                ..Default::default()
            },
            None,
        )
        .unwrap();
        let submitted =
            crate::generation::instructions(&options::apply_snapshot(prompt, &snapshot));
        PendingArtwork::new(
            &format!(
                "data:image/png;base64,{}",
                STANDARD.encode(include_bytes!("../icons/icon.png"))
            ),
            prompt.into(),
            snapshot,
            submitted,
        )
        .unwrap()
    }

    #[test]
    fn persists_files_metadata_and_snapshot_across_restart_and_exports_original() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let pending = pending("窗边的小猫\n<script>literal</script>");
        let a = store.save(&pending).unwrap();
        assert_eq!(store.save(&pending).unwrap().id, a.id); // retries are idempotent
        assert_eq!(list(&store, "", false, false, 1).unwrap().total, 1);
        assert_eq!(
            fs::read(dir.path().join(&a.image_path)).unwrap(),
            pending.bytes
        );
        let thumb = image::open(dir.path().join(&a.thumbnail_path)).unwrap();
        assert!(thumb.width() <= 480 && thumb.height() <= 480);
        assert!(a.image_path.starts_with("images/"));
        assert!(!Path::new(&a.image_path).is_absolute());
        assert!(a.model.is_none() && a.tool_version.is_none());
        let json: String = store
            .db
            .query_row("SELECT metadata FROM artworks", [], |r| r.get(0))
            .unwrap();
        assert!(!json.contains("base64"));
        assert!(!json.contains("api_key"));
        drop(store);
        let reopened = Store::open(dir.path().into()).unwrap();
        let restored = reopened.get(&a.id).unwrap();
        assert_eq!(restored.prompt, pending.artwork.prompt);
        assert_eq!(restored.submitted_prompt, pending.artwork.submitted_prompt);
        assert_eq!(
            restored.snapshot.style.unwrap().prompt,
            pending.artwork.snapshot.style.as_ref().unwrap().prompt
        );
        assert_eq!(
            restored.snapshot.catalog_version,
            pending.artwork.snapshot.catalog_version
        );
        let export_dir = tempfile::tempdir().unwrap();
        let target = export_dir.path().join("cat.png");
        export_to(&target, &pending.bytes, &a, true).unwrap();
        assert_eq!(fs::read(&target).unwrap(), pending.bytes);
        let notes = fs::read_to_string(target.with_extension("txt")).unwrap();
        assert!(notes.contains(&a.prompt) && notes.contains(&a.submitted_prompt));
        // Existing notes are not overwritten without a separate confirmation.
        assert!(export_to(&target, &pending.bytes, &a, true).is_err());
        export_to(
            &export_dir.path().join("image-only.png"),
            &pending.bytes,
            &a,
            false,
        )
        .unwrap();
        assert!(!export_dir.path().join("image-only.txt").exists());
    }

    #[test]
    fn pagination_search_favorite_trash_restore_and_purge() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let mut ids = vec![];
        for i in 0..11 {
            let mut p = pending(&format!("小猫 {i}"));
            p.artwork.created_at = format!("2026-10-21T00:00:{i:02}+00:00");
            ids.push(store.save(&p).unwrap().id);
        }
        let first = list(&store, "", false, false, 1).unwrap();
        assert_eq!(first.total, 11);
        assert_eq!(first.items.len(), 9);
        assert_eq!(first.items[0].group.id, ids[10]);
        assert_eq!(list(&store, "", false, false, 2).unwrap().items.len(), 2);
        assert_eq!(list(&store, "", false, false, usize::MAX).unwrap().page, 2);
        let a = store
            .update(
                &ids[0],
                "100%_Cat'".into(),
                vec!["水彩".into()],
                true,
                false,
            )
            .unwrap();
        assert_eq!(list(&store, "100%_cat'", false, false, 1).unwrap().total, 1);
        assert_eq!(list(&store, "小猫 9", false, false, 1).unwrap().total, 1);
        assert_eq!(
            list(&store, "' OR 1=1 --", false, false, 1).unwrap().total,
            0
        );
        assert_eq!(list(&store, "", true, false, 1).unwrap().total, 1);
        assert!(store.purge(&a.id).is_err());
        store
            .update(&a.id, a.title.clone(), a.tags.clone(), true, true)
            .unwrap();
        assert_eq!(list(&store, "", false, false, 1).unwrap().total, 10);
        assert_eq!(list(&store, "", false, true, 1).unwrap().total, 1);
        store
            .update(&a.id, a.title.clone(), a.tags.clone(), true, false)
            .unwrap();
        assert_eq!(list(&store, "", false, false, 1).unwrap().total, 11);
        assert_eq!(store.get(&a.id).unwrap().tags, vec!["水彩"]);
        store.update(&a.id, a.title, a.tags, true, true).unwrap();
        store.purge(&a.id).unwrap();
        assert!(store.get(&a.id).is_err());
        assert!(!dir.path().join(a.image_path).exists());
        assert!(!dir.path().join(a.thumbnail_path).exists());
    }

    #[test]
    fn rolls_back_files_on_database_failure_and_allows_retry() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let p = pending("rollback");
        store.db.execute_batch("CREATE TRIGGER fail_save BEFORE INSERT ON artworks BEGIN SELECT RAISE(FAIL, 'disk full'); END;").unwrap();
        assert!(store.save(&p).err().unwrap().contains("disk full"));
        assert!(!dir.path().join(&p.artwork.image_path).exists());
        assert!(!dir.path().join(&p.artwork.thumbnail_path).exists());
        assert_eq!(list(&store, "", false, false, 1).unwrap().total, 0);
        assert_eq!(fs::read_dir(dir.path().join("staging")).unwrap().count(), 0);
        store.db.execute_batch("DROP TRIGGER fail_save").unwrap();
        assert_eq!(store.save(&p).unwrap().id, p.artwork.id);
    }

    #[test]
    fn file_failure_never_publishes_a_card() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let p = pending("file failure");
        fs::create_dir(dir.path().join(&p.artwork.thumbnail_path)).unwrap();
        assert!(store.save(&p).is_err());
        assert!(!dir.path().join(&p.artwork.image_path).exists());
        assert_eq!(list(&store, "", false, false, 1).unwrap().total, 0);
        fs::remove_dir(dir.path().join(&p.artwork.thumbnail_path)).unwrap();
        store.save(&p).unwrap();
    }

    #[test]
    fn startup_preserves_unknown_files_missing_metadata_and_trash() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let good = store.save(&pending("kept")).unwrap();
        store
            .update(&good.id, good.title.clone(), vec![], false, true)
            .unwrap();
        let missing = store.save(&pending("missing")).unwrap();
        fs::remove_file(dir.path().join(&missing.image_path)).unwrap();
        fs::write(dir.path().join("staging/unfinished"), b"partial").unwrap();
        fs::write(dir.path().join("images/orphan.png"), b"partial").unwrap();
        fs::write(dir.path().join("thumbnails/orphan.webp"), b"partial").unwrap();
        // A second process must not clean files while this instance is saving.
        assert!(Store::open(dir.path().into()).is_err());
        drop(store);
        let reopened = Store::open(dir.path().into()).unwrap();
        assert!(reopened.get(&missing.id).is_ok());
        assert!(dir.path().join(&missing.thumbnail_path).exists());
        let availability: String = reopened
            .db
            .query_row(
                "SELECT availability FROM artworks WHERE id=?",
                [&missing.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(availability, "missing");
        assert_eq!(list(&reopened, "", false, true, 1).unwrap().total, 1);
        assert!(reopened.image(&good.id).is_ok());
        for path in [
            "staging/unfinished",
            "images/orphan.png",
            "thumbnails/orphan.webp",
        ] {
            assert!(dir.path().join(path).exists());
        }
    }

    #[test]
    fn rejects_invalid_images_and_unsafe_relative_paths() {
        assert!(PendingArtwork::new(
            "data:image/png;base64,bm90IGFuIGltYWdl",
            "bad".into(),
            Snapshot::default(),
            "".into()
        )
        .is_err());
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        for path in ["../secret", "/absolute.png", "", "images/../../secret"] {
            assert!(store.path(path).is_err());
        }
    }
}

pub fn export_to(path: &Path, bytes: &[u8], artwork: &Artwork, include_notes: bool) -> Result<()> {
    let parent = path.parent().ok_or("请选择有效的导出位置")?;
    let notes = path.with_extension("txt");
    // Do not silently overwrite a separate prompt file the save dialog did not confirm.
    if include_notes && notes.exists() {
        return Err("同名提示词说明已存在，请选择新的文件名。".into());
    }
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    if include_notes {
        let text = format!("{}\n\n原始描述：\n{}\n\n生成条件（复用不保证生成完全相同的图片）：\n{}\n\n实际提交提示词：\n{}\n",
            artwork.title, artwork.prompt, serde_json::to_string_pretty(artwork).map_err(|e| e.to_string())?, artwork.submitted_prompt);
        let mut notes_file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        notes_file
            .write_all(text.as_bytes())
            .and_then(|_| notes_file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        notes_file
            .persist_noclobber(&notes)
            .map_err(|e| e.to_string())?;
    }
    if let Err(e) = file.persist(path) {
        if include_notes {
            let _ = fs::remove_file(notes);
        }
        return Err(e.to_string());
    }
    Ok(())
}
