//! Windows 標準の IMAPI2 を使って音楽CDを書き込む。

use std::io::Write;
use std::path::{Path, PathBuf};

use windows::Win32::Storage::Imapi::{
    IDiscFormat2TrackAtOnce, IDiscMaster2, IDiscRecorder2, IMAPI_MEDIA_TYPE_CDR,
    IMAPI_MEDIA_TYPE_CDRW, MsftDiscFormat2TrackAtOnce, MsftDiscMaster2, MsftDiscRecorder2,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize, STGM_READ,
    STGM_SHARE_DENY_WRITE,
};
use windows::Win32::UI::Shell::SHCreateStreamOnFileEx;
use windows::core::{BSTR, HSTRING};

use super::DiscStatus;

struct ComGuard {
    initialized: bool,
}

impl ComGuard {
    fn new() -> Self {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        ComGuard { initialized }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

fn first_recorder() -> windows::core::Result<Option<IDiscRecorder2>> {
    unsafe {
        let master: IDiscMaster2 = CoCreateInstance(&MsftDiscMaster2, None, CLSCTX_ALL)?;
        if master.Count()? == 0 {
            return Ok(None);
        }
        let id = master.get_Item(0)?;
        let recorder: IDiscRecorder2 = CoCreateInstance(&MsftDiscRecorder2, None, CLSCTX_ALL)?;
        recorder.InitializeDiscRecorder(&id)?;
        Ok(Some(recorder))
    }
}

fn new_format(recorder: &IDiscRecorder2) -> windows::core::Result<IDiscFormat2TrackAtOnce> {
    unsafe {
        let format: IDiscFormat2TrackAtOnce =
            CoCreateInstance(&MsftDiscFormat2TrackAtOnce, None, CLSCTX_ALL)?;
        format.SetRecorder(recorder)?;
        format.SetClientName(&BSTR::from("RecordToCD"))?;
        Ok(format)
    }
}

pub fn check_disc() -> DiscStatus {
    let _com = ComGuard::new();
    let recorder = match first_recorder() {
        Ok(Some(r)) => r,
        Ok(None) => return DiscStatus::NoDrive,
        Err(e) => {
            return DiscStatus::Unknown {
                message: format!("CDドライブを確かめられませんでした（{e}）"),
            };
        }
    };
    let Ok(format) = new_format(&recorder) else {
        return DiscStatus::NoDrive;
    };
    unsafe {
        let supported = format
            .IsCurrentMediaSupported(&recorder)
            .map(|v| v.0 != 0)
            .unwrap_or(false);
        let media = format.CurrentPhysicalMediaType();
        let Ok(media) = media else {
            return DiscStatus::NoDisc;
        };
        if media != IMAPI_MEDIA_TYPE_CDR && media != IMAPI_MEDIA_TYPE_CDRW {
            return if media.0 == 0 {
                DiscStatus::NoDisc
            } else {
                DiscStatus::WrongMedia
            };
        }
        if !supported {
            return DiscStatus::NotBlank;
        }
        let blank = format
            .MediaHeuristicallyBlank()
            .map(|v| v.0 != 0)
            .unwrap_or(false);
        if blank {
            DiscStatus::Ready
        } else {
            DiscStatus::NotBlank
        }
    }
}

/// IMAPI2 はヘッダーなしの PCM を求めるので、WAV の中身だけを書き出す
fn write_raw_pcm(wav: &Path) -> Result<PathBuf, String> {
    let mut reader = hound::WavReader::open(wav)
        .map_err(|e| format!("曲のファイルを開けませんでした（{e}）"))?;
    let raw = wav.with_extension("pcm");
    let file = std::fs::File::create(&raw).map_err(|e| e.to_string())?;
    let mut out = std::io::BufWriter::new(file);
    let mut count = 0u64;
    for s in reader.samples::<i16>() {
        let s = s.map_err(|e| e.to_string())?;
        out.write_all(&s.to_le_bytes()).map_err(|e| e.to_string())?;
        count += 2;
    }
    // 2352 バイト単位に揃える
    let rest = (2352 - count % 2352) % 2352;
    out.write_all(&vec![0u8; rest as usize])
        .map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    Ok(raw)
}

fn write_tracks(
    format: &IDiscFormat2TrackAtOnce,
    recorder: &IDiscRecorder2,
    raws: &[PathBuf],
    on_progress: &mut dyn FnMut(f64),
) -> windows::core::Result<()> {
    unsafe {
        format.PrepareMedia()?;
        for (i, raw) in raws.iter().enumerate() {
            let stream = SHCreateStreamOnFileEx(
                &HSTRING::from(raw.as_os_str()),
                (STGM_READ | STGM_SHARE_DENY_WRITE).0,
                0,
                false,
                None,
            )?;
            format.AddAudioTrack(&stream)?;
            on_progress((i + 1) as f64 / raws.len() as f64);
        }
        format.ReleaseMedia()?;
        let _ = recorder.EjectMedia();
    }
    Ok(())
}

pub fn burn(tracks: &[PathBuf], on_progress: &mut dyn FnMut(f64)) -> Result<(), String> {
    let _com = ComGuard::new();
    let fail = |e: windows::core::Error| {
        format!("CDの書き込みに失敗しました。別の新しいCDでもう一度お試しください。（{e}）")
    };
    let recorder = first_recorder()
        .map_err(fail)?
        .ok_or("CDドライブが見つかりません。")?;
    let format = new_format(&recorder).map_err(fail)?;
    let raws: Vec<PathBuf> = tracks
        .iter()
        .map(|t| write_raw_pcm(t.as_path()))
        .collect::<Result<_, _>>()?;
    let result = write_tracks(&format, &recorder, &raws, on_progress);
    for raw in &raws {
        let _ = std::fs::remove_file(raw);
    }
    if result.is_err() {
        unsafe {
            let _ = format.ReleaseMedia();
        }
    }
    result.map_err(fail)
}
