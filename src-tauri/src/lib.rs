mod gallery;
mod generation;
mod groups;
mod options;
mod updates;

use std::{collections::HashMap, sync::Mutex};
use tauri::Manager;

#[derive(Default)]
struct GalleryState {
    store: Option<gallery::Store>,
    pending: HashMap<String, gallery::PendingArtwork>,
    pending_groups: HashMap<String, groups::PhotoGroup>,
    work: updates::WorkState,
}

// Owned by the blocking job itself, so cancellation of the IPC future cannot release it early.
struct WorkGuard(tauri::AppHandle);

impl WorkGuard {
    fn begin(app: &tauri::AppHandle) -> Result<Self, String> {
        with_gallery(app, |s| s.work.begin())?;
        Ok(Self(app.clone()))
    }
}

impl Drop for WorkGuard {
    fn drop(&mut self) {
        let _ = with_gallery(&self.0, |s| {
            s.work.finish();
            Ok(())
        });
    }
}

impl GalleryState {
    fn persist_group(&mut self, group: &groups::PhotoGroup) -> Result<(), String> {
        // Keep group metadata in memory on disk/SQLite failure, just like photo bytes.
        self.pending_groups.insert(group.id.clone(), group.clone());
        self.store.as_ref().ok_or("图库未打开")?.put_group(group)?;
        self.pending_groups.remove(&group.id);
        Ok(())
    }

    fn retry_group(&mut self, id: &str) -> Result<(), String> {
        if let Some(group) = self.pending_groups.get(id).cloned() {
            self.persist_group(&group)?;
        }
        Ok(())
    }

    fn store(&mut self, app: &tauri::AppHandle) -> Result<&gallery::Store, String> {
        if self.store.is_none() {
            let root = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
            self.store = Some(gallery::Store::open(root)?);
        }
        Ok(self.store.as_ref().unwrap())
    }

    fn save(&mut self, app: &tauri::AppHandle, id: &str) -> Result<gallery::Artwork, String> {
        self.store(app)?;
        let group_id = if let Some(p) = self.pending.get(id) {
            p.artwork.group_id.clone()
        } else {
            self.store.as_ref().unwrap().get(id)?.group_id
        };
        self.retry_group(&group_id)?;
        let store = self.store.as_ref().unwrap();
        let result = if let Some(pending) = self.pending.get(id) {
            store.save(pending)
        } else {
            store.get(id)
        };
        if result.is_ok() {
            self.pending.remove(id);
        }
        result
    }
}

fn with_gallery<T>(
    app: &tauri::AppHandle,
    action: impl FnOnce(&mut GalleryState) -> Result<T, String>,
) -> Result<T, String> {
    let state = app.state::<Mutex<GalleryState>>();
    let mut state = state.lock().map_err(|_| "图库任务意外结束，请重启应用。")?;
    action(&mut state)
}

#[derive(serde::Serialize)]
struct Generated {
    id: String,
    image: String,
    save_error: Option<String>,
}

#[derive(serde::Serialize)]
struct GeneratedGroup {
    group_id: String,
    photos: Vec<Generated>,
    expected_count: usize,
    generation_errors: Vec<String>,
    group_save_error: Option<String>,
}

