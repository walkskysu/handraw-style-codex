//! IPC admission, notifications and the single model worker. Database is the source of truth.
use crate::{
    gallery::PendingArtwork,
    generation::GenerationGuard,
    jobs::{Job, JobSnapshot},
    options,
    planning::{self, Draft, Plan, PlanningJob, Result},
    with_gallery, WorkGuard,
};
use tauri::{Emitter, Manager};

fn notify(app: &tauri::AppHandle, id: &str, event: &str) {
    notify_item(app, id, event, None);
}
fn notify_item(app: &tauri::AppHandle, id: &str, event: &str, item: Option<&crate::jobs::Item>) {
    let value = with_gallery(app, |s| {
        let store = s.store(app)?;
        if let Ok(j) = store.job(id) {
            return Ok(
                serde_json::json!({"job_id":j.id,"group_id":j.group_id,"revision":j.revision,"event_type":event,"item_id":item.map(|i|&i.id),"attempt_id":item.and_then(|i|i.attempt_id.as_ref())}),
            );
        }
        let j: PlanningJob = store.record("planning_jobs", id)?;
        Ok(serde_json::json!({"job_id":j.id,"revision":j.revision,"event_type":event}))
    });
    if let Ok(value) = value {
        let _ = app.emit("creation-changed", value);
    }
}
#[derive(serde::Deserialize)]
pub struct DraftInput {
    id: Option<String>,
    mode: String,
    prompt: String,
    selections: options::Selections,
    reuse_id: Option<String>,
    requested_count: Option<usize>,
    feedback: String,
    input_revision: Option<u64>,
}
#[tauri::command]
pub fn draft_update(app: tauri::AppHandle, input: DraftInput) -> Result<Draft> {
    with_gallery(&app, |s| {
        let store = s.store(&app)?;
        let previous = input.reuse_id.map(|id| store.get(&id)).transpose()?;
        let old = input.id.as_ref().map(|id| store.draft(id)).transpose()?;
        let snapshot = options::snapshot(
            &input.selections,
            previous
                .as_ref()
                .map(|a| &a.snapshot)
                .or_else(|| old.as_ref().map(|d| &d.snapshot)),
        )?;
        let tx = store
            .db
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        let mut draft = store.save_draft(
            input.id,
            input.mode,
            input.prompt,
            snapshot,
            input.requested_count,
            input.feedback,
            input.input_revision,
        )?;
        if let Some(source) = previous {
            draft.source_photo_id = Some(source.id);
            draft.source_group_id = Some(source.group_id);
            store.put_draft(&draft)?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(draft)
    })
}
#[derive(serde::Serialize)]
pub struct Library {
    warnings: Vec<String>,
    drafts: Vec<Draft>,
    plans: Vec<Plan>,
    planning_jobs: Vec<PlanningJob>,
    jobs: Vec<Job>,
}
#[tauri::command]
pub fn job_list(app: tauri::AppHandle) -> Result<Library> {
    with_gallery(&app, |s| {
        let store = s.store(&app)?;
        Ok(Library {
            warnings: store.recovery_notices()?,
            drafts: store.records("creation_drafts")?,
            plans: store.records("generation_plans")?,
            planning_jobs: store.records("planning_jobs")?,
            jobs: store.records("generation_jobs")?,
        })
    })
}
#[tauri::command]
pub fn job_get(app: tauri::AppHandle, id: String) -> Result<JobSnapshot> {
    with_gallery(&app, |s| {
        let mut snapshot = s.store(&app)?.snapshot_job(&id)?;
        // Even a disk failure that also prevents the status transaction must expose the bytes
        // retained in memory, so the user can export them instead of losing the only result.
        for i in &mut snapshot.items {
            if s.pending.contains_key(&i.id) && i.generation_status != "generated" {
                i.generation_status = "generated".into();
                i.save_status = "save_failed".into();
                i.artwork_id = Some(i.id.clone());
                i.error = Some("结果仅保留在内存中，无法保证重启恢复；请立即重试保存或导出".into());
                snapshot.job.status = "paused".into();
                snapshot.generated_count += 1;
                snapshot.pending_save_count += 1;
            }
        }
        Ok(snapshot)
    })
}
#[tauri::command]
pub fn plan_select_version(
    app: tauri::AppHandle,
    draft_id: String,
    plan_id: String,
) -> Result<Plan> {
    with_gallery(&app, |s| s.store(&app)?.select_plan(&draft_id, &plan_id))
}
#[tauri::command]
pub fn plan_create(
    app: tauri::AppHandle,
    draft_id: String,
    input_revision: u64,
    base_plan_id: Option<String>,
) -> Result<String> {
    let work = WorkGuard::begin(&app)?;
    let model = GenerationGuard::acquire()?;
    let (d, base, j) = with_gallery(&app, |s| {
        let store = s.store(&app)?;
        let d = store.draft(&draft_id)?;
        crate::generation::validate_prompt(&d.prompt)?;
        planning::count(&d)?;
        if d.mode != "planned" || d.input_revision != input_revision {
            return Err("草稿已变化，请刷新".into());
        }
        let base = base_plan_id.map(|id| store.plan(&id)).transpose()?;
        if base.as_ref().is_some_and(|p| {
            (p.draft_id != d.id || d.current_plan_id.as_deref() != Some(&p.id))
                && !(p.confirmed_at.is_some() && d.current_plan_id.is_none())
        }) {
            return Err("只能基于当前计划修订，或从已确认计划创建新草稿".into());
        }
        let j = PlanningJob {
            id: planning::id(),
            draft_id: d.id.clone(),
            base_plan_id: base.as_ref().map(|p| p.id.clone()),
            input_revision,
            status: "running".into(),
            error: None,
            output_plan_id: None,
            revision: 1,
            updated_at: planning::now(),
        };
        store.put_planning_job(&j)?;
        Ok((d, base, j))
    })?;
    let job_id = j.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _work = work;
        let mut j = j;
        let result = (|| {
            let instructions = planning::instructions(&d, base.as_ref())?;
            let mut output = model.plan(&instructions)?;
            let mut content = serde_json::from_str::<planning::Content>(&output)
                .map_err(|e| e.to_string())
                .and_then(|c| {
                    planning::validate(&c, &d)?;
                    Ok(c)
                });
            if let Err(error) = &content {
                // Exactly one text-only structural repair, never an automatic image retry.
                output=model.plan(&format!("{instructions}\nRepair this invalid JSON once. Error: {error}\nPrevious output: {output}"))?;
                content = serde_json::from_str(&output).map_err(|e| e.to_string());
            }
            let content = content?;
            with_gallery(&app, |s| {
                s.store(&app)?.save_plan(&d, content, base.map(|p| p.id))
            })
        })();
        match result {
            Ok(p) => {
                j.status = "completed".into();
                j.output_plan_id = Some(p.id);
            }
            Err(e) => {
                j.status = "failed".into();
                j.error = Some(e);
            }
        }
        j.revision += 1;
        j.updated_at = planning::now();
        let _ = with_gallery(&app, |s| s.store(&app)?.put_planning_job(&j));
        notify(&app, &j.id, "plan_finished");
    });
    Ok(job_id)
}

