use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Selections {
    pub style: Option<String>,
    pub layout: Option<String>,
    pub color: Option<String>,
}

#[derive(Deserialize)]
struct Catalog {
    style: Vec<CatalogOption>,
    layout: Vec<CatalogOption>,
    color: Vec<CatalogOption>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct CatalogOption {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub struct Snapshot {
    pub style: Option<CatalogOption>,
    pub layout: Option<CatalogOption>,
    pub color: Option<CatalogOption>,
    pub catalog_version: String,
}

pub fn snapshot(selections: &Selections, previous: Option<&Snapshot>) -> Result<Snapshot, String> {
    let catalog: Catalog = serde_json::from_str(CATALOG).map_err(|e| e.to_string())?;
    let resolve = |id: &Option<String>, options: &[CatalogOption], old: Option<&CatalogOption>| {
        id.as_ref()
            .map(|id| {
                old.filter(|o| &o.id == id)
                    .or_else(|| options.iter().find(|o| &o.id == id))
                    .cloned()
                    .ok_or_else(|| format!("无效的选项：{id}"))
            })
            .transpose()
    };
    let mut result = Snapshot {
        style: resolve(
            &selections.style,
            &catalog.style,
            previous.and_then(|p| p.style.as_ref()),
        )?,
        layout: resolve(
            &selections.layout,
            &catalog.layout,
            previous.and_then(|p| p.layout.as_ref()),
        )?,
        color: resolve(
            &selections.color,
            &catalog.color,
            previous.and_then(|p| p.color.as_ref()),
        )?,
        catalog_version: format!("sha256:{:x}", Sha256::digest(CATALOG.as_bytes())),
    };
    // A mixed snapshot retains both provenance versions; the specifications themselves are authoritative.
    if let Some(old) = previous {
        let reuses_old = [
            (&selections.style, &old.style),
            (&selections.layout, &old.layout),
            (&selections.color, &old.color),
        ]
        .iter()
        .any(|(id, option)| {
            id.as_ref()
                .zip(option.as_ref())
                .is_some_and(|(id, option)| id == &option.id)
        });
        if reuses_old && old.catalog_version != result.catalog_version {
            result.catalog_version = if old
                .catalog_version
                .starts_with(&format!("{}; reused:", result.catalog_version))
            {
                old.catalog_version.clone()
            } else {
                format!("{}; reused:{}", result.catalog_version, old.catalog_version)
            };
        }
    }
    Ok(result)
}

const CATALOG: &str = include_str!("../../src/data/generationCatalog.json");

// Resolve IDs on the trusted side of IPC. The standalone app needs no source checkout
// or network access to supply the full specifications to Codex's temporary workspace.
#[cfg(test)]
pub fn apply_selections(prompt: &str, selections: &Selections) -> Result<String, String> {
    Ok(apply_snapshot(prompt, &snapshot(selections, None)?))
}

pub fn apply_snapshot(prompt: &str, snapshot: &Snapshot) -> String {
    let mut constraints = Vec::new();
    for (label, option) in [
        ("手绘风格 / style", &snapshot.style),
        ("排版图型 / layout", &snapshot.layout),
        ("经典单色主题色 / color", &snapshot.color),
    ] {
        if let Some(option) = option {
            constraints.push(format!(
                "{label}: {} · {}\n{}",
                option.id, option.name, option.prompt
            ));
        }
    }
    if constraints.is_empty() {
        return prompt.to_owned();
    }
    format!(
        "{prompt}\n\nMandatory user-selected rendering constraints:\n\
         Apply every selected specification below to the generated image, not just to your reply. \
         These explicit selections override conflicting requests in the image description. \
         Selected style controls drawing technique, linework and texture; selected layout controls \
         composition and typography; selected color controls the monochrome theme palette, including \
         when style or layout examples mention different colors. If a color is selected, use shades/tints \
         of that theme color and neutral paper/ink as needed. Categories not selected remain unconstrained. \
         Adapt example subjects and copy in the specifications to the user's image description. \
         Make any necessary template decisions yourself without asking follow-up questions. \
         Render exactly one finished image; do not render option lists, IDs or instructions.\n\n{}",
        constraints.join("\n\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuse_preserves_historical_specifications_even_for_removed_options() {
        let old = Snapshot {
            style: Some(CatalogOption {
                id: "removed-id".into(),
                name: "旧风格".into(),
                prompt: "历史规范".into(),
            }),
            catalog_version: "old-version".into(),
            ..Default::default()
        };
        let reused = snapshot(
            &Selections {
                style: Some("removed-id".into()),
                color: Some("C-01".into()),
                ..Default::default()
            },
            Some(&old),
        )
        .unwrap();
        assert!(apply_snapshot("小猫", &reused).contains("历史规范"));
        assert!(reused.catalog_version.contains("old-version"));
        assert!(reused.color.is_some());
        let changed = snapshot(
            &Selections {
                style: Some("FA-001".into()),
                ..Default::default()
            },
            Some(&old),
        )
        .unwrap();
        assert!(!apply_snapshot("小猫", &changed).contains("历史规范"));
        assert!(snapshot(
            &Selections {
                style: Some("removed-id".into()),
                ..Default::default()
            },
            None
        )
        .is_err());
    }

    #[test]
    fn all_catalog_options_resolve_with_their_full_specifications() {
        let catalog: Catalog = serde_json::from_str(CATALOG).unwrap();
        assert_eq!(
            (
                catalog.style.len(),
                catalog.layout.len(),
                catalog.color.len()
            ),
            (331, 167, 36)
        );
        for (category, options) in [
            ("style", catalog.style),
            ("layout", catalog.layout),
            ("color", catalog.color),
        ] {
            for option in options {
                let selections: Selections =
                    serde_json::from_value(serde_json::json!({ category: option.id })).unwrap();
                let result = apply_selections("小猫", &selections).unwrap();
                assert!(result.contains(&option.prompt));
                assert!(result.contains(&option.id));
            }
        }
    }

    #[test]
    fn selections_are_independent_optional_and_combined_with_explicit_precedence() {
        assert_eq!(
            apply_selections("原始\n描述", &Selections::default()).unwrap(),
            "原始\n描述"
        );
        for mask in 1..8 {
            let selections = Selections {
                style: (mask & 1 != 0).then(|| "FA-001".into()),
                layout: (mask & 2 != 0).then(|| "SC-001".into()),
                color: (mask & 4 != 0).then(|| "C-01".into()),
            };
            let result = apply_selections("彩色小猫", &selections).unwrap();
            assert!(result.starts_with("彩色小猫"));
            assert_eq!(result.contains("松散黑线"), mask & 1 != 0);
            assert_eq!(result.contains("上文下图结构"), mask & 2 != 0);
            assert_eq!(result.contains("Klein Blue"), mask & 4 != 0);
            assert!(result.contains("override conflicting requests"));
        }
    }

    #[test]
    fn rejects_unknown_and_cross_category_ids_instead_of_silently_ignoring_them() {
        for id in ["", "unknown", "C-01", "../../STYLES.md"] {
            let selections = Selections {
                style: Some(id.into()),
                ..Default::default()
            };
            assert!(apply_selections("小猫", &selections)
                .unwrap_err()
                .contains("无效"));
        }
        assert!(serde_json::from_str::<Selections>(r#"{"colour":"C-01"}"#).is_err());
    }
}