#[tauri::command]
async fn generate_image(
    app: tauri::AppHandle,
    prompt: String,
    selections: Option<options::Selections>,
    reuse_id: Option<String>,
    count: Option<usize>,
) -> Result<GeneratedGroup, String> {
    generation::validate_prompt(&prompt)?;
    let count = count.unwrap_or(1);
    if !(1..=4).contains(&count) {
        return Err("每组可生成 1–4 张照片。".into());
    }
    let work = WorkGuard::begin(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _work = work;
        let generation = generation::GenerationGuard::acquire()?;
        let previous = reuse_id
            .map(|id| with_gallery(&app, |s| s.store(&app)?.get(&id)))
            .transpose()?;
        let snapshot = options::snapshot(
            &selections.unwrap_or_default(),
            previous.as_ref().map(|a| &a.snapshot),
        )?;
        let mut group = groups::PhotoGroup::new(
            prompt.trim().into(),
            snapshot.clone(),
            count,
            previous.as_ref(),
        );
        with_gallery(&app, |s| s.store(&app)?.put_group(&group))?;
        let submitted =
            generation::instructions(&options::apply_snapshot(prompt.trim(), &snapshot));
        let mut photos = vec![];
        let mut group_save_error = None;
        for position in 0..count {
            let result = generation.generate(&prompt, &snapshot).and_then(|output| {
                let mut pending = gallery::PendingArtwork::new(
                    &output.image,
                    prompt.clone(),
                    snapshot.clone(),
                    submitted.clone(),
                )?;
                pending.artwork.tool_version = output.tool_version;
                pending.artwork.group_id = group.id.clone();
                pending.artwork.position = position;
                Ok((pending, output.image))
            });
            match result {
                Ok((pending, image)) => {
                    let id = pending.artwork.id.clone();
                    let save_error = with_gallery(&app, |s| {
                        s.pending.insert(id.clone(), pending);
                        s.save(&app, &id).map(|_| ())
                    })
                    .err();
                    group.generated_count += 1;
                    if group.cover_id.is_none() && save_error.is_none() {
                        group.cover_id = Some(id.clone());
                    }
                    photos.push(Generated {
                        id,
                        image,
                        save_error,
                    });
                }
                Err(error) => group
                    .generation_errors
                    .push(format!("第 {} 张：{error}", position + 1)),
            }
            group.status = if position + 1 < count {
                "generating"
            } else if group.generated_count == 0 {
                "failed"
            } else if group.generated_count == count {
                "complete"
            } else {
                "partial"
            }
            .into();
            group_save_error = with_gallery(&app, |s| {
                // Preserve edits made while this batch was running.
                let current = match s.pending_groups.get(&group.id) {
                    Some(current) => current.clone(),
                    None => s.store(&app)?.group(&group.id)?,
                };
                group.title = current.title;
                group.tags = current.tags;
                group.favorite = current.favorite;
                s.persist_group(&group)
            })
            .err();
        }
        if photos.is_empty() {
            with_gallery(&app, |s| {
                s.pending_groups.remove(&group.id);
                Ok(())
            })?;
            return Err(group.generation_errors.join("\n"));
        }
        Ok(GeneratedGroup {
            group_id: group.id,
            photos,
            expected_count: count,
            generation_errors: group.generation_errors,
            group_save_error,
        })
    })
    .await
    .map_err(|_| "图片生成任务意外结束，请重试。".to_string())?
}

