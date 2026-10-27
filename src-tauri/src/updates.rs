use std::{path::Path, time::Duration};
use tauri::{ipc::Channel, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

pub const RELEASE_PAGE: &str = "https://github.com/walkskysu/handraw-style-codex/releases/latest";

#[derive(Default)]
pub struct UpdateState {
    update: Option<Update>,
    // Only bytes returned by the updater's signature-verifying download enter this slot.
    verified: Option<Vec<u8>>,
}

#[derive(Default)]
pub struct WorkState {
    active: usize,
    installing: bool,
}

impl WorkState {
    pub fn begin(&mut self) -> Result<(), String> {
        if self.installing {
            return Err("正在安装更新，请稍后再试。".into());
        }
        self.active += 1;
        Ok(())
    }

    pub fn finish(&mut self) {
        self.active -= 1;
    }

    pub fn install(&mut self, unsaved: bool) -> Result<(), String> {
        if self.active > 0 || unsaved {
            return Err("请等待生成和保存任务结束，并先保存所有作品。".into());
        }
        if self.installing {
            return Err("更新正在安装。".into());
        }
        self.installing = true;
        Ok(())
    }

    pub fn installation_failed(&mut self) {
        self.installing = false;
    }
}

#[derive(serde::Serialize)]
pub struct About {
    current_version: String,
    install_error: Option<String>,
}

fn incomplete_install(root: &Path, current: &semver::Version) -> Result<Option<String>, String> {
    let path = root.join("update-attempt.json");
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let target: String = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let target_version = semver::Version::parse(&target).map_err(|e| e.to_string())?;
    if current >= &target_version {
        std::fs::remove_file(path).map_err(|e| e.to_string())?;
        Ok(None)
    } else {
        Ok(Some(format!(
            "上次更新到 {target} 未完成，请重新下载或手动安装。"
        )))
    }
}

#[tauri::command]
pub fn update_about(app: tauri::AppHandle) -> Result<About, String> {
    let root = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    Ok(About {
        current_version: app.package_info().version.to_string(),
        install_error: incomplete_install(&root, &app.package_info().version)
            .unwrap_or_else(|e| Some(format!("无法读取上次更新状态：{e}。请重新下载或手动安装。"))),
    })
}

#[derive(serde::Serialize)]
pub struct Release {
    version: String,
    notes: String,
}

#[tauri::command]
pub async fn update_check(app: tauri::AppHandle) -> Result<Option<Release>, String> {
    let state = app.state::<tauri::async_runtime::Mutex<UpdateState>>();
    let mut state = state.try_lock().map_err(|_| "更新任务正在进行。")?;
    // A fresh check invalidates the previous candidate and its downloaded bytes.
    state.update = None;
    state.verified = None;
    let update = app
        .updater_builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| format!("检查更新失败：{e}"))?;
    if update
        .as_ref()
        .is_some_and(|u| u.download_url.scheme() != "https")
    {
        return Err("更新包必须使用 HTTPS 下载地址。".into());
    }
    let release = update.as_ref().map(|u| Release {
        version: u.version.clone(),
        notes: u.body.clone().unwrap_or_default(),
    });
    state.update = update;
    Ok(release)
}

#[derive(Clone, serde::Serialize)]
pub struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn update_download(
    app: tauri::AppHandle,
    progress: Channel<Progress>,
) -> Result<(), String> {
    let state = app.state::<tauri::async_runtime::Mutex<UpdateState>>();
    let mut state = state.try_lock().map_err(|_| "更新任务正在进行。")?;
    state.verified = None;
    let update = state.update.as_ref().ok_or("请先检查更新。")?;
    let mut downloaded = 0;
    // Tauri sends Accept: application/octet-stream for downloads, including GitHub API assets.
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.send(Progress { downloaded, total });
            },
            || {},
        )
        .await
        .map_err(|e| format!("下载或签名校验失败：{e}"))?;
    state.verified = Some(bytes);
    Ok(())
}

#[tauri::command]
pub async fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<tauri::async_runtime::Mutex<UpdateState>>();
    let mut state = state.try_lock().map_err(|_| "更新任务正在进行。")?;
    let update = state.update.as_ref().ok_or("请先检查更新。")?;
    let bytes = state.verified.as_ref().ok_or("请先下载并校验更新包。")?;
    // This shares a lock with task admission: no generation can slip in after this check.
    crate::with_gallery(&app, |s| {
        s.work
            .install(!s.pending.is_empty() || !s.pending_groups.is_empty())
    })?;
    let result = (|| {
        let root = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let marker = serde_json::to_vec(&update.version).map_err(|e| e.to_string())?;
        std::fs::write(root.join("update-attempt.json"), marker).map_err(|e| e.to_string())?;
        // On Windows this launches NSIS, which restarts the app, then exits the old process.
        update.install(bytes).map_err(|e| e.to_string())
    })();
    if let Err(error) = result {
        state.verified = None;
        crate::with_gallery(&app, |s| {
            s.work.installation_failed();
            Ok(())
        })?;
        return Err(format!("安装失败，请重新下载或手动安装：{error}"));
    }
    app.restart();
}

#[tauri::command]
pub fn update_manual() -> Result<(), String> {
    // Fixed destination only; no arbitrary URL or executable from the WebView.
    open::that(RELEASE_PAGE).map_err(|e| format!("无法打开浏览器：{e}。请访问 {RELEASE_PAGE}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_requires_all_jobs_and_saves_to_finish() {
        let mut work = WorkState::default();
        work.begin().unwrap();
        work.begin().unwrap();
        assert!(work.install(false).is_err());
        work.finish();
        assert!(work.install(false).is_err());
        work.finish();
        assert!(work.install(true).is_err());
        work.install(false).unwrap();
        assert!(work.begin().is_err());
        assert!(work.install(false).is_err());
        work.installation_failed();
        work.begin().unwrap();
    }

    #[test]
    fn interrupted_install_is_reported_until_target_or_newer_version_runs() {
        let dir = tempfile::tempdir().unwrap();
        let old = semver::Version::parse("0.2.4").unwrap();
        assert!(incomplete_install(dir.path(), &old).unwrap().is_none());
        let marker = dir.path().join("update-attempt.json");
        std::fs::write(&marker, "\"0.3.0\"").unwrap();
        assert!(incomplete_install(dir.path(), &old)
            .unwrap()
            .unwrap()
            .contains("0.3.0"));
        assert!(marker.exists());
        assert!(
            incomplete_install(dir.path(), &semver::Version::parse("0.3.1").unwrap())
                .unwrap()
                .is_none()
        );
        assert!(!marker.exists());
    }
}
