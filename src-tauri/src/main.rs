// Windows のリリース版で黒い画面（コンソール）を出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Linux（特に NVIDIA のグラフィック）で画面が真っ白・起動しない問題を避ける
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: ほかのスレッドを作る前に設定している
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
    record_to_cd_lib::run()
}
