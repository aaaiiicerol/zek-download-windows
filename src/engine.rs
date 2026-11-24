//! Gerenciador do motor de download (yt-dlp) e das ferramentas auxiliares (ffmpeg, Deno).
//!
//! O Zek Download NÃO embute o yt-dlp. Ele baixa o executável oficial das releases do
//! yt-dlp no GitHub, confere o SHA-256 contra o arquivo SHA2-256SUMS publicado na mesma
//! release e guarda numa pasta do usuário (sem precisar de administrador). Quando os sites
//! mudam e o yt-dlp lança correção, o app se atualiza sozinho.
use crate::paths::{exe_dir, find_in_path, no_window};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

const MAX_SUMS_BYTES: u64 = 256 * 1024;
const MAX_BINARY_BYTES: u64 = 250 * 1024 * 1024;
const MAX_ZIP_BYTES: u64 = 700 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateResult {
    Installed,
    Updated,
    Current,
}

// ------------------------------------------------------------------- rede

/// Camada de rede (trocável nos testes).
pub trait Http: Send + Sync {
    fn get_bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String>;
    /// Baixa para `dest` e devolve o SHA-256 (hex minúsculo) do que foi gravado.
    fn download(
        &self,
        url: &str,
        dest: &Path,
        limit: u64,
        progress: &dyn Fn(u64, u64),
    ) -> Result<String, String>;
}

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new() -> UreqHttp {
        let agent = ureq::AgentBuilder::new()
            .https_only(true)
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(60))
            .user_agent(concat!("ZekDownload/", env!("CARGO_PKG_VERSION")))
            .build();
        UreqHttp { agent }
    }

    fn open(&self, url: &str) -> Result<ureq::Response, String> {
        if !url.to_lowercase().starts_with("https://") {
            return Err("Endereço de atualização inválido.".into());
        }
        match self.agent.get(url).call() {
            Ok(r) => Ok(r),
            Err(ureq::Error::Status(code, _)) => {
                Err(format!("O servidor de atualização respondeu com erro {}.", code))
            }
            Err(_) => Err("Sem conexão com a internet (não consegui falar com o GitHub).".into()),
        }
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

impl Http for UreqHttp {
    fn get_bytes(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        let resp = self.open(url)?;
        let mut reader = resp.into_reader().take(limit + 1);
        let mut data: Vec<u8> = Vec::new();
        reader
            .read_to_end(&mut data)
            .map_err(|_| "A conexão caiu durante a verificação de atualização.".to_string())?;
        if data.len() as u64 > limit {
            return Err("Resposta inesperadamente grande do servidor de atualização.".into());
        }
        Ok(data)
    }

    fn download(
        &self,
        url: &str,
        dest: &Path,
        limit: u64,
        progress: &dyn Fn(u64, u64),
    ) -> Result<String, String> {
        let resp = self.open(url)?;
        let total = resp
            .header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let mut reader = resp.into_reader();
        let mut out = fs::File::create(dest)
            .map_err(|e| format!("Não consegui gravar o arquivo temporário: {}", e))?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        let mut done: u64 = 0;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|_| "A conexão caiu durante o download da atualização.".to_string())?;
            if n == 0 {
                break;
            }
            done += n as u64;
            if done > limit {
                return Err("Arquivo maior do que o esperado; download abortado.".into());
            }
            hasher.update(&buf[..n]);
            out.write_all(&buf[..n])
                .map_err(|e| format!("Erro ao gravar o arquivo: {}", e))?;
            progress(done, total);
        }
        let _ = out.flush();
        Ok(hex(hasher.finalize().as_slice()))
    }
}

// ----------------------------------------------------------------- helpers

/// Interpreta o conteúdo de um arquivo de checksums -> {arquivo: sha256}.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    let mut sums = HashMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 2 && is_sha256(parts[0]) {
            sums.insert(parts[1].trim_start_matches('*').to_string(), parts[0].to_lowercase());
        }
    }
    sums
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(hasher.finalize().as_slice()))
}

/// "2025.01.01" ou "2025.01.01.123456" etc.
pub fn looks_like_version(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() >= 10
        && b[..4].iter().all(|c| c.is_ascii_digit())
        && b[4] == b'.'
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[7] == b'.'
        && b[8..10].iter().all(|c| c.is_ascii_digit())
}

fn run_version(path: &Path) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.arg("--version").stdin(Stdio::null());
    no_window(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

fn primary_asset() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "yt-dlp_arm64.exe"
    } else if cfg!(target_arch = "x86") {
        "yt-dlp_x86.exe"
    } else {
        "yt-dlp.exe"
    }
}

fn ffmpeg_asset() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "ffmpeg-master-latest-winarm64-gpl.zip"
    } else {
        "ffmpeg-master-latest-win64-gpl.zip"
    }
}

