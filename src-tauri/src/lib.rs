mod gallery;
mod generation;
mod options;
mod updates;

use std::{collections::HashMap, sync::Mutex};
use tauri::Manager;

#[derive(Default)]
struct GalleryState {
    store: Option<gallery::Store>,
    pending: HashMap<String, gallery::PendingArtwork>,
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
    fn store(&mut self, app: &tauri::AppHandle) -> Result<&gallery::Store, String> {
        if self.store.is_none() {
            let root = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
            self.store = Some(gallery::Store::open(root)?);
        }
        Ok(self.store.as_ref().unwrap())
    }

    fn save(&mut self, app: &tauri::AppHandle, id: &str) -> Result<gallery::Artwork, String> {
        self.store(app)?;
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

#[tauri::command]
async fn generate_image(
    app: tauri::AppHandle,
    prompt: String,
    selections: Option<options::Selections>,
    reuse_id: Option<String>,
) -> Result<Generated, String> {
    let work = WorkGuard::begin(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _work = work;
        let previous = reuse_id
            .map(|id| with_gallery(&app, |s| Ok(s.store(&app)?.get(&id)?.snapshot)))
            .transpose()?;
        let snapshot = options::snapshot(&selections.unwrap_or_default(), previous.as_ref())?;
        let output = generation::generate(&prompt, &snapshot)?;
        let image = output.image;
        let submitted =
            generation::instructions(&options::apply_snapshot(prompt.trim(), &snapshot));
        let mut pending = gallery::PendingArtwork::new(&image, prompt, snapshot, submitted)?;
        pending.artwork.tool_version = output.tool_version;
        let id = pending.artwork.id.clone();
        let save_error = with_gallery(&app, |s| {
            s.pending.insert(id.clone(), pending);
            s.save(&app, &id).map(|_| ())
        })
        .err();
        Ok(Generated {
            id,
            image,
            save_error,
        })
    })
    .await
    .map_err(|_| "图片生成任务意外结束，请重试。".to_string())?
}

#[tauri::command]
async fn gallery_list(
    app: tauri::AppHandle,
    search: String,
    favorite: bool,
    deleted: bool,
    page: usize,
) -> Result<gallery::Page, String> {
    tauri::async_runtime::spawn_blocking(move || {
        with_gallery(&app, |s| {
            s.store(&app)?.list(&search, favorite, deleted, page)
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
            let store = s.store(&app)?;
            Ok((store.get(&id)?, store.image(&id)?))
        })
    })
    .await
    .map_err(|e| e.to_string())?
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
