use crate::{
    gallery::{Artwork, PendingArtwork, Store},
    groups::PhotoGroup,
    options::Snapshot,
    planning::{self, id, json, now, Plan, Result},
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Serialize, Deserialize)]
pub struct Job {
    pub input_revision: u64,
    pub request_hash: String,
    pub id: String,
    pub group_id: String,
    pub draft_id: String,
    pub plan_id: Option<String>,
    pub mode: String,
    pub status: String,
    pub stop_requested: bool,
    pub expected_count: usize,
    pub revision: u64,
    pub idempotency_key: String,
    pub updated_at: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub job_id: String,
    pub position: usize,
    pub plan_item_id: String,
    pub title: String,
    pub prompt: String,
    pub snapshot: Snapshot,
    pub generation_status: String,
    pub save_status: String,
    pub attempt_no: usize,
    pub attempt_id: Option<String>,
    pub artwork_id: Option<String>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Attempt {
    pub id: String,
    pub item_id: String,
    pub attempt_no: usize,
    pub status: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub error: Option<String>,
    pub staging_path: String,
    pub checksum: Option<String>,
    pub tool_version: Option<String>,
}
#[derive(Serialize)]
pub struct JobSnapshot {
    #[serde(flatten)]
    pub job: Job,
    pub items: Vec<Item>,
    pub plan: Option<Plan>,
    pub generated_count: usize,
    pub saved_count: usize,
    pub available_count: usize,
    pub failed_count: usize,
    pub pending_save_count: usize,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    attempt_id: String,
    item_id: String,
    checksum: String,
    artwork: Artwork,
}

impl Store {
    pub fn job(&self, id: &str) -> Result<Job> {
        self.record("generation_jobs", id)
    }
    pub fn item(&self, id: &str) -> Result<Item> {
        self.record("generation_items", id)
    }
    pub fn attempt(&self, id: &str) -> Result<Attempt> {
        self.record("generation_attempts", id)
    }
    pub fn items(&self, job: &str) -> Result<Vec<Item>> {
        let mut items: Vec<Item> = self
            .records::<Item>("generation_items")?
            .into_iter()
            .filter(|i| i.job_id == job)
            .collect();
        items.sort_by_key(|i| i.position);
        Ok(items)
    }
    pub fn put_job(&self, j: &Job) -> Result<()> {
        self.db.execute("INSERT INTO generation_jobs VALUES(?,?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET status=excluded.status,revision=excluded.revision,metadata=excluded.metadata",params![j.id,j.group_id,j.plan_id,j.status,j.revision as i64,j.idempotency_key,json(j)?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn put_item(&self, i: &Item) -> Result<()> {
        self.db.execute("INSERT INTO generation_items VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET generation_status=excluded.generation_status,save_status=excluded.save_status,metadata=excluded.metadata",params![i.id,i.job_id,i.position as i64,i.generation_status,i.save_status,json(i)?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn put_attempt(&self, a: &Attempt) -> Result<()> {
        self.db.execute("INSERT INTO generation_attempts VALUES(?,?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET status=excluded.status,metadata=excluded.metadata",params![a.id,a.item_id,a.attempt_no as i64,a.status,a.staging_path,json(a)?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn snapshot_job(&self, id: &str) -> Result<JobSnapshot> {
        let job = self.job(id)?;
        let items = self.items(id)?;
        let available_count:i64=self.db.query_row("SELECT count(*) FROM artworks WHERE group_id=? AND deleted=0 AND availability='available'",[&job.group_id],|r|r.get(0)).map_err(|e|e.to_string())?;
        Ok(JobSnapshot {
            plan: job.plan_id.as_ref().map(|p| self.plan(p)).transpose()?,
            generated_count: items
                .iter()
                .filter(|i| i.generation_status == "generated")
                .count(),
            saved_count: items.iter().filter(|i| i.save_status == "saved").count(),
            failed_count: items
                .iter()
                .filter(|i| i.generation_status == "failed")
                .count(),
            pending_save_count: items
                .iter()
                .filter(|i| {
                    i.generation_status == "generated"
                        && !["saved", "discarded"].contains(&i.save_status.as_str())
                })
                .count(),
            available_count: available_count as usize,
            job,
            items,
        })
    }
    pub fn start_job(
        &self,
        draft_id: &str,
        revision: u64,
        confirmation: Option<(&str, &str)>,
        key: &str,
    ) -> Result<Job> {
        let request_hash = planning::hash(&json(&(draft_id, revision, confirmation))?);
        if key.is_empty() || key.len() > 100 {
            return Err("无效幂等键".into());
        }
        if let Some(existing) = self
            .db
            .query_row(
                "SELECT id FROM generation_jobs WHERE idempotency_key=?",
                [key],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        {
            let job = self.job(&existing)?;
            if job.request_hash != request_hash
                || job.draft_id != draft_id
                || job.plan_id.as_deref() != confirmation.map(|p| p.0)
            {
                return Err("幂等键已用于其他请求".into());
            }
            return Ok(job);
        }
        let d = self.draft(draft_id)?;
        crate::generation::validate_prompt(&d.prompt)?;
        if d.input_revision != revision {
            return Err("输入已变化，请更新计划".into());
        }
        let mut plan = if let Some((pid, digest)) = confirmation {
            let p = self.plan(pid)?;
            if d.mode != "planned"
                || d.current_plan_id.as_deref() != Some(pid)
                || p.draft_id != d.id
                || p.input_revision != revision
                || p.input_hash != d.input_hash()?
                || p.content_hash != digest
                || !d.feedback_draft.trim().is_empty()
            {
                return Err("计划过期、摘要不一致或有未应用反馈，不能确认".into());
            }
            planning::validate(&p.content, &d)?;
            if p.content_hash
                != planning::hash(&json(&(
                    &p.content,
                    &p.snapshot,
                    &p.original_prompt,
                    p.input_revision,
                ))?)
            {
                return Err("计划完整内容摘要不一致".into());
            }
            if p.confirmed_at.is_some() {
                return Err("该计划已执行，请创建新草稿".into());
            }
            Some(p)
        } else {
            if d.mode != "quick" {
                return Err("规划模式必须明确确认当前计划".into());
            }
            None
        };
        let n = plan.as_ref().map(|p| p.content.items.len()).unwrap_or(1);
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut group = PhotoGroup::new(d.prompt.clone(), d.snapshot.clone(), n, None);
        group.mode = d.mode.clone();
        group.source_photo_id = d.source_photo_id.clone();
        group.source_group_id = d.source_group_id.clone();
        if let Some(p) = &mut plan {
            p.confirmed_at = Some(now());
            group.title = p.content.title.clone();
            group.confirmed_plan = Some(serde_json::to_value(&p).map_err(|e| e.to_string())?);
            self.db
                .execute(
                    "UPDATE generation_plans SET metadata=? WHERE id=?",
                    params![json(p)?, p.id],
                )
                .map_err(|e| e.to_string())?;
        }
        self.put_group(&group)?;
        let job = Job {
            input_revision: revision,
            request_hash,
            id: id(),
            group_id: group.id,
            draft_id: d.id,
            plan_id: plan.as_ref().map(|p| p.id.clone()),
            mode: d.mode,
            status: "queued".into(),
            stop_requested: false,
            expected_count: n,
            revision: 1,
            idempotency_key: key.into(),
            updated_at: now(),
        };
        self.put_job(&job)?;
        for position in 0..n {
            let (title, prompt, pid) = if let Some(p) = &plan {
                let item = &p.content.items[position];
                (item.title.clone(),format!("整体目标：{}\n受众：{}\n统一视觉：{}\n尺寸：{}\n知识核查说明：{}\n仅输出第 {} 张独立图片，不拼接整组。\n本张文案：{}\n画面：{}\n特殊要求：{}",p.content.goal,p.content.audience,p.content.visual,p.content.dimensions,p.content.verification,position+1,item.copy,item.description,item.requirements),item.id.clone())
            } else {
                (
                    d.prompt.chars().take(40).collect(),
                    d.prompt.clone(),
                    "quick".into(),
                )
            };
            self.put_item(&Item {
                id: id(),
                job_id: job.id.clone(),
                position,
                plan_item_id: pid,
                title,
                prompt,
                snapshot: d.snapshot.clone(),
                generation_status: "queued".into(),
                save_status: "none".into(),
                attempt_no: 0,
                attempt_id: None,
                artwork_id: None,
                error: None,
            })?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(job)
    }
    pub fn bump(&self, j: &mut Job) -> Result<()> {
        j.revision += 1;
        j.updated_at = now();
        self.put_job(j)
    }
    pub fn project(&self, j: &Job) -> Result<()> {
        let items = self.items(&j.id)?;
        let mut group = self.group(&j.group_id)?;
        group.generated_count = items
            .iter()
            .filter(|i| i.generation_status == "generated")
            .count();
        group.status = match j.status.as_str() {
            "running" | "queued" | "stopping" => "generating",
            "completed" => "complete",
            s => s,
        }
        .into();
        group.generation_errors = items
            .iter()
            .filter_map(|i| {
                i.error
                    .as_ref()
                    .map(|e| format!("第 {} 张：{e}", i.position + 1))
            })
            .collect();
        self.put_group(&group)?;
        self.repair_cover(&group.id)
    }
    // All admission and stop decisions occur while the caller holds the GalleryState mutex.
    pub fn next_item(&self, id: &str) -> Result<Option<(Item, Attempt)>> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut j = self.job(id)?;
        if !["queued", "running", "stopping"].contains(&j.status.as_str()) {
            return Ok(None);
        }
        let items = self.items(id)?;
        if items.iter().any(|i| {
            i.generation_status == "generated"
                && !["saved", "discarded"].contains(&i.save_status.as_str())
        }) {
            j.status = "paused".into();
        } else if j.stop_requested {
            for mut i in items
                .iter()
                .filter(|i| i.generation_status == "queued")
                .cloned()
            {
                i.generation_status = "cancelled".into();
                self.put_item(&i)?;
            }
            j.status = if items.iter().any(|i| i.save_status == "saved") {
                "partial"
            } else {
                "cancelled"
            }
            .into();
        } else if let Some(mut item) = items
            .iter()
            .find(|i| i.generation_status == "queued")
            .cloned()
        {
            item.attempt_no += 1;
            let aid = planning::id();
            item.attempt_id = Some(aid.clone());
            item.generation_status = "generating".into();
            item.save_status = "none".into();
            item.error = None;
            let attempt = Attempt {
                id: aid.clone(),
                item_id: item.id.clone(),
                attempt_no: item.attempt_no,
                status: "running".into(),
                started_at: now(),
                ended_at: None,
                error: None,
                staging_path: format!("staging/{aid}.png"),
                checksum: None,
                tool_version: None,
            };
            self.put_item(&item)?;
            self.put_attempt(&attempt)?;
            j.status = "running".into();
            self.bump(&mut j)?;
            self.project(&j)?;
            tx.commit().map_err(|e| e.to_string())?;
            return Ok(Some((item, attempt)));
        } else {
            j.status = terminal(&items).into();
        }
        self.bump(&mut j)?;
        self.project(&j)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(None)
    }
    pub fn stop_job(&self, id: &str) -> Result<()> {
        let mut j = self.job(id)?;
        if ["queued", "running", "stopping"].contains(&j.status.as_str()) {
            j.stop_requested = true;
            j.status = "stopping".into();
            self.bump(&mut j)?;
        }
        Ok(())
    }
    pub fn resume_job(&self, id: &str, revision: u64, only: Option<(&str, usize)>) -> Result<Job> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut j = self.job(id)?;
        if j.revision != revision || ["running", "queued", "stopping"].contains(&j.status.as_str())
        {
            return Err("任务状态已变化，请刷新".into());
        }
        let items = self.items(id)?;
        if items.iter().any(|i| {
            i.generation_status == "generated"
                && !["saved", "discarded"].contains(&i.save_status.as_str())
        }) {
            return Err("请先重试保存现有结果".into());
        }
        if let Some((id, attempt)) = only {
            if !items.iter().any(|i| {
                i.id == id
                    && i.attempt_no == attempt
                    && ["failed", "cancelled", "interrupted"]
                        .contains(&i.generation_status.as_str())
            }) {
                return Err("重试位置或尝试号已变化".into());
            }
        }
        for mut i in items {
            if i.save_status == "saved" || i.generation_status == "generated" {
                continue;
            }
            if only.is_none_or(|(id, _)| i.id == id) {
                i.generation_status = "queued".into();
                i.error = None;
                self.put_item(&i)?;
            } else if i.generation_status == "queued" {
                i.generation_status = "interrupted".into();
                self.put_item(&i)?;
            }
        }
        j.stop_requested = false;
        j.status = "queued".into();
        self.bump(&mut j)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(j)
    }
    fn current(&self, item: &Item, a: &Attempt) -> Result<()> {
        let latest = self.item(&item.id)?;
        if latest.attempt_id.as_deref() != Some(&a.id)
            || latest.attempt_no != a.attempt_no
            || a.item_id != item.id
        {
            return Err("忽略过期尝试结果".into());
        }
        Ok(())
    }
    pub fn stage_result(&self, item: &Item, a: &Attempt, p: &PendingArtwork) -> Result<()> {
        self.current(item, a)?;
        if ["saved", "discarded"].contains(&self.item(&item.id)?.save_status.as_str()) {
            return Err("当前结果已处理，拒绝迟到写入".into());
        }
        if p.artwork.generation_item_id.as_deref() != Some(&item.id) || p.artwork.id != item.id {
            return Err("产物身份不一致".into());
        }
        let checksum = hash_bytes(&p.bytes);
        let manifest = Manifest {
            attempt_id: a.id.clone(),
            item_id: item.id.clone(),
            checksum: checksum.clone(),
            artwork: p.artwork.clone(),
        };
        // Both files use sync_all + same-volume atomic rename; the manifest survives failed DB writes.
        self.persist(
            &self.path(&format!("staging/{}.json", a.id))?,
            json(&manifest)?.as_bytes(),
        )?;
        self.persist(&self.path(&a.staging_path)?, &p.bytes)?;
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut i = self.item(&item.id)?;
        i.generation_status = "generated".into();
        i.save_status = "pending".into();
        i.artwork_id = Some(p.artwork.id.clone());
        i.error = None;
        self.put_item(&i)?;
        let mut a = a.clone();
        a.status = "generated".into();
        a.checksum = Some(checksum);
        a.tool_version = p.artwork.tool_version.clone();
        a.ended_at = Some(now());
        self.put_attempt(&a)?;
        let mut j = self.job(&i.job_id)?;
        self.bump(&mut j)?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn staged(&self, item_id: &str) -> Result<PendingArtwork> {
        let i = self.item(item_id)?;
        let a = self.attempt(i.attempt_id.as_deref().ok_or("无生成尝试")?)?;
        if i.save_status == "discarded" {
            return Err("此图暂存已放弃".into());
        }
        let path = self.path(&format!("staging/{}.json", a.id))?;
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 256 * 1024 {
            return Err("暂存清单过大".into());
        }
        let m: Manifest = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        self.current(&i, &a)?;
        if m.item_id != i.id
            || m.attempt_id != a.id
            || m.artwork.id != i.id
            || m.artwork.generation_item_id.as_deref() != Some(&i.id)
            || m.artwork.group_id != self.job(&i.job_id)?.group_id
        {
            return Err("暂存清单身份校验失败；文件已保留".into());
        }
        let path = self.path(&a.staging_path)?;
        if fs::metadata(&path).map_err(|e| e.to_string())?.len() > 20 * 1024 * 1024 {
            return Err("暂存图片过大".into());
        }
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        if hash_bytes(&bytes) != m.checksum {
            return Err("暂存图片摘要校验失败；文件已保留".into());
        }
        Ok(PendingArtwork {
            artwork: m.artwork,
            bytes,
        })
    }
    pub fn save_item(&self, item_id: &str) -> Result<Artwork> {
        let mut i = self.item(item_id)?;
        if i.save_status == "saved" {
            return self.get(i.artwork_id.as_deref().ok_or("无已保存图片")?);
        }
        let pending = self.staged(item_id)?;
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let a = self.save(&pending)?;
        i.generation_status = "generated".into();
        i.save_status = "saved".into();
        i.error = None;
        i.artwork_id = Some(a.id.clone());
        self.put_item(&i)?;
        let mut j = self.job(&i.job_id)?;
        if self.items(&j.id)?.iter().all(|i| i.save_status == "saved") {
            j.status = "completed".into();
        }
        self.bump(&mut j)?;
        self.project(&j)?;
        tx.commit().map_err(|e| e.to_string())?;
        // Commit is the boundary. Cleanup failure never reverts a successful save.
        if let Some(aid) = i.attempt_id {
            if let Ok(attempt) = self.attempt(&aid) {
                for relative in [attempt.staging_path, format!("staging/{aid}.json")] {
                    if let Ok(path) = self.path(&relative) {
                        let _ = fs::remove_file(path);
                    }
                }
            }
        }
        Ok(a)
    }
    pub fn retry_item_save(&self, id: &str, pending: Option<&PendingArtwork>) -> Result<Artwork> {
        let item = self.item(id)?;
        if item.save_status != "saved" {
            if let Some(pending) = pending {
                let attempt = self.attempt(item.attempt_id.as_deref().ok_or("缺少生成尝试")?)?;
                self.stage_result(&item, &attempt, pending)?;
            }
        }
        self.save_item(id)
    }
    pub fn exportable(&self, id: &str) -> Result<(Artwork, Vec<u8>)> {
        if let Ok(artwork) = self.get(id) {
            let bytes = self.bytes(&artwork)?;
            return Ok((artwork, bytes));
        }
        let pending = self.staged(id)?;
        Ok((pending.artwork, pending.bytes))
    }
    pub fn discard_result(&self, id: &str) -> Result<()> {
        let mut i = self.item(id)?;
        let mut j = self.job(&i.job_id)?;
        if i.generation_status != "generated"
            || i.save_status == "saved"
            || ["running", "queued", "stopping"].contains(&j.status.as_str())
        {
            return Err("只能放弃已暂停任务中尚未入库的当前结果".into());
        }
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        i.save_status = "discarded".into();
        i.error = Some("用户已明确放弃此图暂存；未入库".into());
        self.put_item(&i)?;
        j.status = terminal(&self.items(&j.id)?).into();
        self.bump(&mut j)?;
        self.project(&j)?;
        tx.commit().map_err(|e| e.to_string())?;
        if let Some(aid) = i.attempt_id {
            let a = self.attempt(&aid)?;
            let _ = fs::remove_file(self.path(&a.staging_path)?);
            let _ = fs::remove_file(self.path(&format!("staging/{aid}.json"))?);
        }
        Ok(())
    }
    pub fn fail_item(&self, item: &Item, a: &Attempt, error: String, save: bool) -> Result<()> {
        self.current(item, a)?;
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut i = self.item(&item.id)?;
        i.error = Some(error.clone());
        if save {
            i.generation_status = "generated".into();
            i.save_status = "save_failed".into();
            i.artwork_id = Some(i.id.clone());
        } else {
            i.generation_status = "failed".into();
        }
        self.put_item(&i)?;
        let mut a = a.clone();
        a.error = Some(error.clone());
        a.ended_at = Some(now());
        a.status = if save { "generated" } else { "failed" }.into();
        self.put_attempt(&a)?;
        let mut j = self.job(&item.job_id)?;
        // Unknown and shared setup failures pause conservatively. Only definite item failures continue.
        if save || !ordinary_item_failure(&error) {
            j.status = "paused".into();
        }
        self.bump(&mut j)?;
        self.project(&j)?;
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn recover_jobs(&self) -> Result<()> {
        for mut p in self.records::<planning::PlanningJob>("planning_jobs")? {
            if p.status == "running" {
                p.status = "interrupted".into();
                p.error = Some("规划被中断；上个完整版本和反馈已保留".into());
                p.revision += 1;
                self.put_planning_job(&p)?;
            }
        }
        for mut j in self.records::<Job>("generation_jobs")? {
            for mut i in self.items(&j.id)? {
                if let Some(aid) = &i.attempt_id {
                    let a = self.attempt(aid)?;
                    if ["saved", "discarded"].contains(&i.save_status.as_str()) {
                        let _ = fs::remove_file(self.path(&a.staging_path)?);
                        let _ = fs::remove_file(self.path(&format!("staging/{aid}.json"))?);
                        continue;
                    }
                    if self.path(&a.staging_path)?.is_file() {
                        match self.staged(&i.id) {
                            Ok(p) => {
                                self.stage_result(&i, &a, &p)?;
                                if let Err(e) = self.save_item(&i.id) {
                                    self.fail_item(&i, &a, e, true)?;
                                }
                            }
                            Err(e) => {
                                i.generation_status = "generated".into();
                                i.save_status = "missing".into();
                                i.error = Some(e);
                                self.put_item(&i)?;
                            }
                        }
                    } else if i.generation_status == "generating" {
                        i.generation_status = "interrupted".into();
                        i.error = Some("应用退出，等待手动继续".into());
                        self.put_item(&i)?;
                        let mut attempt = a;
                        attempt.status = "interrupted".into();
                        attempt.ended_at = Some(now());
                        self.put_attempt(&attempt)?;
                    } else if i.generation_status == "generated" {
                        i.save_status = "missing".into();
                        i.error = Some("暂存文件缺失；未入库，不能自动重新生成".into());
                        self.put_item(&i)?;
                    }
                }
            }
            j = self.job(&j.id)?;
            let items = self.items(&j.id)?;
            if items.iter().any(|i| {
                i.generation_status == "generated"
                    && !["saved", "discarded"].contains(&i.save_status.as_str())
            }) {
                j.status = "paused".into();
            } else if ["queued", "running", "stopping"].contains(&j.status.as_str()) {
                j.status = "interrupted".into();
            }
            self.bump(&mut j)?;
            self.project(&j)?;
        }
        Ok(())
    }
    pub fn assert_group_idle(&self, group_id: &str) -> Result<()> {
        let active:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM generation_jobs WHERE group_id=? AND status IN ('queued','running','stopping'))",[group_id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if active {
            Err("请先停止本组任务；当前图片完成后才可删除".into())
        } else {
            Ok(())
        }
    }
}
fn ordinary_item_failure(error: &str) -> bool {
    // Missing output can mean unavailable image tooling; pause rather than exhaust the queue.
    [
        "超过 10 分钟",
        "无效的 PNG",
        "不是有效的 PNG",
        "图片超过 20 MB",
        "图片分辨率过大",
    ]
    .iter()
    .any(|message| error.contains(message))
}
fn terminal(items: &[Item]) -> &'static str {
    if items.iter().all(|i| i.save_status == "saved") {
        "completed"
    } else if items.iter().any(|i| i.save_status == "saved") {
        "partial"
    } else if items.iter().all(|i| i.generation_status == "failed") {
        "failed"
    } else {
        "cancelled"
    }
}
pub fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
