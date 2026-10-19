use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::{DiscStatus, parse_percent};

fn drutil(args: &[&str]) -> Option<String> {
    let out = Command::new("/usr/bin/drutil")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Some(text)
}

pub fn check_disc() -> DiscStatus {
    match drutil(&["status"]) {
        Some(text) => parse_status(&text),
        None => DiscStatus::NoDrive,
    }
}

fn parse_status(text: &str) -> DiscStatus {
    let lower = text.to_lowercase();
    if lower.trim().is_empty() || lower.contains("no drives") || !lower.contains("type:") {
        return DiscStatus::NoDrive;
    }
    if lower.contains("no media") {
        return DiscStatus::NoDisc;
    }
    let media = lower
        .lines()
        .find_map(|l| l.split_once("type:").map(|(_, t)| t.trim().to_string()))
        .unwrap_or_default();
    if !media.starts_with("cd") {
        return DiscStatus::WrongMedia;
    }
    let blank = lower
        .lines()
        .find(|l| l.contains("writability"))
        .is_some_and(|l| l.contains("blank"));
    if blank {
        DiscStatus::Ready
    } else {
        DiscStatus::NotBlank
    }
}

pub fn burn(tracks: &[PathBuf], on_progress: &mut dyn FnMut(f64)) -> Result<(), String> {
    // drutil は「フォルダの中の音声ファイル」を名前順に書き込む
    let folder = tracks
        .first()
        .and_then(|t| t.parent())
        .ok_or("書き込む曲がありません。")?;
    let mut child = Command::new("/usr/bin/drutil")
        .args(["burn", "-audio", "-noverify", "-eject"])
        .arg(folder)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("CDの書き込みを始められませんでした（{e}）"))?;
    let mut last_line = String::new();
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for chunk in reader.split(b'\r') {
            let Ok(chunk) = chunk else { break };
            let text = String::from_utf8_lossy(&chunk);
            for line in text.lines() {
                if let Some(p) = parse_percent(line) {
                    on_progress(p);
                }
                if !line.trim().is_empty() {
                    last_line = line.trim().to_string();
                }
            }
        }
    }
    let status = child
        .wait()
        .map_err(|e| format!("CDの書き込みに失敗しました（{e}）"))?;
    if status.success() && !last_line.to_lowercase().contains("fail") {
        on_progress(1.0);
        Ok(())
    } else {
        Err(format!(
            "CDの書き込みに失敗しました。別の新しいCDでもう一度お試しください。（{last_line}）"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_parsing() {
        let blank = " Vendor   Product           Rev\n MATSHITA DVD-R   UJ-85J    FM0S\n\n           Type: CD-R                 Name: /dev/disk2\n    Writability: appendable, blank, overwritable\n";
        assert_eq!(parse_status(blank), DiscStatus::Ready);
        let none = " Vendor   Product\n MATSHITA DVD-R\n           Type: No Media Inserted\n";
        assert_eq!(parse_status(none), DiscStatus::NoDisc);
        assert_eq!(parse_status(""), DiscStatus::NoDrive);
    }
}