fn repo_for(channel: &str) -> &'static str {
    if channel == "nightly" {
        "yt-dlp/yt-dlp-nightly-builds"
    } else {
        "yt-dlp/yt-dlp"
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Meta {
    asset: String,
    channel: String,
    sha256: String,
    version: String,
}

// ------------------------------------------------------------------ motor

pub struct Engine {
    dir: PathBuf,
    http: Box<dyn Http>,
    lock: Mutex<()>,
    version: Mutex<Option<String>>,
    channel: Mutex<String>,
}

impl Engine {
    pub fn new(data_dir: &Path, channel: &str, http: Box<dyn Http>) -> Engine {
        Engine {
            dir: data_dir.join("bin"),
            http,
            lock: Mutex::new(()),
            version: Mutex::new(None),
            channel: Mutex::new(channel.to_string()),
        }
    }

    pub fn set_channel(&self, channel: &str) {
        *self.channel.lock().unwrap() = channel.to_string();
    }

    fn channel(&self) -> String {
        self.channel.lock().unwrap().clone()
    }

    fn bin_path(&self) -> PathBuf {
        self.dir.join("yt-dlp.exe")
    }

    fn meta_path(&self) -> PathBuf {
        self.dir.join("engine.json")
    }

    fn read_meta(&self) -> Option<Meta> {
        fs::read_to_string(self.meta_path())
            .ok()
            .and_then(|t| serde_json::from_str::<Meta>(&t).ok())
    }

    /// Caminho do yt-dlp a usar (o gerenciado; senão um que já esteja no PATH).
    pub fn command(&self) -> Option<PathBuf> {
        let p = self.bin_path();
        if p.is_file() {
            Some(p)
        } else {
            find_in_path("yt-dlp.exe")
        }
    }

    pub fn installed(&self) -> bool {
        self.bin_path().is_file()
    }

    pub fn cached_version(&self) -> Option<String> {
        self.version.lock().unwrap().clone()
    }

    pub fn version(&self, refresh: bool) -> Option<String> {
        if !refresh {
            if let Some(v) = self.cached_version() {
                return Some(v);
            }
        }
        let cmd = self.command()?;
        let v = run_version(&cmd);
        *self.version.lock().unwrap() = v.clone();
        v
    }

    fn install_asset(
        &self,
        base: &str,
        asset: &str,
        expected: &str,
        progress: &dyn Fn(u64, u64),
    ) -> Result<(), String> {
        fs::create_dir_all(&self.dir).map_err(|e| format!("Não consegui criar a pasta do motor: {}", e))?;
        let tmp = self.dir.join("yt-dlp.new.exe");
        let _ = fs::remove_file(&tmp);
        let result = (|| -> Result<String, String> {
            let got = self
                .http
                .download(&format!("{}{}", base, asset), &tmp, MAX_BINARY_BYTES, progress)?;
            if got != expected {
                return Err("Falha de segurança: o SHA-256 do yt-dlp baixado não confere. A atualização foi descartada.".into());
            }
            let ver = run_version(&tmp)
                .filter(|v| looks_like_version(v))
                .ok_or_else(|| "O yt-dlp baixado não passou no teste de execução.".to_string())?;
            fs::rename(&tmp, self.bin_path()).map_err(|_| {
                "Não consegui substituir o yt-dlp (ele está em uso?). Aguarde os downloads terminarem e tente de novo.".to_string()
            })?;
            Ok(ver)
        })();
        let _ = fs::remove_file(&tmp);
        let ver = result?;
        let meta = Meta {
            asset: asset.to_string(),
            channel: self.channel(),
            sha256: expected.to_string(),
            version: ver.clone(),
        };
        if let Ok(text) = serde_json::to_string(&meta) {
            let _ = fs::write(self.meta_path(), text);
        }
        *self.version.lock().unwrap() = Some(ver);
        Ok(())
    }

    /// Confere a release mais recente e instala/atualiza se for preciso.
    pub fn check_and_update(&self, progress: &dyn Fn(u64, u64)) -> Result<UpdateResult, String> {
        let _guard = self.lock.lock().unwrap();
        let channel = self.channel();
        let base = format!(
            "https://github.com/{}/releases/latest/download/",
            repo_for(&channel)
        );
        let sums_raw = self
            .http
            .get_bytes(&format!("{}SHA2-256SUMS", base), MAX_SUMS_BYTES)?;
        let sums = parse_sums(&String::from_utf8_lossy(&sums_raw));
        if sums.is_empty() {
            return Err("Lista de verificação (SHA2-256SUMS) vazia ou inválida.".into());
        }
        let installed = self.bin_path().is_file();
        if installed {
            if let Some(meta) = self.read_meta() {
                if meta.channel == channel {
                    if let Some(expected) = sums.get(&meta.asset) {
                        if let Ok(actual) = sha256_file(&self.bin_path()) {
                            if &actual == expected {
                                return Ok(UpdateResult::Current);
                            }
                        }
                    }
                }
            }
        }
        let mut candidates: Vec<&str> = vec![primary_asset()];
        if primary_asset() != "yt-dlp.exe" {
            candidates.push("yt-dlp.exe");
        }
        let mut last_err = "Nenhum arquivo compatível com este sistema na release do yt-dlp.".to_string();
        for asset in candidates {
            if let Some(expected) = sums.get(asset) {
                match self.install_asset(&base, asset, expected, progress) {
                    Ok(()) => {
                        return Ok(if installed {
                            UpdateResult::Updated
                        } else {
                            UpdateResult::Installed
                        })
                    }
                    Err(e) => last_err = e,
                }
            }
        }
        Err(last_err)
    }

    // ------------------------------------------------- ferramentas auxiliares

    /// Pasta onde está o ffmpeg.exe (ao lado do app, gerenciada ou no PATH).
    pub fn ffmpeg_dir(&self) -> Option<PathBuf> {
        let candidates = [exe_dir().join("ffmpeg"), self.dir.join("ffmpeg")];
        for c in candidates.iter() {
            if c.join("ffmpeg.exe").is_file() {
                return Some(c.clone());
            }
        }
        find_in_path("ffmpeg.exe").and_then(|p| p.parent().map(|d| d.to_path_buf()))
    }

    pub fn deno_path(&self) -> Option<PathBuf> {
        let p = self.dir.join("deno").join("deno.exe");
        if p.is_file() {
            Some(p)
        } else {
            find_in_path("deno.exe")
        }
    }

    /// Interpretadores JavaScript disponíveis (o YouTube exige um no yt-dlp).
    pub fn js_runtimes(&self) -> Vec<String> {
        let mut v: Vec<String> = Vec::new();
        if let Some(p) = self.deno_path() {
            v.push(format!("deno:{}", p.display()));
        }
        if let Some(p) = find_in_path("node.exe") {
            v.push(format!("node:{}", p.display()));
        }
        if let Some(p) = find_in_path("bun.exe") {
            v.push(format!("bun:{}", p.display()));
        }
        v
    }

    fn install_zip(
        &self,
        zip_url: &str,
        expected: &str,
        subdir: &str,
        wanted: &[&str],
        progress: &dyn Fn(u64, u64),
    ) -> Result<(), String> {
        let target = self.dir.join(subdir);
        fs::create_dir_all(&target).map_err(|e| format!("Não consegui criar a pasta: {}", e))?;
        let tmp_zip = self.dir.join(format!(".{}.zip", subdir));
        let got = self.http.download(zip_url, &tmp_zip, MAX_ZIP_BYTES, progress);
        let result = match got {
            Ok(h) if h == expected => extract_zip(&tmp_zip, &target, wanted),
            Ok(_) => Err("Falha de segurança: o SHA-256 do arquivo baixado não confere. Instalação descartada.".to_string()),
            Err(e) => Err(e),
        };
        let _ = fs::remove_file(&tmp_zip);
        result
    }

    /// Baixa o ffmpeg (build oficial usada pelo yt-dlp) se ainda não existir.
    pub fn ensure_ffmpeg(&self, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap();
        if self.ffmpeg_dir().is_some() {
            return Ok(());
        }
        let base = "https://github.com/yt-dlp/FFmpeg-Builds/releases/latest/download/";
        let sums_raw = self
            .http
            .get_bytes(&format!("{}checksums.sha256", base), MAX_SUMS_BYTES)?;
        let sums = parse_sums(&String::from_utf8_lossy(&sums_raw));
        let expected = sums
            .get(ffmpeg_asset())
            .ok_or_else(|| "Não achei o ffmpeg na lista de verificação.".to_string())?;
        self.install_zip(
            &format!("{}{}", base, ffmpeg_asset()),
            expected,
            "ffmpeg",
            &["/bin/ffmpeg.exe", "/bin/ffprobe.exe"],
            progress,
        )
    }

    /// Baixa o Deno (interpretador JS usado para o YouTube) se não houver nenhum.
    pub fn ensure_deno(&self, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
        let _guard = self.lock.lock().unwrap();
        if !self.js_runtimes().is_empty() || !cfg!(target_arch = "x86_64") {
            return Ok(());
        }
        let zip_url = "https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip";
        let raw = self
            .http
            .get_bytes(&format!("{}.sha256sum", zip_url), MAX_SUMS_BYTES)?;
        let text = String::from_utf8_lossy(&raw).to_string();
        let expected = text
            .split_whitespace()
            .find(|t| is_sha256(t))
            .map(|t| t.to_lowercase())
            .ok_or_else(|| "Lista de verificação do Deno inválida.".to_string())?;
        self.install_zip(zip_url, &expected, "deno", &["deno.exe"], progress)
    }
}

fn extract_zip(zip_path: &Path, target: &Path, wanted: &[&str]) -> Result<(), String> {
    let file = fs::File::open(zip_path).map_err(|e| format!("Não consegui abrir o arquivo baixado: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| "O arquivo ZIP baixado é inválido.".to_string())?;
    let mut found = 0usize;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|_| "O arquivo ZIP baixado é inválido.".to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().replace('\\', "/");
        for w in wanted {
            if name.ends_with(*w) {
                // Só gravamos com nomes fixos (nunca com o caminho de dentro do ZIP).
                let file_name = Path::new(*w)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let part = target.join(format!("{}.part", file_name));
                {
                    let mut out = fs::File::create(&part)
                        .map_err(|e| format!("Não consegui gravar {}: {}", file_name, e))?;
                    std::io::copy(&mut entry, &mut out)
                        .map_err(|e| format!("Erro ao extrair {}: {}", file_name, e))?;
                }
                fs::rename(&part, target.join(&file_name))
                    .map_err(|e| format!("Não consegui finalizar {}: {}", file_name, e))?;
                found += 1;
                break;
            }
        }
    }
    if found < wanted.len() {
        return Err("O arquivo baixado não tem todos os programas esperados.".into());
    }
    Ok(())
}

// ------------------------------------------------------------------ testes

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    /// "GitHub" falso: serve SHA2-256SUMS e um binário.
    struct FakeHttp {
        sums: String,
        binary: Vec<u8>,
        asked: StdMutex<Vec<String>>,
    }

    impl Http for FakeHttp {
        fn get_bytes(&self, url: &str, _limit: u64) -> Result<Vec<u8>, String> {
            self.asked.lock().unwrap().push(url.to_string());
            if url.ends_with("SHA2-256SUMS") {
                Ok(self.sums.clone().into_bytes())
            } else {
                Err("404".into())
            }
        }
        fn download(&self, url: &str, dest: &Path, _l: u64, _p: &dyn Fn(u64, u64)) -> Result<String, String> {
            self.asked.lock().unwrap().push(url.to_string());
            fs::write(dest, &self.binary).map_err(|e| e.to_string())?;
            Ok(hex(Sha256::digest(&self.binary).as_slice()))
        }
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("zekdl-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sums_parse() {
        let h = "a".repeat(64);
        let m = parse_sums(&format!("{h}  yt-dlp.exe\n{h} *yt-dlp_x86.exe\nlixo\n"));
        assert_eq!(m.len(), 2);
        assert!(m.contains_key("yt-dlp_x86.exe"));
    }

    #[test]
    fn version_shape() {
        assert!(looks_like_version("2025.01.01"));
        assert!(looks_like_version("2025.11.12.232815"));
        assert!(!looks_like_version("hello"));
        assert!(!looks_like_version("2025-01-01"));
    }

    #[test]
    fn tampered_binary_is_rejected_and_not_installed() {
        let dir = tmpdir("tamper");
        let good = b"binario-bom".to_vec();
        let bad_hash = hex(Sha256::digest(b"outro-conteudo").as_slice());
        let sums = format!("{}  {}\n", bad_hash, primary_asset()); // hash publicado != hash do que foi baixado
        let http = FakeHttp { sums, binary: good, asked: StdMutex::new(vec![]) };
        let engine = Engine::new(&dir, "stable", Box::new(http));
        let err = engine.check_and_update(&|_, _| {}).unwrap_err();
        assert!(err.contains("SHA-256"), "{}", err);
        assert!(!engine.installed());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_sums_rejected() {
        let dir = tmpdir("empty");
        let http = FakeHttp { sums: "nada".into(), binary: vec![], asked: StdMutex::new(vec![]) };
        let engine = Engine::new(&dir, "stable", Box::new(http));
        assert!(engine.check_and_update(&|_, _| {}).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn http_only_urls_are_refused() {
        let h = UreqHttp::new();
        assert!(h.get_bytes("http://github.com/x", 10).is_err());
        assert!(h.get_bytes("file:///C:/Windows/win.ini", 10).is_err());
    }

    #[test]
    fn nightly_uses_nightly_repo() {
        assert_eq!(repo_for("nightly"), "yt-dlp/yt-dlp-nightly-builds");
        assert_eq!(repo_for("stable"), "yt-dlp/yt-dlp");
        assert_eq!(repo_for("qualquer"), "yt-dlp/yt-dlp");
    }
}
