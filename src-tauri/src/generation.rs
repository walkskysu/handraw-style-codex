use base64::{engine::general_purpose::STANDARD, Engine};
use std::{
    env,
    fs::{self, File},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

const OUTPUT_NAME: &str = "result.png";
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10 * 60);
static BUSY: AtomicBool = AtomicBool::new(false);

struct GenerationGuard;
impl GenerationGuard {
    fn acquire() -> Result<Self, String> {
        BUSY.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| "已有图片正在生成，请稍候。".to_string())?;
        Ok(Self)
    }
}
impl Drop for GenerationGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

pub fn generate(prompt: &str) -> Result<String, String> {
    let prompt = validate_prompt(prompt)?;
    let _guard = GenerationGuard::acquire()?;
    let executable = find_codex()?;
    let workspace = tempfile::Builder::new()
        .prefix("handraw-")
        .tempdir()
        .map_err(|e| format!("无法创建图片生成目录：{e}"))?;
    let mut command = codex_command(&executable);
    run_generation(&mut command, workspace.path(), prompt, TIMEOUT)
}

fn validate_prompt(prompt: &str) -> Result<&str, String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("请先描述你想生成的画面。".into());
    }
    if prompt.chars().count() > 4000 {
        return Err("提示词不能超过 4000 个字符。".into());
    }
    Ok(prompt)
}

fn find_codex() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let names = ["codex.exe", "codex.cmd"];
    #[cfg(not(windows))]
    let names = ["codex"];
    // Resolve before changing cwd; never search the generated workspace for executables.
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        if !directory.is_absolute() {
            continue;
        }
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err("未找到 Codex。请安装 Codex CLI，运行 codex login，并重启 Handraw 以更新 PATH。".into())
}

fn codex_command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    // Only fixed arguments enter the command line, including Windows npm .cmd shims.
    // User text goes through stdin, never through shell interpolation.
    command.args([
        "--ask-for-approval",
        "never",
        "exec",
        "--skip-git-repo-check",
        "--sandbox",
        "workspace-write",
        "--config",
        "sandbox_workspace_write.network_access=true",
        "--color",
        "never",
        "-",
    ]);
    hide_window(&mut command);
    command
}

fn hide_window(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = command;
}

