use crate::{gallery::Store, options::Snapshot};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
pub type Result<T> = std::result::Result<T, String>;
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn json<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}
pub fn hash(v: &str) -> String {
    format!("{:x}", Sha256::digest(v.as_bytes()))
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: String,
    pub mode: String,
    pub prompt: String,
    pub snapshot: Snapshot,
    pub requested_count: Option<usize>,
    pub input_revision: u64,
    pub current_plan_id: Option<String>,
    pub feedback_draft: String,
    pub updated_at: String,
    #[serde(default)]
    pub source_photo_id: Option<String>,
    #[serde(default)]
    pub source_group_id: Option<String>,
}
impl Draft {
    pub fn input_hash(&self) -> Result<String> {
        Ok(hash(&json(&(
            &self.prompt,
            &self.snapshot,
            self.requested_count,
            self.input_revision,
        ))?))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanItem {
    pub id: String,
    pub position: usize,
    pub title: String,
    pub copy: String,
    pub description: String,
    pub requirements: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Content {
    pub title: String,
    pub goal: String,
    pub audience: String,
    pub kind: String,
    pub visual: String,
    pub dimensions: String,
    pub verification: String,
    pub change_summary: String,
    pub items: Vec<PlanItem>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: String,
    pub draft_id: String,
    pub version: usize,
    pub parent_plan_id: Option<String>,
    pub input_revision: u64,
    pub input_hash: String,
    pub content_hash: String,
    pub content: Content,
    pub snapshot: Snapshot,
    pub original_prompt: String,
    pub schema_version: u32,
    pub feedback: String,
    pub confirmed_at: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct PlanningJob {
    pub id: String,
    pub draft_id: String,
    pub base_plan_id: Option<String>,
    pub input_revision: u64,
    pub status: String,
    pub error: Option<String>,
    pub output_plan_id: Option<String>,
    pub revision: u64,
    pub updated_at: String,
}

pub fn explicit_count(prompt: &str) -> Option<usize> {
    // Deliberately require an image/card unit; unrelated numbers are not counts.
    let chars: Vec<_> = prompt.chars().collect();
    for i in 0..chars.len() {
        let remainder: String = chars[i + 1..].iter().collect();
        let card_unit = chars[i] == '个'
            && ["知识卡片", "卡片", "图片", "变体"]
                .iter()
                .any(|s| remainder.trim_start().starts_with(s));
        if !matches!(chars[i], '张' | '幅' | '款') && !card_unit {
            continue;
        }
        let mut end = i;
        while end > 0 && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0
            && (chars[start - 1].is_ascii_digit()
                || "零〇一二三四五六七八九十两百千万".contains(chars[start - 1]))
        {
            start -= 1;
        }
        let n: String = chars[start..end].iter().collect();
        if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) {
            return Some(n.parse().unwrap_or(usize::MAX));
        }
        if let Some(n) = [
            "一", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二",
        ]
        .iter()
        .position(|s| *s == n)
        {
            return Some(n + 1);
        }
        if n == "两" {
            return Some(2);
        }
        if n == "零" || n == "〇" {
            return Some(0);
        }
        if n.chars().any(|c| "十百千万".contains(c)) {
            return Some(13);
        } // rejected, never silently truncate
    }
    let words: Vec<_> = prompt.split_whitespace().collect();
    words.windows(2).find_map(|pair| {
        if ["images", "pictures", "cards", "variants"].contains(
            &pair[1]
                .trim_matches(|c: char| !c.is_ascii_alphabetic())
                .to_lowercase()
                .as_str(),
        ) && pair[0].chars().all(|c| c.is_ascii_digit())
        {
            Some(pair[0].parse().unwrap_or(usize::MAX))
        } else {
            None
        }
    })
}
pub fn count(d: &Draft) -> Result<Option<usize>> {
    let n = d.requested_count.or_else(|| explicit_count(&d.prompt));
    if n.is_some_and(|n| !(1..=12).contains(&n)) {
        return Err("规划数量需为 1–12 张，请修改数量。".into());
    }
    Ok(n)
}
pub fn validate(c: &Content, d: &Draft) -> Result<()> {
    if json(c)?.len() > 128 * 1024 {
        return Err("计划超过 128 KB".into());
    }
    if !(1..=12).contains(&c.items.len()) || count(d)?.is_some_and(|n| n != c.items.len()) {
        return Err("计划张数不符合要求".into());
    }
    if !["series", "variants"].contains(&c.kind.as_str()) {
        return Err("计划类型需为 series 或 variants".into());
    }
    let required = |s: &str| !s.trim().is_empty() && s.chars().count() <= 8000;
    if [
        &c.title,
        &c.goal,
        &c.audience,
        &c.visual,
        &c.dimensions,
        &c.verification,
    ]
    .iter()
    .any(|s| !required(s))
    {
        return Err("计划整体字段为空或过长".into());
    }
    let mut ids = HashSet::new();
    for (i, item) in c.items.iter().enumerate() {
        if item.position != i
            || item.id.len() > 100
            || !required(&item.id)
            || !ids.insert(&item.id)
            || [
                &item.title,
                &item.copy,
                &item.description,
                &item.requirements,
            ]
            .iter()
            .any(|s| !required(s))
        {
            return Err("计划条目需具有唯一 ID、连续序号和完整内容（无文案时填写‘无’）".into());
        }
    }
    Ok(())
}

impl Store {
    pub fn draft(&self, id: &str) -> Result<Draft> {
        self.record("creation_drafts", id)
    }
    pub fn plan(&self, id: &str) -> Result<Plan> {
        self.record("generation_plans", id)
    }
    pub fn record<T: serde::de::DeserializeOwned>(&self, table: &str, id: &str) -> Result<T> {
        // Table names are compile-time call-site constants, never IPC data.
        let value: String = self
            .db
            .query_row(
                &format!("SELECT metadata FROM {table} WHERE id=?"),
                [id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        serde_json::from_str(&value).map_err(|e| e.to_string())
    }
    pub fn records<T: serde::de::DeserializeOwned>(&self, table: &str) -> Result<Vec<T>> {
        let mut q = self
            .db
            .prepare(&format!("SELECT metadata FROM {table} ORDER BY rowid DESC"))
            .map_err(|e| e.to_string())?;
        let rows = q
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn put_draft(&self, d: &Draft) -> Result<()> {
        self.db.execute("INSERT INTO creation_drafts VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET input_revision=excluded.input_revision,metadata=excluded.metadata",params![d.id,d.input_revision as i64,json(d)?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn save_draft(
        &self,
        old: Option<String>,
        mode: String,
        prompt: String,
        snapshot: Snapshot,
        requested_count: Option<usize>,
        feedback: String,
        revision: Option<u64>,
    ) -> Result<Draft> {
        if !["quick", "planned"].contains(&mode.as_str())
            || prompt.chars().count() > 4000
            || feedback.chars().count() > 4000
        {
            return Err("无效草稿".into());
        }
        let mut d = if let Some(id) = old {
            self.draft(&id)?
        } else {
            Draft {
                id: id(),
                mode: mode.clone(),
                prompt: String::new(),
                snapshot: Snapshot::default(),
                requested_count: None,
                input_revision: 0,
                current_plan_id: None,
                feedback_draft: String::new(),
                updated_at: now(),
                source_photo_id: None,
                source_group_id: None,
            }
        };
        if revision.is_some_and(|r| r != d.input_revision) {
            return Err("草稿已更新，请刷新".into());
        }
        if d.mode != mode
            || d.prompt != prompt
            || json(&d.snapshot)? != json(&snapshot)?
            || d.requested_count != requested_count
        {
            d.input_revision += 1;
        }
        d.mode = mode;
        d.prompt = prompt;
        d.snapshot = snapshot;
        d.requested_count = requested_count;
        d.feedback_draft = feedback;
        d.updated_at = now();
        self.put_draft(&d)?;
        Ok(d)
    }
    pub fn put_planning_job(&self, j: &PlanningJob) -> Result<()> {
        self.db.execute("INSERT INTO planning_jobs VALUES(?,?,?,?,?) ON CONFLICT(id) DO UPDATE SET status=excluded.status,revision=excluded.revision,metadata=excluded.metadata",params![j.id,j.draft_id,j.status,j.revision as i64,json(j)?]).map_err(|e|e.to_string())?;
        Ok(())
    }
    pub fn save_plan(&self, d: &Draft, content: Content, parent: Option<String>) -> Result<Plan> {
        validate(&content, d)?;
        let tx = self.db.unchecked_transaction().map_err(|e| e.to_string())?;
        let mut current = self.draft(&d.id)?;
        if current.input_hash() != d.input_hash() || current.feedback_draft != d.feedback_draft {
            return Err("输入已变化，请重新规划".into());
        }
        let version: i64 = self
            .db
            .query_row(
                "SELECT coalesce(max(version),0)+1 FROM generation_plans WHERE draft_id=?",
                [&d.id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let p = Plan {
            id: id(),
            draft_id: d.id.clone(),
            version: version as usize,
            parent_plan_id: parent,
            input_revision: d.input_revision,
            input_hash: d.input_hash()?,
            content_hash: hash(&json(&(
                &content,
                &d.snapshot,
                &d.prompt,
                d.input_revision,
            ))?),
            content,
            snapshot: d.snapshot.clone(),
            original_prompt: d.prompt.clone(),
            schema_version: 1,
            feedback: d.feedback_draft.clone(),
            confirmed_at: None,
        };
        self.db
            .execute(
                "INSERT INTO generation_plans VALUES(?,?,?,?)",
                params![p.id, p.draft_id, p.version as i64, json(&p)?],
            )
            .map_err(|e| e.to_string())?;
        current.current_plan_id = Some(p.id.clone());
        current.feedback_draft.clear();
        self.put_draft(&current)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(p)
    }
    pub fn select_plan(&self, draft_id: &str, plan_id: &str) -> Result<Plan> {
        let d = self.draft(draft_id)?;
        let p = self.plan(plan_id)?;
        let fork = p.confirmed_at.is_some()
            && d.current_plan_id.is_none()
            && p.original_prompt == d.prompt
            && json(&p.snapshot)? == json(&d.snapshot)?;
        if !fork && (p.draft_id != d.id || p.input_hash != d.input_hash()?) {
            return Err("历史计划与当前输入不一致，请重新规划".into());
        }
        self.save_plan(&d, p.content, Some(p.id))
    }
}

pub fn instructions(d: &Draft, base: Option<&Plan>) -> Result<String> {
    Ok(format!("You are a text-only image planner. Never generate images or call tools. Return only the JSON matching the provided schema. Plan 1–12 independent images. Quantity constraint: {:?}; this overrides numbers in the description. Distinguish series from variations. Preserve stable item IDs when revising. All positions start at 0. Include exact image copy, concrete visual directions, requirements, common visual rules and dimensions. Respect the complete selected snapshot. Decide unspecified visual rules now. Record sources or explicitly state unverified in verification; structural validation is not fact checking. Use Chinese for user-facing text. No empty required strings; use 无 where appropriate.\nDraft: {}\nBase complete plan: {}",count(d)?,json(d)?,json(&base)?))
}
