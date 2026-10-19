#[tauri::command]
fn greet(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("请先填写你的名字。".into());
    }

    Ok(format!("你好，{name}！欢迎来到 Handraw。"))
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("启动 Handraw 时发生错误");
}

#[cfg(test)]
mod tests {
    use super::greet;

    #[test]
    fn greets_a_chinese_name() {
        let expected = "你好，小画家！欢迎来到 Handraw。";
        assert_eq!(greet("小画家").unwrap(), expected);
    }

    #[test]
    fn trims_surrounding_whitespace() {
        let expected = "你好，Handraw！欢迎来到 Handraw。";
        assert_eq!(greet("  Handraw \n").unwrap(), expected);
    }

    #[test]
    fn rejects_empty_and_whitespace_only_names() {
        for name in ["", " \t\n", "\u{3000}"] {
            assert_eq!(greet(name), Err("请先填写你的名字。".into()));
        }
    }
}
