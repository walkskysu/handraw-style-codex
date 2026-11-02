//! App-owned reference files. IPC accepts library IDs, never arbitrary file paths.
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};
use tauri::Manager;

type Result<T> = std::result::Result<T, String>;
const MAX_BYTES: u64 = 20 * 1024 * 1024;
pub const MAX_REFERENCES: usize = 12;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Reference {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryImage {
    #[serde(flatten)]
    pub reference: Reference,
    pub thumbnail: String,
}

fn directory(root: &Path, id: &str) -> Result<PathBuf> {
    let parsed = uuid::Uuid::parse_str(id).map_err(|_| "无效的参考图像编号")?;
    if parsed.to_string() != id {
        return Err("无效的参考图像编号".into());
    }
    Ok(root.join(id))
}

fn read_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "参考图像文件不可用，请重新上传或移除选择。")?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err("参考图像必须是普通文件且不超过 20 MB。".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("参考图像超过大小限制。".into());
    }
    Ok(bytes)
}

fn decode(bytes: &[u8]) -> Result<image::DynamicImage> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::WebP)) {
        return Err("无效的参考图像数据，请重新上传。".into());
    }
    let mut limits = Limits::default();
    limits.max_alloc = Some(64 * 1024 * 1024);
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("无法读取参考图像：{e}"))
}

pub fn import_data(root: &Path, name: &str, data_url: &str) -> Result<LibraryImage> {
    let encoded = data_url
        .strip_prefix("data:image/png;base64,")
        .or_else(|| data_url.strip_prefix("data:image/webp;base64,"))
        .ok_or("无效的参考图像数据。")?;
    if encoded.len() as u64 > MAX_BYTES.div_ceil(3) * 4 {
        return Err("参考图像不能超过 20 MB。".into());
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "无效的参考图像编码。")?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("参考图像不能超过 20 MB。".into());
    }
    let image = decode(&bytes)?;
    let mut png = Cursor::new(Vec::new());
    image
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    if png.get_ref().len() as u64 > MAX_BYTES {
        return Err("转换后的参考图像超过 20 MB，请缩小图片。".into());
    }
    let mut thumbnail = Cursor::new(Vec::new());
    image
        .thumbnail(240, 240)
        .write_to(&mut thumbnail, ImageFormat::WebP)
        .map_err(|e| e.to_string())?;
    let record = LibraryImage {
        reference: Reference {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.chars().filter(|c| !c.is_control()).take(200).collect(),
        },
        thumbnail: format!(
            "data:image/webp;base64,{}",
            STANDARD.encode(thumbnail.into_inner())
        ),
    };
    fs::create_dir_all(root).map_err(|e| e.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(".import-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    fs::write(staging.path().join("image.png"), png.into_inner()).map_err(|e| e.to_string())?;
    fs::write(
        staging.path().join("metadata.json"),
        serde_json::to_vec(&record).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(staging.path(), directory(root, &record.reference.id)?)
        .map_err(|e| e.to_string())?;
    Ok(record)
}

fn get(root: &Path, id: &str) -> Result<LibraryImage> {
    let dir = directory(root, id)?;
    if fs::symlink_metadata(&dir)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("参考图像目录不可用。".into());
    }
    let record: LibraryImage =
        serde_json::from_slice(&read_file(&dir.join("metadata.json"), 1024 * 1024)?)
            .map_err(|e| e.to_string())?;
    if record.reference.id != id {
        return Err("参考图像编号不匹配。".into());
    }
    Ok(record)
}

pub fn list(root: &Path) -> Result<Vec<LibraryImage>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let id = entry.file_name().to_string_lossy().into_owned();
        if uuid::Uuid::parse_str(&id).is_ok() {
            records.push(get(root, &id)?);
        }
    }
    records.sort_by(|a, b| {
        a.reference
            .name
            .cmp(&b.reference.name)
            .then(a.reference.id.cmp(&b.reference.id))
    });
    Ok(records)
}

pub fn resolve(root: &Path, ids: &[String]) -> Result<Vec<Reference>> {
    if ids.len() > MAX_REFERENCES {
        return Err(format!("最多选择 {MAX_REFERENCES} 张参考图像。"));
    }
    let mut seen = std::collections::HashSet::new();
    ids.iter()
        .map(|id| {
            if !seen.insert(id) {
                return Err("参考图像不能重复选择。".into());
            }
            let record = get(root, id)?;
            read_file(&directory(root, id)?.join("image.png"), MAX_BYTES)?;
            Ok(record.reference)
        })
        .collect()
}

