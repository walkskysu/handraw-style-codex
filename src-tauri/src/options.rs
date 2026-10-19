use serde::Deserialize;

#[derive(Default, Deserialize)]
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

#[derive(Deserialize)]
struct CatalogOption {
    id: String,
    name: String,
    prompt: String,
}

const CATALOG: &str = include_str!("../../src/data/generationCatalog.json");

// Resolve IDs on the trusted side of IPC. The standalone app needs no source checkout
// or network access to supply the full specifications to Codex's temporary workspace.
pub fn apply_selections(prompt: &str, selections: &Selections) -> Result<String, String> {
    let catalog: Catalog =
        serde_json::from_str(CATALOG).map_err(|_| "无法读取生图选项目录，请重新安装应用。")?;
    let mut constraints = Vec::new();
    for (label, id, options) in [
        ("手绘风格 / style", &selections.style, &catalog.style),
        ("排版图型 / layout", &selections.layout, &catalog.layout),
        ("经典单色主题色 / color", &selections.color, &catalog.color),
    ] {
        if let Some(id) = id {
            let option = options
                .iter()
                .find(|option| &option.id == id)
                .ok_or_else(|| format!("无效的{label}选项：{id}"))?;
            constraints.push(format!(
                "{label}: {} · {}\n{}",
                option.id, option.name, option.prompt
            ));
        }
    }
    if constraints.is_empty() {
        return Ok(prompt.to_owned());
    }
    Ok(format!(
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
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
