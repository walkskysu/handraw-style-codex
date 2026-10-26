use crate::{
    gallery::{Artwork, Store},
    options::Snapshot,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;

pub fn export_group_to(
    destination: &Path,
    group: &PhotoGroup,
    photos: &[(Artwork, Vec<u8>)],
    include_notes: bool,
) -> Result<PathBuf> {
    let output = destination.join(format!("handraw-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&output).map_err(|e| e.to_string())?;
    for (a, bytes) in photos {
        crate::gallery::export_to(
            &output.join(format!("{:02}-{}.png", a.position + 1, a.id)),
            bytes,
            a,
            include_notes,
        )?;
    }
    if include_notes {
        fs::write(
            output.join("group.json"),
            serde_json::to_vec_pretty(group).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(output)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PhotoGroup {
    pub id: String,
    pub title: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub cover_id: Option<String>,
    pub prompt: String,
    pub snapshot: Snapshot,
    pub created_at: String,
    pub status: String,
    pub expected_count: usize,
    pub generated_count: usize,
    pub generation_errors: Vec<String>,
    pub source_group_id: Option<String>,
    pub source_photo_id: Option<String>,
    pub deleted: bool,
    // Only this deletion's members may be restored by a group restore.
    pub deleted_photo_ids: Vec<String>,
}

impl PhotoGroup {
    pub fn new(prompt: String, snapshot: Snapshot, count: usize, source: Option<&Artwork>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            title: prompt.chars().take(40).collect(),
            tags: vec![],
            favorite: false,
            cover_id: None,
            prompt,
            snapshot,
            created_at: chrono::Utc::now().to_rfc3339(),
            status: "generating".into(),
            expected_count: count,
            generated_count: 0,
            generation_errors: vec![],
            source_group_id: source.map(|a| a.group_id.clone()),
            source_photo_id: source.map(|a| a.id.clone()),
            deleted: false,
            deleted_photo_ids: vec![],
        }
    }
}

#[derive(Serialize)]
pub struct GroupCard {
    #[serde(flatten)]
    pub group: PhotoGroup,
    pub thumbnail: String,
    pub thumbnails: Vec<String>,
    pub photo_count: usize,
    pub pending_count: usize,
    pub matching_ids: Vec<String>,
    pub matching_count: usize,
}

#[derive(Serialize)]
pub struct GroupPage {
    pub items: Vec<GroupCard>,
    pub total: usize,
    pub page: usize,
    pub photo_total: usize,
}

#[derive(Serialize)]
pub struct GroupDetail {
    pub group: PhotoGroup,
    pub photos: Vec<Artwork>,
}

fn matches(search: &str, title: &str, prompt: &str, tags: &[String]) -> bool {
    title.to_lowercase().contains(search)
        || prompt.to_lowercase().contains(search)
        || tags.iter().any(|t| t.to_lowercase().contains(search))
}

impl Store {
    pub fn repair_cover(&self, id: &str) -> Result<()> {
        let detail = self.group_detail(id, false)?;
        self.put_group(&detail.group)
    }
    pub fn put_group(&self, group: &PhotoGroup) -> Result<()> {
        self.db.execute("INSERT INTO photo_groups(id, metadata) VALUES(?,?) ON CONFLICT(id) DO UPDATE SET metadata=excluded.metadata",
            params![group.id, serde_json::to_string(group).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn group(&self, id: &str) -> Result<PhotoGroup> {
        let json: String = self
            .db
            .query_row("SELECT metadata FROM photo_groups WHERE id=?", [id], |r| {
                r.get(0)
            })
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&json).map_err(|e| e.to_string())
    }

    pub fn ensure_group(&self, a: &Artwork) -> Result<()> {
        let id = if a.group_id.is_empty() {
            &a.id
        } else {
            &a.group_id
        };
        let exists: bool = self
            .db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM photo_groups WHERE id=?)",
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            let mut group = PhotoGroup::new(a.prompt.clone(), a.snapshot.clone(), 1, None);
            group.id = id.clone();
            group.title = a.title.clone();
            group.tags = a.tags.clone();
            group.favorite = a.favorite;
            group.cover_id = Some(a.id.clone());
            group.created_at = a.created_at.clone();
            group.generated_count = 1;
            group.status = "complete".into();
            self.put_group(&group)?;
        }
        Ok(())
    }

    pub fn migrate_groups(&self) -> Result<()> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        self.db.execute_batch("CREATE TABLE IF NOT EXISTS photo_groups(id TEXT PRIMARY KEY, metadata TEXT NOT NULL);").map_err(|e| e.to_string())?;
        for mut a in self.all()? {
            if a.group_id.is_empty() {
                a.group_id = a.id.clone();
                self.db
                    .execute(
                        "UPDATE artworks SET metadata=? WHERE id=?",
                        params![serde_json::to_string(&a).map_err(|e| e.to_string())?, a.id],
                    )
                    .map_err(|e| e.to_string())?;
            }
            self.ensure_group(&a)?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn group_detail(&self, id: &str, deleted: bool) -> Result<GroupDetail> {
        let mut group = self.group(id)?;
        let mut photos: Vec<_> = self
            .all()?
            .into_iter()
            .filter(|a| a.group_id == id && a.deleted == deleted)
            .collect();
        photos.sort_by(|a, b| a.position.cmp(&b.position).then(a.id.cmp(&b.id)));
        if !photos
            .iter()
            .any(|a| Some(&a.id) == group.cover_id.as_ref())
        {
            group.cover_id = photos.first().map(|a| a.id.clone());
        }
        Ok(GroupDetail { group, photos })
    }

    pub fn list_groups(
        &self,
        search: &str,
        favorite: bool,
        photo_favorite: bool,
        deleted: bool,
        page: usize,
        pending: &[Artwork],
    ) -> Result<GroupPage> {
        let mut query = self
            .db
            .prepare("SELECT metadata FROM photo_groups")
            .map_err(|e| e.to_string())?;
        let groups = query
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let search = search.trim().to_lowercase();
        let all = self.all()?;
        let mut by_group: HashMap<&str, Vec<&Artwork>> = HashMap::new();
        for photo in all.iter().filter(|a| a.deleted == deleted) {
            by_group.entry(&photo.group_id).or_default().push(photo);
        }
        let mut candidates = vec![];
        for json in groups {
            let mut group: PhotoGroup = serde_json::from_str(&json.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let mut photos = by_group.remove(group.id.as_str()).unwrap_or_default();
            photos.sort_by_key(|a| (a.position, &a.id));
            let unsaved: Vec<_> = pending
                .iter()
                .filter(|a| a.group_id == group.id && !deleted)
                .collect();
            if photos.is_empty() && unsaved.is_empty() {
                continue;
            }
            if favorite && !group.favorite {
                continue;
            }
            if photo_favorite && !photos.iter().any(|a| a.favorite) {
                continue;
            }
            let common = matches(&search, &group.title, &group.prompt, &group.tags);
            let matching_ids: Vec<_> = photos
                .iter()
                .chain(unsaved.iter())
                .filter(|a| common || matches(&search, &a.title, &a.prompt, &a.tags))
                .map(|a| a.id.clone())
                .collect();
            if matching_ids.is_empty() {
                continue;
            }
            if !photos
                .iter()
                .any(|a| Some(&a.id) == group.cover_id.as_ref())
            {
                group.cover_id = photos.first().map(|a| a.id.clone());
            }
            candidates.push((group, photos, unsaved.len(), matching_ids));
        }
        candidates.sort_by(|a, b| {
            b.0.created_at
                .cmp(&a.0.created_at)
                .then(b.0.id.cmp(&a.0.id))
        });
        let total = candidates.len();
        let photo_total = candidates
            .iter()
            .map(|(_, photos, _, _)| photos.len())
            .sum();
        let page = page.max(1).min(total.saturating_sub(1) / 9 + 1);
        let mut items = vec![];
        for (group, photos, pending_count, matching_ids) in
            candidates.into_iter().skip((page - 1) * 9).take(9)
        {
            let thumbnail = |a: &Artwork| -> Result<String> {
                // Artwork paths originate from the store, never IPC input.
                Ok(format!(
                    "data:image/webp;base64,{}",
                    STANDARD.encode(
                        fs::read(self.path(&a.thumbnail_path)?).map_err(|e| e.to_string())?
                    )
                ))
            };
            let cover = photos
                .iter()
                .find(|a| Some(&a.id) == group.cover_id.as_ref());
            items.push(GroupCard {
                matching_count: photos
                    .iter()
                    .filter(|a| matching_ids.contains(&a.id))
                    .count(),
                thumbnail: cover.map(|a| thumbnail(a)).transpose()?.unwrap_or_default(),
                thumbnails: photos
                    .iter()
                    .filter(|a| Some(&a.id) != group.cover_id.as_ref())
                    .take(3)
                    .map(|a| thumbnail(a))
                    .collect::<Result<_>>()?,
                photo_count: photos.len(),
                pending_count,
                group,
                matching_ids,
            });
        }
        Ok(GroupPage {
            items,
            total,
            page,
            photo_total,
        })
    }

    pub fn update_group(
        &self,
        id: &str,
        title: String,
        tags: Vec<String>,
        favorite: bool,
        deleted: bool,
    ) -> Result<PhotoGroup> {
        if title.trim().is_empty()
            || title.chars().count() > 200
            || tags.len() > 30
            || tags.iter().any(|t| t.chars().count() > 80)
        {
            return Err("组标题需为 1–200 字；最多 30 个标签，每个不超过 80 字。".into());
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut group = self.group(id)?;
        if deleted != group.deleted || (deleted && !self.group_detail(id, false)?.photos.is_empty())
        {
            if deleted {
                group.deleted_photo_ids = self
                    .group_detail(id, false)?
                    .photos
                    .iter()
                    .map(|a| a.id.clone())
                    .collect();
            }
            for photo_id in &group.deleted_photo_ids {
                if let Ok(a) = self.get(photo_id) {
                    self.update(&a.id, a.title, a.tags, a.favorite, deleted)?;
                }
            }
            if !deleted {
                group.deleted_photo_ids.clear();
            }
        }
        group.title = title.trim().into();
        group.tags = tags;
        group.favorite = favorite;
        group.deleted = deleted;
        self.put_group(&group)?;
        self.repair_cover(id)?;
        tx.commit().map_err(|e| e.to_string())?;
        self.group(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery::PendingArtwork;

    #[test]
    fn group_export_preserves_originals_notes_and_provenance_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let (mut group, photos) = create(&store, 2);
        group.source_group_id = Some("source-group".into());
        group.source_photo_id = Some("source-photo".into());
        let photos: Vec<_> = photos
            .into_iter()
            .map(|a| {
                let bytes = store.bytes(&a).unwrap();
                (a, bytes)
            })
            .collect();
        let output = export_group_to(dir.path(), &group, &photos, true).unwrap();
        for (a, bytes) in &photos {
            let path = output.join(format!("{:02}-{}.png", a.position + 1, a.id));
            assert_eq!(fs::read(&path).unwrap(), *bytes);
            assert!(fs::read_to_string(path.with_extension("txt"))
                .unwrap()
                .contains(&a.submitted_prompt));
        }
        let manifest: PhotoGroup =
            serde_json::from_slice(&fs::read(output.join("group.json")).unwrap()).unwrap();
        assert_eq!(manifest.source_photo_id, group.source_photo_id);
        let other = export_group_to(dir.path(), &group, &photos, false).unwrap();
        assert_ne!(output, other);
        assert_eq!(fs::read_dir(other).unwrap().count(), 2);
    }

    fn photo(group: &PhotoGroup, position: usize) -> PendingArtwork {
        let mut p = PendingArtwork::new(
            &format!(
                "data:image/png;base64,{}",
                STANDARD.encode(include_bytes!("../icons/icon.png"))
            ),
            group.prompt.clone(),
            group.snapshot.clone(),
            "submitted specification".into(),
        )
        .unwrap();
        p.artwork.group_id = group.id.clone();
        p.artwork.position = position;
        p
    }

    fn create(store: &Store, count: usize) -> (PhotoGroup, Vec<Artwork>) {
        let mut g = PhotoGroup::new("same prompt".into(), Snapshot::default(), count, None);
        g.status = "complete".into();
        g.generated_count = count;
        store.put_group(&g).unwrap();
        let photos = (0..count)
            .map(|i| store.save(&photo(&g, i)).unwrap())
            .collect();
        (g, photos)
    }

    fn list(
        store: &Store,
        search: &str,
        favorite: bool,
        photo_favorite: bool,
        deleted: bool,
    ) -> GroupPage {
        store
            .list_groups(search, favorite, photo_favorite, deleted, 1, &[])
            .unwrap()
    }

    #[test]
    fn migrates_each_legacy_photo_once_without_merging_or_changing_history() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let (_, photos) = create(&store, 2);
        for (i, a) in photos.iter().enumerate() {
            let a = store
                .update(
                    &a.id,
                    format!("old title {i}"),
                    vec!["tag".into()],
                    true,
                    i == 1,
                )
                .unwrap();
            let mut legacy = serde_json::to_value(&a).unwrap();
            for key in ["group_id", "position", "save_status"] {
                legacy.as_object_mut().unwrap().remove(key);
            }
            store
                .db
                .execute(
                    "UPDATE artworks SET metadata=? WHERE id=?",
                    params![legacy.to_string(), a.id],
                )
                .unwrap();
        }
        store.db.execute_batch("DROP TABLE photo_groups").unwrap();
        drop(store);
        for _ in 0..2 {
            let store = Store::open(dir.path().into()).unwrap();
            for (i, a) in photos.iter().enumerate() {
                let restored = store.get(&a.id).unwrap();
                assert_eq!(restored.group_id, a.id);
                assert_eq!(restored.submitted_prompt, a.submitted_prompt);
                assert_eq!(restored.image_path, a.image_path);
                assert_eq!(restored.title, format!("old title {i}"));
                assert_eq!(restored.tags, vec!["tag"]);
                assert!(restored.favorite && store.group(&a.id).unwrap().favorite);
                assert_eq!(restored.deleted, i == 1);
            }
            assert_eq!(list(&store, "", false, false, false).total, 1);
            assert_eq!(list(&store, "", false, false, true).total, 1);
        }
    }

    #[test]
    fn same_prompt_creates_distinct_groups_and_reuse_records_both_sources() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let (first, photos) = create(&store, 1);
        let (second, _) = create(&store, 2);
        assert_ne!(first.id, second.id);
        let reused = PhotoGroup::new(
            first.prompt.clone(),
            first.snapshot.clone(),
            1,
            Some(&photos[0]),
        );
        assert_ne!(first.id, reused.id);
        assert_eq!(reused.source_group_id.as_deref(), Some(first.id.as_str()));
        assert_eq!(
            reused.source_photo_id.as_deref(),
            Some(photos[0].id.as_str())
        );
        let page = list(&store, "", false, false, false);
        assert_eq!((page.total, page.photo_total), (2, 3));
    }

    #[test]
    fn group_and_photo_favorites_are_independent_and_search_matches_tags() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let (g, photos) = create(&store, 3);
        let a = &photos[1];
        store
            .update(
                &a.id,
                "unique cat".into(),
                vec!["watercolor".into()],
                true,
                false,
            )
            .unwrap();
        assert_eq!(list(&store, "", true, false, false).total, 0);
        assert_eq!(list(&store, "", false, true, false).total, 1);
        let matches = list(&store, "WATERCOLOR", false, false, false);
        assert_eq!(matches.items[0].matching_ids, vec![a.id.clone()]);
        assert_eq!(matches.items[0].photo_count, 3);
        assert_eq!(list(&store, "unique cat", false, false, false).total, 1);
        store
            .update_group(
                &g.id,
                "100%_CAT'".into(),
                vec!["group-tag".into()],
                true,
                false,
            )
            .unwrap();
        assert_eq!(
            list(&store, "100%_cat'", true, false, false).items[0]
                .matching_ids
                .len(),
            3
        );
        assert_eq!(
            list(&store, "group-tag", false, false, false).items[0]
                .matching_ids
                .len(),
            3
        );
        assert_eq!(list(&store, "same prompt", false, false, false).total, 1);
        assert_eq!(list(&store, "' OR 1=1 --", false, false, false).total, 0);
        store
            .update(&a.id, a.title.clone(), vec![], false, false)
            .unwrap();
        assert_eq!(list(&store, "", false, true, false).total, 0);
        assert_eq!(list(&store, "", true, false, false).total, 1);
    }

    #[test]
    fn deleting_cover_advances_it_and_group_restore_excludes_prior_photo_deletions() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let (g, photos) = create(&store, 3);
        store.repair_cover(&g.id).unwrap();
        let old = &photos[0];
        store
            .update(&old.id, old.title.clone(), old.tags.clone(), false, true)
            .unwrap();
        assert_eq!(
            store.group(&g.id).unwrap().cover_id,
            Some(photos[1].id.clone())
        );
        store
            .update_group(&g.id, g.title.clone(), vec![], false, true)
            .unwrap();
        assert_eq!(list(&store, "", false, false, false).total, 0);
        // Editing a trashed group must not replace the deletion's member snapshot.
        store
            .update_group(&g.id, "renamed in trash".into(), vec![], true, true)
            .unwrap();
        drop(store);
        let store = Store::open(dir.path().into()).unwrap();
        store
            .update_group(&g.id, g.title, vec![], false, false)
            .unwrap();
        assert!(store.get(&old.id).unwrap().deleted);
        assert_eq!(
            list(&store, "", false, false, false).items[0].photo_count,
            2
        );
        assert_eq!(list(&store, "", false, false, true).items[0].photo_count, 1);
        for a in &photos[1..] {
            store
                .update(&a.id, a.title.clone(), vec![], false, true)
                .unwrap();
        }
        assert_eq!(list(&store, "", false, false, false).total, 0);
    }

    #[test]
    fn partial_generation_and_save_retry_count_only_saved_photos_and_keep_group_and_position() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let (mut g, _) = create(&store, 2);
        g.expected_count = 4;
        g.generated_count = 3;
        g.status = "partial".into();
        g.generation_errors = vec!["one generation failed".into()];
        store.put_group(&g).unwrap();
        let p = photo(&g, 3);
        store.db.execute_batch("CREATE TRIGGER fail_save BEFORE INSERT ON artworks BEGIN SELECT RAISE(FAIL, 'disk full'); END;").unwrap();
        assert!(store.save(&p).is_err());
        let page = store
            .list_groups("", false, false, false, 1, &[p.artwork.clone()])
            .unwrap();
        assert_eq!(page.photo_total, 2);
        assert_eq!(page.items[0].matching_count, 2);
        assert_eq!(
            (page.items[0].photo_count, page.items[0].pending_count),
            (2, 1)
        );
        assert_eq!(page.items[0].group.generated_count, 3);
        store.db.execute_batch("DROP TRIGGER fail_save").unwrap();
        for _ in 0..2 {
            let saved = store.save(&p).unwrap();
            assert_eq!(saved.group_id, g.id);
            assert_eq!(saved.position, 3);
            assert_eq!(saved.id, p.artwork.id);
        }
        assert_eq!(list(&store, "", false, false, false).photo_total, 3);
        assert_eq!(store.group(&g.id).unwrap().generation_errors.len(), 1);
    }

    #[test]
    fn group_deletion_rolls_back_all_members_when_a_database_update_fails() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        let (g, photos) = create(&store, 3);
        store.db.execute_batch(&format!("CREATE TRIGGER fail_update BEFORE UPDATE ON artworks WHEN NEW.id='{}' BEGIN SELECT RAISE(FAIL, 'disk full'); END;", photos[1].id)).unwrap();
        assert!(store
            .update_group(&g.id, g.title, vec![], false, true)
            .is_err());
        assert!(photos.iter().all(|a| !store.get(&a.id).unwrap().deleted));
        assert!(!store.group(&g.id).unwrap().deleted);
    }

    #[test]
    fn pagination_is_by_group_and_thumbnails_are_limited_to_three() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().into()).unwrap();
        for _ in 0..10 {
            create(&store, 5);
        }
        let page = list(&store, "", false, false, false);
        assert_eq!(
            (page.total, page.photo_total, page.items.len()),
            (10, 50, 9)
        );
        assert!(page.items.iter().all(|c| c.thumbnails.len() == 3));
        let last = store
            .list_groups("", false, false, false, usize::MAX, &[])
            .unwrap();
        assert_eq!((last.page, last.items.len()), (2, 1));
    }
}