/// Stable, generated relative names also work with Windows npm .cmd shims.
pub fn prepare(root: &Path, workspace: &Path, references: &[Reference]) -> Result<Vec<String>> {
    resolve(
        root,
        &references.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
    )?;
    references
        .iter()
        .enumerate()
        .map(|(index, reference)| {
            let bytes = read_file(
                &directory(root, &reference.id)?.join("image.png"),
                MAX_BYTES,
            )?;
            decode(&bytes)?;
            let name = format!("reference-{}.png", index + 1);
            fs::write(workspace.join(&name), bytes).map_err(|e| e.to_string())?;
            Ok(name)
        })
        .collect()
}

fn root(app: &tauri::AppHandle) -> Result<PathBuf> {
    Ok(app
        .path()
        .app_local_data_dir()
        .map_err(|e| e.to_string())?
        .join("references"))
}

#[tauri::command]
pub async fn reference_list(app: tauri::AppHandle) -> Result<Vec<LibraryImage>> {
    let root = root(&app)?;
    tauri::async_runtime::spawn_blocking(move || list(&root))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn reference_import(
    app: tauri::AppHandle,
    name: String,
    data_url: String,
) -> Result<LibraryImage> {
    let work = crate::WorkGuard::begin(&app)?;
    let root = root(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _work = work;
        import_data(&root, &name, &data_url)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    pub fn fixture(root: &Path, name: &str) -> Reference {
        import_data(
            root,
            name,
            &format!(
                "data:image/png;base64,{}",
                STANDARD.encode(include_bytes!("../icons/icon.png"))
            ),
        )
        .unwrap()
        .reference
    }

    #[test]
    fn imports_owned_copies_lists_after_restart_and_prepares_only_selected_files_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("references");
        let first = fixture(&root, "人物 & %PATH% $(echo bad).png");
        let second = fixture(&root, "风格.png");
        fixture(&root, "未选择.png");
        let listed = list(&root).unwrap();
        assert_eq!(listed.len(), 3);
        assert!(listed
            .iter()
            .all(|r| r.thumbnail.starts_with("data:image/webp;base64,")));
        let selected = resolve(&root, &[second.id.clone(), first.id.clone()]).unwrap();
        assert_eq!(selected, vec![second.clone(), first.clone()]);
        let workspace = tempfile::tempdir().unwrap();
        assert_eq!(
            prepare(&root, workspace.path(), &selected).unwrap(),
            vec!["reference-1.png", "reference-2.png"]
        );
        assert_eq!(
            fs::read(workspace.path().join("reference-1.png")).unwrap(),
            fs::read(directory(&root, &second.id).unwrap().join("image.png")).unwrap()
        );
        assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 2);
        assert!(!workspace.path().join(&first.name).exists());
    }

    #[test]
    fn rejects_invalid_uploads_paths_duplicate_missing_and_excessive_selections() {
        let dir = tempfile::tempdir().unwrap();
        for data in [
            "data:image/svg+xml;base64,PHN2Zy8+",
            "data:image/png;base64,bm90IGEgcG5n",
            "data:image/png;base64,!",
        ] {
            assert!(import_data(dir.path(), "bad.png", data).is_err());
        }
        let too_big = format!(
            "data:image/png;base64,{}",
            "A".repeat((MAX_BYTES.div_ceil(3) * 4 + 1) as usize)
        );
        assert!(import_data(dir.path(), "large.png", &too_big)
            .unwrap_err()
            .contains("20 MB"));
        assert!(list(dir.path()).unwrap().is_empty());
        let r = fixture(dir.path(), "safe.png");
        for ids in [
            vec!["../outside".into()],
            vec![r.id.clone(), r.id.clone()],
            vec![r.id.clone(); 13],
            vec![uuid::Uuid::new_v4().to_string()],
        ] {
            assert!(resolve(dir.path(), &ids).is_err());
        }
        fs::remove_file(directory(dir.path(), &r.id).unwrap().join("image.png")).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        assert!(prepare(dir.path(), workspace.path(), &[r])
            .unwrap_err()
            .contains("不可用"));
        assert_eq!(fs::read_dir(workspace.path()).unwrap().count(), 0);
    }

    #[test]
    fn accepts_legacy_snapshots_without_references() {
        let snapshot: crate::options::Snapshot = serde_json::from_str(
            r#"{"style":null,"layout":null,"color":null,"catalog_version":"old"}"#,
        )
        .unwrap();
        assert!(snapshot.reference_images.is_empty());
    }
}