#[tauri::command]
async fn gallery_group_retry(app: tauri::AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || with_gallery(&app, |s| s.retry_group(&id)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gallery_list(
    app: tauri::AppHandle,
    search: String,
    favorite: bool,
    photo_favorite: Option<bool>,
    deleted: bool,
    page: usize,
) -> Result<groups::GroupPage, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_gallery(&app, |s| {
            let pending: Vec<_> = s.pending.values().map(|p| p.artwork.clone()).collect();
            let mut result = s.store(&app)?.list_groups(
                &search,
                favorite,
                photo_favorite.unwrap_or(false),
                deleted,
                page,
                &pending,
            )?;
            for card in &mut result.items {
                if let Some(group) = s.pending_groups.get(&card.group.id) {
                    let cover_id = card.group.cover_id.clone();
                    card.group = group.clone();
                    card.group.cover_id = cover_id;
                }
            }
            Ok(result)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gallery_detail(
    app: tauri::AppHandle,
    id: String,
) -> Result<(gallery::Artwork, String), String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_gallery(&app, |s| {
            if let Some(pending) = s.pending.get(&id) {
                use base64::Engine;
                let mut artwork = pending.artwork.clone();
                artwork.save_status = "pending".into();
                return Ok((
                    artwork,
                    format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&pending.bytes)
                    ),
                ));
            }
            let store = s.store(&app)?;
            Ok((store.get(&id)?, store.image(&id)?))
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gallery_group_detail(
    app: tauri::AppHandle,
    id: String,
    deleted: bool,
) -> Result<groups::GroupDetail, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_gallery(&app, |s| {
            let mut detail = s.store(&app)?.group_detail(&id, deleted)?;
            if let Some(group) = s.pending_groups.get(&id) {
                let cover_id = detail.group.cover_id;
                detail.group = group.clone();
                detail.group.cover_id = cover_id;
            }
            if !deleted {
                for p in s.pending.values().filter(|p| p.artwork.group_id == id) {
                    let mut a = p.artwork.clone();
                    a.save_status = "pending".into();
                    detail.photos.push(a);
                }
            }
            detail.photos.sort_by_key(|a| a.position);
            Ok(detail)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gallery_group_update(
    app: tauri::AppHandle,
    id: String,
    title: String,
    tags: Vec<String>,
    favorite: bool,
    deleted: bool,
) -> Result<groups::PhotoGroup, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_gallery(&app, |s| {
            s.retry_group(&id)?;
            if deleted && s.pending.values().any(|p| p.artwork.group_id == id) {
                return Err("请先保存或导出组内待保存照片。".into());
            }
            s.store(&app)?
                .update_group(&id, title, tags, favorite, deleted)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gallery_group_export(
    app: tauri::AppHandle,
    id: String,
    include_notes: bool,
    deleted: bool,
) -> Result<bool, String> {
    let work = WorkGuard::begin(&app)?;
    let (group, photos) = with_gallery(&app, |s| {
        let store = s.store(&app)?;
        let detail = store.group_detail(&id, deleted)?;
        let mut photos = detail
            .photos
            .into_iter()
            .map(|a| Ok((a.clone(), store.bytes(&a)?)))
            .collect::<Result<Vec<_>, String>>()?;
        if !deleted {
            photos.extend(
                s.pending
                    .values()
                    .filter(|p| p.artwork.group_id == id)
                    .map(|p| (p.artwork.clone(), p.bytes.clone())),
            );
        }
        let group = s.pending_groups.get(&id).cloned().unwrap_or(detail.group);
        Ok((group, photos))
    })?;
    let Some(folder) = rfd::AsyncFileDialog::new()
        .set_title("导出整组到文件夹")
        .pick_folder()
        .await
    else {
        return Ok(false);
    };
    let destination = folder.path().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let _work = work;
        groups::export_group_to(&destination, &group, &photos, include_notes)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(true)
}

#[tauri::command]
async fn gallery_retry(app: tauri::AppHandle, id: String) -> Result<gallery::Artwork, String> {
    tauri::async_runtime::spawn_blocking(move || with_gallery(&app, |s| s.save(&app, &id)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn gallery_discard_pending(app: tauri::AppHandle, id: String) -> Result<(), String> {
    with_gallery(&app, |s| {
        s.pending.remove(&id);
        Ok(())
    })
}

#[tauri::command]
async fn gallery_update(
    app: tauri::AppHandle,
    id: String,
    title: String,
    tags: Vec<String>,
    favorite: bool,
    deleted: bool,
) -> Result<gallery::Artwork, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_gallery(&app, |s| {
            s.store(&app)?.update(&id, title, tags, favorite, deleted)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gallery_purge(app: tauri::AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || with_gallery(&app, |s| s.store(&app)?.purge(&id)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gallery_export(
    app: tauri::AppHandle,
    id: String,
    include_notes: bool,
) -> Result<bool, String> {
    let work = WorkGuard::begin(&app)?;
    let (artwork, bytes) = with_gallery(&app, |s| {
        if let Some(pending) = s.pending.get(&id) {
            return Ok((pending.artwork.clone(), pending.bytes.clone()));
        }
        let store = s.store(&app)?;
        let artwork = store.get(&id)?;
        let bytes = store.bytes(&artwork)?;
        Ok((artwork, bytes))
    })?;
    let mut dialog = rfd::AsyncFileDialog::new()
        .set_title("导出图片")
        .add_filter("PNG 图片", &["png"])
        .set_file_name(format!("handraw-{}.png", artwork.id));
    if let Ok(pictures) = app.path().picture_dir() {
        dialog = dialog.set_directory(pictures);
    }
    let Some(file) = dialog.save_file().await else {
        return Ok(false);
    };
    let path = file.path().to_path_buf();
    if !path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
    {
        return Err("请使用 .png 扩展名导出图片。".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _work = work;
        gallery::export_to(&path, &bytes, &artwork, include_notes)
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(true)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Mutex::new(GalleryState::default()))
        .manage(tauri::async_runtime::Mutex::new(
            updates::UpdateState::default(),
        ))
        .invoke_handler(tauri::generate_handler![
            generate_image,
            gallery_list,
            gallery_group_detail,
            gallery_group_retry,
            gallery_group_update,
            gallery_group_export,
            gallery_detail,
            gallery_retry,
            gallery_discard_pending,
            gallery_update,
            gallery_purge,
            gallery_export,
            updates::update_about,
            updates::update_check,
            updates::update_download,
            updates::update_install,
            updates::update_manual
        ])
        .run(tauri::generate_context!())
        .expect("启动 Handraw 时发生错误");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_group_metadata_write_is_retained_and_can_be_retried_without_generation() {
        let dir = tempfile::tempdir().unwrap();
        let store = gallery::Store::open(dir.path().into()).unwrap();
        let mut group =
            groups::PhotoGroup::new("cats".into(), options::Snapshot::default(), 4, None);
        store.put_group(&group).unwrap();
        store.db.execute_batch("CREATE TRIGGER fail_group BEFORE UPDATE ON photo_groups BEGIN SELECT RAISE(FAIL, 'disk full'); END;").unwrap();
        let mut state = GalleryState {
            store: Some(store),
            ..Default::default()
        };
        group.generated_count = 3;
        group.generation_errors.push("one failed".into());
        group.status = "partial".into();
        assert!(state.persist_group(&group).is_err());
        assert_eq!(state.pending_groups[&group.id].generated_count, 3);
        state
            .store
            .as_ref()
            .unwrap()
            .db
            .execute_batch("DROP TRIGGER fail_group")
            .unwrap();
        state.retry_group(&group.id).unwrap();
        assert!(state.pending_groups.is_empty());
        let saved = state.store.as_ref().unwrap().group(&group.id).unwrap();
        assert_eq!(saved.generated_count, 3);
        assert_eq!(saved.generation_errors, vec!["one failed"]);
        state.retry_group(&group.id).unwrap();
    }
}
