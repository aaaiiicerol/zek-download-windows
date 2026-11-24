//! Pastas do app, modo portátil e utilitários de processo.
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Modo portátil: existe um arquivo `portable.txt` ao lado do .exe.
pub fn is_portable() -> bool {
    exe_dir().join("portable.txt").is_file()
}

pub fn data_dir() -> PathBuf {
    if is_portable() {
        exe_dir().join("data")
    } else {
        dirs::data_local_dir().unwrap_or_else(exe_dir).join("ZekDownload")
    }
}

pub fn config_dir() -> PathBuf {
    if is_portable() {
        exe_dir().join("data")
    } else {
        dirs::config_dir().unwrap_or_else(exe_dir).join("ZekDownload")
    }
}

pub fn default_download_dir() -> PathBuf {
    dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Procura um executável no PATH.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// Evita que uma janela de console pisque ao rodar processos filhos no Windows.
pub fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

/// Encerra o processo e todos os filhos (o yt-dlp.exe "onefile" cria um processo filho).
pub fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        let pid_s = pid.to_string();
        let mut c = Command::new("taskkill");
        c.args(["/PID", pid_s.as_str(), "/T", "/F"]);
        no_window(&mut c);
        let _ = c.spawn();
    }
    #[cfg(not(windows))]
    {
        let pid_s = pid.to_string();
        let _ = Command::new("kill").args(["-TERM", pid_s.as_str()]).spawn();
    }
}

/// Abre um arquivo (programa padrão) ou uma pasta no Explorer.
pub fn open_path(p: &Path) {
    #[cfg(windows)]
    {
        let mut c = Command::new("explorer.exe");
        c.arg(p);
        no_window(&mut c);
        let _ = c.spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("xdg-open").arg(p).spawn();
    }
}

/// Mostra o arquivo selecionado dentro da pasta.
pub fn reveal_in_folder(p: &Path) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("explorer.exe");
        c.raw_arg(format!("/select,\"{}\"", p.display()));
        no_window(&mut c);
        let _ = c.spawn();
    }
    #[cfg(not(windows))]
    {
        if let Some(parent) = p.parent() {
            open_path(parent);
        }
    }
}

pub fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