fn launch(app: tauri::AppHandle, j: Job, model: GenerationGuard, work: WorkGuard) {
    tauri::async_runtime::spawn_blocking(move || {
        let _work = work;
        let run: Result<()> = (|| {
            loop {
                let next = with_gallery(&app, |s| s.store(&app)?.next_item(&j.id))?;
                let Some((item, attempt)) = next else {
                    break;
                };
                notify_item(&app, &j.id, "item_started", Some(&item));
                let result = model
                    .generate(&item.prompt, &item.snapshot)
                    .and_then(|output| {
                        let mut p = PendingArtwork::new(
                            &output.image,
                            item.prompt.clone(),
                            item.snapshot.clone(),
                            crate::generation::instructions(&options::apply_snapshot(
                                &item.prompt,
                                &item.snapshot,
                            )),
                        )?;
                        p.artwork.id = item.id.clone();
                        p.artwork.image_path = format!("images/{}.png", item.id);
                        p.artwork.thumbnail_path = format!("thumbnails/{}.webp", item.id);
                        p.artwork.group_id = j.group_id.clone();
                        p.artwork.position = item.position;
                        p.artwork.generation_item_id = Some(item.id.clone());
                        p.artwork.title = item.title.clone();
                        p.artwork.tool_version = output.tool_version;
                        Ok(p)
                    });
                match result {
                    Ok(p) => {
                        let staged = with_gallery(&app, |s| {
                            s.pending.insert(item.id.clone(), p);
                            s.store(&app)?;
                            s.store.as_ref().unwrap().stage_result(
                                &item,
                                &attempt,
                                &s.pending[&item.id],
                            )
                        });
                        if let Err(e) = staged {
                            with_gallery(&app, |s| {
                                s.store(&app)?.fail_item(
                                    &item,
                                    &attempt,
                                    format!("暂存失败；无法保证重启恢复，请立即导出：{e}"),
                                    true,
                                )
                            })?;
                            notify_item(&app, &j.id, "save_failed", Some(&item));
                            break;
                        }
                        notify_item(&app, &j.id, "item_preview", Some(&item));
                        let saved = with_gallery(&app, |s| {
                            let saved = s.store(&app)?.save_item(&item.id)?;
                            s.pending.remove(&item.id);
                            Ok(saved)
                        });
                        if let Err(e) = saved {
                            with_gallery(&app, |s| {
                                s.store(&app)?.fail_item(&item, &attempt, e, true)
                            })?;
                            notify_item(&app, &j.id, "save_failed", Some(&item));
                            break;
                        }
                        notify_item(&app, &j.id, "item_saved", Some(&item));
                    }
                    Err(e) => {
                        with_gallery(&app, |s| {
                            s.store(&app)?.fail_item(&item, &attempt, e, false)
                        })?;
                        notify_item(&app, &j.id, "item_failed", Some(&item));
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = run {
            let _ = with_gallery(&app, |s| {
                let store = s.store(&app)?;
                let mut job = store.job(&j.id)?;
                job.status = "paused".into();
                store.bump(&mut job)?;
                let mut group = store.group(&j.group_id)?;
                group.generation_errors.push(error);
                store.put_group(&group)
            });
        }
        notify(&app, &j.id, "job_finished");
    });
}
fn start(
    app: tauri::AppHandle,
    draft_id: String,
    revision: u64,
    plan: Option<(String, String)>,
    key: String,
) -> Result<Job> {
    // A repeated admission returns its original job even while the worker owns the model lock.
    let old = with_gallery(&app, |s| {
        Ok(s.store(&app)?
            .records::<Job>("generation_jobs")?
            .into_iter()
            .find(|j| j.idempotency_key == key))
    })?;
    if old.is_some() {
        return with_gallery(&app, |s| {
            s.store(&app)?.start_job(
                &draft_id,
                revision,
                plan.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
                &key,
            )
        });
    }
    let work = WorkGuard::begin(&app)?;
    let model = GenerationGuard::acquire()?;
    let j = with_gallery(&app, |s| {
        s.store(&app)?.start_job(
            &draft_id,
            revision,
            plan.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
            &key,
        )
    })?;
    launch(app, j.clone(), model, work);
    Ok(j)
}
#[tauri::command]
pub fn generation_start_quick(
    app: tauri::AppHandle,
    draft_id: String,
    input_revision: u64,
    idempotency_key: String,
) -> Result<Job> {
    start(app, draft_id, input_revision, None, idempotency_key)
}
#[tauri::command]
pub fn plan_confirm_and_start(
    app: tauri::AppHandle,
    draft_id: String,
    input_revision: u64,
    plan_id: String,
    content_hash: String,
    idempotency_key: String,
) -> Result<Job> {
    start(
        app,
        draft_id,
        input_revision,
        Some((plan_id, content_hash)),
        idempotency_key,
    )
}
#[tauri::command]
pub fn generation_stop(app: tauri::AppHandle, id: String) -> Result<()> {
    with_gallery(&app, |s| s.store(&app)?.stop_job(&id))?;
    notify(&app, &id, "stopping");
    Ok(())
}
#[tauri::command]
pub fn generation_continue(
    app: tauri::AppHandle,
    id: String,
    revision: u64,
    item_id: Option<String>,
    attempt_no: Option<usize>,
) -> Result<Job> {
    let work = WorkGuard::begin(&app)?;
    let model = GenerationGuard::acquire()?;
    let j = with_gallery(&app, |s| {
        s.store(&app)?.resume_job(
            &id,
            revision,
            item_id
                .as_ref()
                .map(|id| (id.as_str(), attempt_no.unwrap_or(0))),
        )
    })?;
    launch(app, j.clone(), model, work);
    Ok(j)
}
#[tauri::command]
pub fn generation_retry_save(app: tauri::AppHandle, item_id: String) -> Result<()> {
    let job_id = with_gallery(&app, |s| {
        s.store(&app)?;
        let store = s.store.as_ref().unwrap();
        let i = store.item(&item_id)?;
        store.retry_item_save(&item_id, s.pending.get(&item_id))?;
        s.pending.remove(&item_id);
        Ok(i.job_id)
    })?;
    notify(&app, &job_id, "item_saved");
    Ok(())
}
#[tauri::command]
pub fn generation_item_image(app: tauri::AppHandle, item_id: String) -> Result<String> {
    use base64::Engine;
    with_gallery(&app, |s| {
        if let Some(p) = s.pending.get(&item_id) {
            return Ok(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&p.bytes)
            ));
        }
        let store = s.store(&app)?;
        let i = store.item(&item_id)?;
        if i.save_status == "saved" {
            return store.image(i.artwork_id.as_deref().ok_or("缺少图片")?);
        }
        Ok(format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(store.staged(&item_id)?.bytes)
        ))
    })
}
pub fn prevent_unsafe_close(window: &tauri::Window, event: &tauri::WindowEvent) {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        if with_gallery(app, |s| Ok(!s.pending.is_empty())).unwrap_or(false) {
            api.prevent_close();
            let _ = app.emit(
                "creation-close-blocked",
                "有尚未保存的结果，请重试保存或导出后处理，再退出。",
            );
        }
    }
}
