//! Delete task records atomically while preserving independently saved gallery works.
use crate::{gallery::Store, jobs::Job, planning::Result};

impl Store {
    pub fn delete_task(&self, id: &str, kind: &str) -> Result<Vec<String>> {
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let (draft_id, job) = match kind {
            "draft" => (self.draft(id)?.id, None),
            "job" => {
                let job = self.job(id)?;
                if ["queued", "running", "stopping"].contains(&job.status.as_str()) {
                    return Err("任务正在生成，请停止并等待当前图片完成后再删除".into());
                }
                (job.draft_id.clone(), Some(job))
            }
            _ => return Err("无效的任务类型".into()),
        };
        let planning: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM planning_jobs WHERE (draft_id=?1 OR base_plan_id IN (SELECT id FROM generation_plans WHERE draft_id=?1)) AND status IN ('queued','running','stopping'))",
            [&draft_id], |r| r.get(0),
        ).map_err(|e| e.to_string())?;
        if planning {
            return Err("任务正在规划，请等待规划结束后再删除".into());
        }
        let linked = self.records::<Job>("generation_jobs")?;
        if job.is_none() && linked.iter().any(|j| j.draft_id == draft_id) {
            return Err("草稿已开始生成，请刷新后删除对应任务".into());
        }
        let mut removed = Vec::new();
        let mut staging = Vec::new();
        if let Some(job) = &job {
            for item in self.items(&job.id)? {
                // Gallery metadata and image files outlive the task. Remove only the FK.
                self.db.execute("UPDATE artworks SET generation_item_id=NULL, metadata=json_set(metadata,'$.generation_item_id',NULL) WHERE generation_item_id=?", [&item.id]).map_err(|e| e.to_string())?;
                for attempt in self
                    .records::<crate::jobs::Attempt>("generation_attempts")?
                    .into_iter()
                    .filter(|a| a.item_id == item.id)
                {
                    staging.push(self.path(&attempt.staging_path)?);
                    staging.push(self.path(&format!("staging/{}.json", attempt.id))?);
                }
                removed.push(item.id);
            }
            // Validate the complete removal at commit.
            self.db
                .execute_batch("PRAGMA defer_foreign_keys=ON")
                .map_err(|e| e.to_string())?;
            self.db.execute("DELETE FROM generation_attempts WHERE item_id IN (SELECT id FROM generation_items WHERE job_id=?)", [&job.id]).map_err(|e| e.to_string())?;
            self.db
                .execute("DELETE FROM generation_items WHERE job_id=?", [&job.id])
                .map_err(|e| e.to_string())?;
            self.db
                .execute("DELETE FROM generation_jobs WHERE id=?", [&job.id])
                .map_err(|e| e.to_string())?;
        }
        if !linked
            .iter()
            .any(|j| j.draft_id == draft_id && Some(&j.id) != job.as_ref().map(|j| &j.id))
        {
            self.db
                .execute_batch("PRAGMA defer_foreign_keys=ON")
                .map_err(|e| e.to_string())?;
            // A copied plan may cite this task's plan as its parent. Keep its content.
            self.db.execute("UPDATE generation_plans SET metadata=json_set(metadata,'$.parent_plan_id',NULL) WHERE draft_id<>?1 AND parent_plan_id IN (SELECT id FROM generation_plans WHERE draft_id=?1)", [&draft_id]).map_err(|e| e.to_string())?;
            self.db.execute("UPDATE planning_jobs SET metadata=json_set(metadata,'$.base_plan_id',NULL) WHERE draft_id<>?1 AND base_plan_id IN (SELECT id FROM generation_plans WHERE draft_id=?1)", [&draft_id]).map_err(|e| e.to_string())?;
            self.db
                .execute("DELETE FROM planning_jobs WHERE draft_id=?", [&draft_id])
                .map_err(|e| e.to_string())?;
            self.db
                .execute("DELETE FROM generation_plans WHERE draft_id=?", [&draft_id])
                .map_err(|e| e.to_string())?;
            self.db
                .execute("DELETE FROM creation_drafts WHERE id=?", [&draft_id])
                .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        // Only unneeded staging files are cleaned up, after the database commit.
        for path in staging {
            let _ = std::fs::remove_file(path);
        }
        Ok(removed)
    }
}