// Reap on errors/timeouts; on Windows include the npm shim's child processes.
struct RunningChild(Child);
impl Drop for RunningChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        #[cfg(windows)]
        {
            let mut kill = Command::new("taskkill.exe");
            kill.args(["/PID", &self.0.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            hide_window(&mut kill);
            let _ = kill.status();
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn run_generation(
    command: &mut Command,
    workspace: &Path,
    prompt: &str,
    timeout: Duration,
) -> Result<String, String> {
    let instructions = format!(
        "Generate exactly one image using your available image-generation tool or skill. \
         Save the actual generated image as PNG at ./result.png in the current directory. \
         If the tool saves elsewhere, copy/convert that image to ./result.png. \
         Do not substitute a programmatically drawn placeholder. If image generation is \
         unavailable, explain the missing setup and fail without creating result.png. \
         Do not modify files outside this working directory.\n\nImage description:\n{prompt}"
    );
    // File-backed stdin avoids a blocked pipe if CLI startup fails before reading input.
    let mut input = tempfile::tempfile().map_err(|e| format!("无法准备提示词：{e}"))?;
    input
        .write_all(instructions.as_bytes())
        .map_err(|e| e.to_string())?;
    input.rewind().map_err(|e| e.to_string())?;
    let diagnostics = tempfile::tempfile().map_err(|e| e.to_string())?;
    command
        .current_dir(workspace)
        .stdin(Stdio::from(input))
        .stdout(Stdio::from(
            diagnostics.try_clone().map_err(|e| e.to_string())?,
        ))
        .stderr(Stdio::from(
            diagnostics.try_clone().map_err(|e| e.to_string())?,
        ));
    let child = command
        .spawn()
        .map_err(|e| format!("无法启动 Codex，请确认已安装 Codex CLI 并可从终端运行：{e}"))?;
    let mut child = RunningChild(child);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child
            .0
            .try_wait()
            .map_err(|e| format!("无法等待 Codex：{e}"))?
        {
            break status;
        }
        if start.elapsed() >= timeout {
            return Err("生成超过 10 分钟，已停止 Codex。请检查网络或缩短提示词后重试。".into());
        }
        thread::sleep(Duration::from_millis(100));
    };
    if !status.success() {
        return Err(format!(
            "Codex 执行失败（{status}）。请检查登录、网络和生图能力后重试。{}",
            diagnostic_tail(diagnostics)
        ));
    }
    read_image(workspace).map_err(|error| format!("{error}{}", diagnostic_tail(diagnostics)))
}

fn diagnostic_tail(mut file: File) -> String {
    let mut bytes = Vec::new();
    let length = file.metadata().map(|m| m.len()).unwrap_or(0);
    if file
        .seek(SeekFrom::Start(length.saturating_sub(2000)))
        .is_err()
        || file.take(2000).read_to_end(&mut bytes).is_err()
    {
        return String::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    if text.trim().is_empty() {
        String::new()
    } else {
        format!("\n{}", text.trim())
    }
}

fn read_image(workspace: &Path) -> Result<String, String> {
    let path = workspace.join(OUTPUT_NAME);
    let metadata = fs::symlink_metadata(&path).map_err(|_| {
        "Codex 未输出 result.png。请确认本地 Codex 已配置可用的生图工具或技能。".to_string()
    })?;
    if !metadata.file_type().is_file()
        || path.canonicalize().map_err(|e| e.to_string())?.parent()
            != Some(
                workspace
                    .canonicalize()
                    .map_err(|e| e.to_string())?
                    .as_path(),
            )
    {
        return Err("图片输出必须是生成目录中的普通文件。".into());
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err("图片超过 20 MB，请生成较小的图片。".into());
    }
    let invalid = || "Codex 输出的图片不是有效的 PNG，请重试。".to_string();
    let decoder = png::Decoder::new(Cursor::new(&bytes));
    let mut reader = decoder.read_info().map_err(|_| invalid())?;
    let size = reader
        .output_buffer_size()
        .filter(|size| *size <= 64 * 1024 * 1024)
        .ok_or_else(|| "图片分辨率过大，请生成较小的图片。".to_string())?;
    reader
        .next_frame(&mut vec![0; size])
        .map_err(|_| invalid())?;
    Ok(format!("data:image/png;base64,{}", STANDARD.encode(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = include_bytes!("../icons/icon.png");

    fn fixture_command(case: &str) -> Command {
        let mut command = Command::new(env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "generation::tests::fake_codex",
                "--ignored",
                "--nocapture",
            ])
            .env("HANDRAW_TEST_CASE", case);
        hide_window(&mut command);
        command
    }

    // Runs only as a child process of the integration tests; no account/network required.
    #[test]
    #[ignore]
    fn fake_codex() {
        let case = env::var("HANDRAW_TEST_CASE").unwrap();
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        fs::write("received.txt", input).unwrap();
        match case.as_str() {
            "success" => fs::write(OUTPUT_NAME, PNG).unwrap(),
            "failure" => {
                eprintln!("Please run codex login");
                std::process::exit(7);
            }
            "missing" => println!("Image tool unavailable"),
            "timeout" => thread::sleep(Duration::from_secs(30)),
            _ => panic!("unknown fixture"),
        }
    }

    #[test]
    fn validates_prompts_and_preserves_multiline_text() {
        for input in ["", " \n\t", "\u{3000}"] {
            assert!(validate_prompt(input).is_err());
        }
        assert!(validate_prompt(&"画".repeat(4001)).is_err());
        assert!(validate_prompt(&"画".repeat(4000)).is_ok());
        assert_eq!(validate_prompt("  小猫\n水彩  ").unwrap(), "小猫\n水彩");
    }

    #[test]
    fn serializes_generation_and_releases_guard_for_retry() {
        let first = GenerationGuard::acquire().unwrap();
        assert!(GenerationGuard::acquire().is_err());
        drop(first);
        assert!(GenerationGuard::acquire().is_ok());
    }

    #[test]
    fn process_receives_literal_prompt_and_returns_preview() {
        let workspace = tempfile::tempdir().unwrap();
        let prompt = "小猫\n\" & echo injected > injected.txt & $(echo bad) %PATH%";
        let result = run_generation(
            &mut fixture_command("success"),
            workspace.path(),
            prompt,
            Duration::from_secs(15),
        )
        .unwrap();
        assert_eq!(
            result,
            format!("data:image/png;base64,{}", STANDARD.encode(PNG))
        );
        let received = fs::read_to_string(workspace.path().join("received.txt")).unwrap();
        assert!(received.ends_with(prompt));
        assert!(!workspace.path().join("injected.txt").exists());
    }

    #[test]
    fn reports_nonzero_exit_and_missing_image_with_diagnostics() {
        for (case, expected) in [
            ("failure", "codex login"),
            ("missing", "Image tool unavailable"),
        ] {
            let workspace = tempfile::tempdir().unwrap();
            let error = run_generation(
                &mut fixture_command(case),
                workspace.path(),
                "小猫",
                Duration::from_secs(15),
            )
            .unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert!(error.contains(if case == "failure" {
                "执行失败"
            } else {
                "未输出"
            }));
        }
    }

    #[test]
    fn reports_missing_executable() {
        let workspace = tempfile::tempdir().unwrap();
        let mut command = Command::new(workspace.path().join("nonexistent-codex.exe"));
        let error = run_generation(&mut command, workspace.path(), "小猫", TIMEOUT).unwrap_err();
        assert!(error.contains("无法启动 Codex"));
    }

    #[test]
    fn times_out_and_reaps_the_child() {
        let workspace = tempfile::tempdir().unwrap();
        let start = Instant::now();
        let error = run_generation(
            &mut fixture_command("timeout"),
            workspace.path(),
            "小猫",
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert!(error.contains("已停止 Codex"));
        assert!(start.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn rejects_missing_corrupt_oversized_and_non_file_output() {
        let workspace = tempfile::tempdir().unwrap();
        assert!(read_image(workspace.path()).unwrap_err().contains("未输出"));
        let output = workspace.path().join(OUTPUT_NAME);
        fs::write(&output, b"not a PNG").unwrap();
        assert!(read_image(workspace.path())
            .unwrap_err()
            .contains("有效的 PNG"));
        fs::write(&output, &PNG[..PNG.len() / 2]).unwrap();
        assert!(read_image(workspace.path()).is_err());
        File::create(&output)
            .unwrap()
            .set_len(MAX_IMAGE_BYTES + 1)
            .unwrap();
        assert!(read_image(workspace.path()).unwrap_err().contains("20 MB"));
        fs::remove_file(&output).unwrap();
        fs::create_dir(&output).unwrap();
        assert!(read_image(workspace.path())
            .unwrap_err()
            .contains("普通文件"));
    }

    #[test]
    fn diagnostics_are_bounded() {
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(&vec![b'x'; 10_000]).unwrap();
        assert_eq!(diagnostic_tail(file).len(), 2001);
    }

    #[cfg(windows)]
    #[test]
    fn launches_windows_cmd_shim_in_path_with_spaces() {
        let workspace = tempfile::Builder::new()
            .prefix("handraw test ")
            .tempdir()
            .unwrap();
        let shim = workspace.path().join("codex.cmd");
        fs::write(&shim, format!(
            "@echo off\r\necho %* > args.txt\r\n\"{}\" --exact generation::tests::fake_codex --ignored --nocapture\r\nexit /b %errorlevel%\r\n",
            env::current_exe().unwrap().display()
        )).unwrap();
        let mut command = codex_command(&shim);
        command.env("HANDRAW_TEST_CASE", "success");
        let prompt = "水彩 & echo injected > injected.txt";
        assert!(run_generation(
            &mut command,
            workspace.path(),
            prompt,
            Duration::from_secs(15)
        )
        .is_ok());
        let args = fs::read_to_string(workspace.path().join("args.txt")).unwrap();
        assert!(args.contains("--ask-for-approval never exec --skip-git-repo-check --sandbox workspace-write --config sandbox_workspace_write.network_access=true --color never -"));
        assert!(fs::read_to_string(workspace.path().join("received.txt"))
            .unwrap()
            .ends_with(prompt));
        assert!(!workspace.path().join("injected.txt").exists());
    }
}
