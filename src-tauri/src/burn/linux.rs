use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{DiscStatus, parse_track_progress};

/// 使える書き込みソフトを探す（cdrskin を優先）
fn find_tool() -> Option<PathBuf> {
    let paths = std::env::var_os("PATH").unwrap_or_default();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&paths).collect();
    dirs.extend(["/usr/bin", "/usr/sbin", "/bin"].map(PathBuf::from));
    for tool in ["cdrskin", "wodim", "cdrecord"] {
        for dir in &dirs {
            let path = dir.join(tool);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

fn run(tool: &Path, args: &[&str]) -> String {
    match Command::new(tool).args(args).stdin(Stdio::null()).output() {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            text.push('\n');
            text.push_str(&String::from_utf8_lossy(&out.stderr));
            text
        }
        Err(_) => String::new(),
    }
}

fn find_drive(tool: &Path) -> Option<String> {
    let text = run(tool, &["--devices"]);
    for line in text.lines() {
        if let Some(start) = line.find("dev='") {
            let rest = &line[start + 5..];
            if let Some(end) = rest.find('\'') {
                return Some(rest[..end].to_string());
            }
        }
    }
    ["/dev/sr0", "/dev/cdrom", "/dev/sr1"]
        .into_iter()
        .find(|p| Path::new(p).exists())
        .map(String::from)
}

pub fn check_disc() -> DiscStatus {
    let Some(tool) = find_tool() else {
        return DiscStatus::NoTool;
    };
    let Some(drive) = find_drive(&tool) else {
        return DiscStatus::NoDrive;
    };
    let text = run(&tool, &[&format!("dev={drive}"), "-minfo"]).to_lowercase();
    parse_minfo(&text)
}

fn parse_minfo(text: &str) -> DiscStatus {
    let no_disc = [
        "no disc",
        "no media",
        "medium not present",
        "no disk",
        "cannot load media",
        "drive is empty",
    ];
    if no_disc.iter().any(|k| text.contains(k)) {
        return DiscStatus::NoDisc;
    }
    if text.contains("dvd") || text.contains("bd-r") {
        return DiscStatus::WrongMedia;
    }
    let status_line = text
        .lines()
        .find(|l| l.contains("disk status") || l.contains("disc status"));
    match status_line {
        Some(line) if line.contains("empty") || line.contains("blank") => DiscStatus::Ready,
        Some(_) => DiscStatus::NotBlank,
        None if text.contains("cd-r") => DiscStatus::Ready,
        None => DiscStatus::Unknown {
            message: "CDの状態を確かめられませんでした。CDを入れ直して、もう一度お試しください。"
                .into(),
        },
    }
}

pub fn burn(tracks: &[PathBuf], on_progress: &mut dyn FnMut(f64)) -> Result<(), String> {
    let tool = find_tool().ok_or(
        "CDを書き込むためのソフト（cdrskin）が見つかりません。アプリを入れ直してください。",
    )?;
    let drive = find_drive(&tool).ok_or("CDドライブが見つかりません。")?;
    let mut args: Vec<String> = vec![
        "-v".into(),
        format!("dev={drive}"),
        "gracetime=2".into(),
        "-eject".into(),
        "-pad".into(),
        "-audio".into(),
    ];
    args.extend(tracks.iter().map(|t| t.to_string_lossy().into_owned()));
    let mut child = Command::new(&tool)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("CDの書き込みを始められませんでした（{e}）"))?;

    // 標準エラーは別スレッドで集めておく（失敗したときの説明用）
    let mut stderr = child.stderr.take();
    let err_thread = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_string(&mut text);
        }
        text
    });
    if let Some(stdout) = child.stdout.take() {
        // 進み具合の行は \r で上書きされるので、\r でも区切る
        for chunk in BufReader::new(stdout).split(b'\r') {
            let Ok(chunk) = chunk else { break };
            for line in String::from_utf8_lossy(&chunk).lines() {
                if let Some(p) = parse_track_progress(line, tracks.len()) {
                    on_progress(p);
                }
            }
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("CDの書き込みに失敗しました（{e}）"))?;
    let err_text = err_thread.join().unwrap_or_default();
    if status.success() {
        on_progress(1.0);
        Ok(())
    } else {
        let last = err_text
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .to_string();
        Err(format!(
            "CDの書き込みに失敗しました。別の新しいCDでもう一度お試しください。（{last}）"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minfo_parsing() {
        assert_eq!(
            parse_minfo("mounted media type: cd-r\ndisk status: empty\n"),
            DiscStatus::Ready
        );
        assert_eq!(
            parse_minfo("mounted media type: cd-r\ndisk status: complete\n"),
            DiscStatus::NotBlank
        );
        assert_eq!(
            parse_minfo("cdrskin: fatal : no disc in drive"),
            DiscStatus::NoDisc
        );
        assert_eq!(
            parse_minfo("mounted media type: dvd+rw\ndisk status: empty"),
            DiscStatus::WrongMedia
        );
    }
}
