mod generation;

#[tauri::command]
async fn generate_image(prompt: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || generation::generate(&prompt))
        .await
        .map_err(|_| "图片生成任务意外结束，请重试。".to_string())?
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![generate_image])
        .run(tauri::generate_context!())
        .expect("启动 Handraw 时发生错误");
}
