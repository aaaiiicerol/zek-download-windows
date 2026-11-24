//! Validação de links, argumentos do yt-dlp, execução, leitura de progresso e erros.
//!
//! Segurança: o yt-dlp roda SEM shell, com lista de argumentos, ignorando arquivos de
//! configuração (`--ignore-config`) e com o link depois de `--` (nunca vira opção).
use crate::paths::{kill_tree, no_window};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const QUALITIES: [&str; 7] = ["best", "2160", "1440", "1080", "720", "480", "360"];
pub const AUDIO_FORMATS: [&str; 4] = ["mp3", "m4a", "opus", "flac"];
pub const BROWSERS: [&str; 8] = [
    "", "firefox", "chrome", "chromium", "brave", "edge", "opera", "vivaldi",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Video,
    Audio,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub mode: Mode,
    pub quality: String,
    pub audio_format: String,
    pub playlist: bool,
    pub folder: PathBuf,
    pub cookies_browser: String,
}

// ------------------------------------------------------------------ links

fn looks_like_domain(s: &str) -> bool {
    let first = s.split(|c: char| c == '/' || c == '?' || c == '#').next().unwrap_or("");
    first.contains('.')
        && !first.starts_with('.')
        && !first.ends_with('.')
        && first
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub fn validate_url(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    if s.is_empty() {
        return Err("Cole um link para começar.".into());
    }
    if s.chars().count() > 2048 {
        return Err("O link é grande demais.".into());
    }
    if s.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("O link contém espaços ou caracteres inválidos.".into());
    }
    let mut full = s.to_string();
    if !full.contains("://") && looks_like_domain(&full) {
        full = format!("https://{}", full);
    }
    let lower = full.to_lowercase();
    let rest: &str = if lower.starts_with("https://") {
        full.get(8..).unwrap_or("")
    } else if lower.starts_with("http://") {
        full.get(7..).unwrap_or("")
    } else {
        return Err("Use um link que comece com http:// ou https://.".into());
    };
    let end = rest
        .find(|c: char| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    let authority = &rest[..end];
    let hostport = authority.rsplit('@').next().unwrap_or("");
    let host = hostport.split(':').next().unwrap_or("");
    if host.is_empty() {
        return Err("O link está sem o endereço do site.".into());
    }
    Ok(full)
}

/// Aceita vários links separados por espaço/linha. Retorna (links válidos, erros).
pub fn extract_urls(text: &str) -> (Vec<String>, Vec<String>) {
    let mut urls: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    for token in text.split_whitespace() {
        match validate_url(token) {
            Ok(u) => {
                if !urls.contains(&u) {
                    urls.push(u);
                }
            }
            Err(e) => errors.push(e),
        }
    }
    if urls.is_empty() && errors.is_empty() {
        errors.push("Cole um link para começar.".into());
    }
    (urls, errors)
}

// ------------------------------------------------------------- argumentos

pub fn build_args(
    url: &str,
    o: &Options,
    js_runtimes: &[String],
    ffmpeg_dir: Option<&Path>,
) -> Result<Vec<String>, String> {
    if !QUALITIES.contains(&o.quality.as_str())
        || !AUDIO_FORMATS.contains(&o.audio_format.as_str())
        || !BROWSERS.contains(&o.cookies_browser.as_str())
    {
        return Err("Opções inválidas.".into());
    }
    let url = validate_url(url)?;
    let mut a: Vec<String> = [
        "--ignore-config",
        "--newline",
        "-q",
        "--progress",
        "--no-simulate",
        "--no-warnings",
        "--socket-timeout",
        "30",
        "--retries",
        "10",
        "--fragment-retries",
        "10",
        "--concurrent-fragments",
        "4",
        "--embed-metadata",
        "--no-mtime",
        "--progress-template",
        "download:FLX|%(progress.status)s|%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s|%(info.playlist_index)s|%(info.n_entries)s|%(info.title)s",
        "--progress-template",
        "postprocess:FLXPP|%(progress.status)s|%(progress.postprocessor)s",
        "--print",
        "after_move:FLXDONE|%(filepath)s",
        "-P",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.push(o.folder.to_string_lossy().to_string());
    if o.playlist {
        a.push("--yes-playlist".into());
        a.push("-o".into());
        a.push("%(playlist_title,uploader|Playlist).100B/%(playlist_index)02d - %(title).150B.%(ext)s".into());
    } else {
        a.push("--no-playlist".into());
        a.push("-o".into());
        a.push("%(title).150B.%(ext)s".into());
    }
    match o.mode {
        Mode::Audio => {
            for s in ["-f", "bestaudio/best", "-x", "--audio-format"] {
                a.push(s.into());
            }
            a.push(o.audio_format.clone());
            a.push("--audio-quality".into());
            a.push("0".into());
            if o.audio_format == "mp3" || o.audio_format == "m4a" {
                for s in ["--embed-thumbnail", "--convert-thumbnails", "jpg"] {
                    a.push(s.into());
                }
            }
        }
        Mode::Video => {
            a.push("-S".into());
            if o.quality == "best" {
                a.push("res,vcodec:h264,acodec:aac,ext".into());
            } else {
                a.push(format!("res:{},vcodec:h264,acodec:aac,ext", o.quality));
            }
        }
    }
    if !o.cookies_browser.is_empty() {
        a.push("--cookies-from-browser".into());
        a.push(o.cookies_browser.clone());
    }
    if let Some(dir) = ffmpeg_dir {
        a.push("--ffmpeg-location".into());
        a.push(dir.to_string_lossy().to_string());
    }
    for rt in js_runtimes {
        a.push("--js-runtimes".into());
        a.push(rt.clone());
    }
    a.push("--".into());
    a.push(url);
    Ok(a)
}

// -------------------------------------------------------------------- job

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Queued,
    Preparing,
    Downloading,
    Processing,
    Done,
    Error,
    Cancelled,
}

impl State {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            State::Queued | State::Preparing | State::Downloading | State::Processing
        )
    }
}

pub struct JobData {
    pub state: State,
    pub title: String,
    pub percent: f64,
    pub speed: Option<f64>,
    pub eta: Option<f64>,
    pub index: Option<u32>,
    pub count: Option<u32>,
    pub part: u32,
    pub last_bytes: f64,
    pub message: String,
    pub filepath: String,
    pub files_done: u32,
    pub heal_hint: bool,
    pub log: VecDeque<String>,
    pub cancelled: bool,
    pub pid: Option<u32>,
}

impl JobData {
    pub fn new() -> JobData {
        JobData {
            state: State::Queued,
            title: String::new(),
            percent: 0.0,
            speed: None,
            eta: None,
            index: None,
            count: None,
            part: 1,
            last_bytes: 0.0,
            message: String::new(),
            filepath: String::new(),
            files_done: 0,
            heal_hint: false,
            log: VecDeque::new(),
            cancelled: false,
            pid: None,
        }
    }

    pub fn push_log(&mut self, line: String) {
        if self.log.len() >= 500 {
            self.log.pop_front();
        }
        self.log.push_back(line);
    }

    /// Zera o progresso para uma nova tentativa (mantém o pedido de cancelamento limpo).
    pub fn reset_for_retry(&mut self) {
        *self = JobData::new();
    }

    pub fn log_text(&self) -> String {
        self.log.iter().cloned().collect::<Vec<String>>().join("\n")
    }
}

pub struct Job {
    pub id: u64,
    pub url: String,
    pub opts: Options,
    pub data: Mutex<JobData>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Cópia leve do estado do job, usada pela interface (sem segurar o lock).
#[derive(Clone)]
pub struct JobView {
    pub id: u64,
    pub url: String,
    pub mode: Mode,
    pub folder: PathBuf,
    pub state: State,
    pub title: String,
    pub percent: f64,
    pub status: String,
    pub filepath: String,
    pub log: String,
}

impl Job {
    pub fn new(url: String, opts: Options) -> Arc<Job> {
        Arc::new(Job {
            id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            url,
            opts,
            data: Mutex::new(JobData::new()),
        })
    }

    pub fn view(&self) -> JobView {
        let d = self.data.lock().unwrap();
        JobView {
            id: self.id,
            url: self.url.clone(),
            mode: self.opts.mode,
            folder: self.opts.folder.clone(),
            state: d.state,
            title: d.title.clone(),
            percent: d.percent,
            status: status_text(&d),
            filepath: d.filepath.clone(),
            log: d.log_text(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.data.lock().unwrap().state.is_active()
    }

    pub fn cancel(&self) {
        let pid = {
            let mut d = self.data.lock().unwrap();
            d.cancelled = true;
            if d.state == State::Queued {
                d.state = State::Cancelled;
            }
            d.pid
        };
        if let Some(pid) = pid {
            kill_tree(pid);
        }
    }
}

// ---------------------------------------------------------------- parsing

fn num(s: &str) -> Option<f64> {
    s.trim().parse::<f64>().ok()
}

fn pp_name(name: &str) -> &'static str {
    match name {
        "Merger" => "Juntando vídeo e áudio…",
        "VideoConvertor" => "Convertendo o vídeo…",
        "VideoRemuxer" => "Ajustando o formato do vídeo…",
        "ExtractAudio" => "Convertendo o áudio…",
        "EmbedThumbnail" => "Adicionando a capa…",
        "Metadata" | "FFmpegMetadata" => "Gravando título e artista…",
        "ThumbnailsConvertor" => "Preparando a capa…",
        "MoveFiles" => "Salvando o arquivo…",
        _ => "Finalizando o arquivo…",
    }
}

/// Atualiza o job a partir de uma linha do yt-dlp. Retorna true se algo mudou.
pub fn parse_line(d: &mut JobData, line: &str) -> bool {
    let line = line.trim_end_matches(&['\r', '\n'][..]);
    if line.is_empty() {
        return false;
    }
    if let Some(rest) = line.strip_prefix("FLX|") {
        let p: Vec<&str> = rest.splitn(9, '|').collect();
        if p.len() < 9 {
            return false;
        }
        let status = p[0];
        let done = num(p[1]);
        let total = num(p[2]).filter(|v| *v > 0.0).or_else(|| num(p[3]).filter(|v| *v > 0.0));
        if let Some(df) = done {
            if df + 1.0 < d.last_bytes {
                d.part += 1; // recomeçou: agora é o outro "pedaço" (ex.: áudio)
            }
        }
        d.last_bytes = done.unwrap_or(0.0);
        let title = p[8];
        if !title.is_empty() && title != "NA" {
            d.title = title.to_string();
        }
        d.index = p[6].trim().parse::<u32>().ok();
        d.count = p[7].trim().parse::<u32>().ok();
        if let (Some(a), Some(b)) = (done, total) {
            d.percent = (a / b * 100.0).clamp(0.0, 100.0);
        }
        d.speed = num(p[4]);
        d.eta = num(p[5]);
        d.state = State::Downloading;
        if status == "finished" {
            d.percent = 100.0;
            d.speed = None;
            d.eta = None;
        }
        return true;
    }
    if let Some(rest) = line.strip_prefix("FLXPP|") {
        let p: Vec<&str> = rest.splitn(2, '|').collect();
        if p.len() == 2 && p[0] == "started" {
            d.state = State::Processing;
            d.message = pp_name(p[1]).to_string();
            return true;
        }
        return false;
    }
    if let Some(rest) = line.strip_prefix("FLXDONE|") {
        d.filepath = rest.to_string();
        d.files_done += 1;
        return true;
    }
    d.push_log(line.to_string());
    false
}

// ------------------------------------------------------------------ erros

const ERROR_RULES: &[(&str, &str, bool)] = &[
    ("no such option", "O motor de download está desatualizado para esta versão do app.", true),
    ("ffmpeg not found", "O ffmpeg não foi encontrado. O Zek Download tenta baixá-lo sozinho; verifique a internet e tente de novo.", false),
    ("ffprobe not found", "O ffmpeg não foi encontrado. O Zek Download tenta baixá-lo sozinho; verifique a internet e tente de novo.", false),
    ("postprocessing: ffmpeg", "O ffmpeg falhou ou não está disponível. Tente de novo.", false),
    ("no space left", "O disco está cheio. Libere espaço ou escolha outra pasta.", false),
    ("not enough space", "O disco está cheio. Libere espaço ou escolha outra pasta.", false),
    ("permission denied", "Sem permissão para salvar nessa pasta. Escolha outra pasta.", false),
    ("sign in to confirm", "O site pediu verificação/login. Em Configurações, escolha o navegador onde você está logado.", false),
    ("confirm your age", "Conteúdo com restrição de idade. Em Configurações, escolha o navegador onde você está logado.", false),
    ("login required", "Este conteúdo exige login. Em Configurações, escolha o navegador onde você está logado.", false),
    ("private video", "Este vídeo é privado.", false),
    ("members-only", "Conteúdo exclusivo para membros/assinantes.", false),
    ("not available in your country", "Este conteúdo não está disponível na sua região.", false),
    ("geo restrict", "Este conteúdo não está disponível na sua região.", false),
    ("video unavailable", "O vídeo não está mais disponível.", false),
    ("has been removed", "O vídeo foi removido.", false),
    ("http error 429", "O site limitou os pedidos (muitos acessos). Aguarde alguns minutos e tente de novo.", false),
    ("getaddrinfo failed", "Sem internet. Verifique a conexão e tente de novo.", false),
    ("temporary failure in name resolution", "Sem internet. Verifique a conexão e tente de novo.", false),
    ("network is unreachable", "Sem internet. Verifique a conexão e tente de novo.", false),
    ("timed out", "A conexão demorou demais. Tente novamente.", false),
    ("unsupported url", "O site mudou ou este link não tem um vídeo compatível.", true),
    ("requested format is not available", "O formato pedido não está disponível (o site pode ter mudado).", true),
    ("http error 403", "O site bloqueou o acesso (provavelmente mudou de forma).", true),
    ("unable to extract", "Não consegui ler esta página (o site mudou).", true),
    ("nsig", "O YouTube mudou o sistema de proteção. Atualização necessária.", true),
    ("failed to extract", "Não consegui ler esta página (o site mudou).", true),
    ("javascript runtime", "Para o YouTube é preciso um interpretador JavaScript. O Zek Download tenta baixá-lo sozinho; verifique a internet e tente de novo.", false),
];

pub fn friendly_error(log_text: &str, code: Option<i32>) -> (String, bool) {
    let low = log_text.to_lowercase();
    for (needle, msg, heal) in ERROR_RULES {
        if low.contains(needle) {
            return (msg.to_string(), *heal);
        }
    }
    if let Some(last) = log_text.lines().filter(|l| l.starts_with("ERROR")).last() {
        let mut detail = last.trim_start_matches("ERROR:").trim().to_string();
        if detail.starts_with('[') {
            if let Some(pos) = detail.find(']') {
                detail = detail[pos + 1..].trim().to_string();
            }
        }
        let detail: String = detail.chars().take(160).collect();
        return (format!("Não foi possível baixar: {}", detail), true);
    }
    let code = code.map(|c| c.to_string()).unwrap_or_else(|| "?".into());
    (format!("O download terminou com erro (código {}).", code), false)
}

// ------------------------------------------------------------- formatação

pub fn human_size(n: f64) -> String {
    let units = ["B", "KB", "MB", "GB"];
    let mut v = n;
    let mut i = 0;
    while v >= 1024.0 && i < units.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{:.0} {}", v, units[i])
    } else {
        format!("{:.1} {}", v, units[i])
    }
}

pub fn fmt_eta(sec: f64) -> String {
    let sec = sec.max(0.0) as u64;
    let (h, rem) = (sec / 3600, sec % 3600);
    let (m, s) = (rem / 60, rem % 60);
    if h > 0 {
        format!("{}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}

pub fn status_text(d: &JobData) -> String {
    match d.state {
        State::Queued => "Na fila — começa em instantes…".to_string(),
        State::Preparing => {
            if d.message.is_empty() {
                "Preparando e lendo as informações do link…".to_string()
            } else {
                d.message.clone()
            }
        }
        State::Downloading => {
            let mut bits: Vec<String> = Vec::new();
            if let Some(n) = d.count {
                if n > 1 {
                    let idx = d.index.map(|i| i.to_string()).unwrap_or_else(|| "?".into());
                    bits.push(format!("Item {} de {}", idx, n));
                }
            }
            let mut pct = format!("Baixando {:.0}%", d.percent);
            if d.part > 1 {
                pct.push_str(&format!(" (parte {})", d.part));
            }
            bits.push(pct);
            if let Some(sp) = d.speed {
                if sp > 0.0 {
                    bits.push(format!("{}/s", human_size(sp)));
                }
            }
            if let Some(eta) = d.eta {
                bits.push(format!("faltam {}", fmt_eta(eta)));
            }
            bits.join(" · ")
        }
        State::Processing => {
            if d.message.is_empty() {
                "Finalizando o arquivo…".to_string()
            } else {
                d.message.clone()
            }
        }
        State::Done => {
            let name = Path::new(&d.filepath)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let extra = if d.files_done > 1 {
                format!(" ({} arquivos)", d.files_done)
            } else {
                String::new()
            };
            if !name.is_empty() && d.files_done <= 1 {
                format!("Concluído{} — {}", extra, name)
            } else {
                format!("Concluído{}", extra)
            }
        }
        State::Cancelled => "Cancelado. Se tentar de novo, o download continua de onde parou.".to_string(),
        State::Error => {
            if d.message.is_empty() {
                "Ocorreu um erro.".to_string()
            } else {
                d.message.clone()
            }
        }
    }
}

// --------------------------------------------------------------- execução

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunResult {
    Done,
    Error,
    Cancelled,
}

fn fail(job: &Arc<Job>, msg: &str) -> RunResult {
    let mut d = job.data.lock().unwrap();
    d.state = State::Error;
    d.message = msg.to_string();
    RunResult::Error
}

/// Roda o yt-dlp para o job e atualiza o progresso. `on_update` pede redesenho da tela.
pub fn run_job(
    job: &Arc<Job>,
    program: &Path,
    js_runtimes: &[String],
    ffmpeg_dir: Option<&Path>,
    on_update: &dyn Fn(),
) -> RunResult {
    let args = match build_args(&job.url, &job.opts, js_runtimes, ffmpeg_dir) {
        Ok(a) => a,
        Err(e) => return fail(job, &e),
    };
    if std::fs::create_dir_all(&job.opts.folder).is_err() {
        return fail(job, "Não consegui criar a pasta de destino. Escolha outra pasta.");
    }
    {
        let mut d = job.data.lock().unwrap();
        d.state = State::Preparing;
        d.message.clear();
    }
    on_update();

    let mut cmd = Command::new(program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("NO_COLOR", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1");
    no_window(&mut cmd);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return fail(job, "Não consegui iniciar o yt-dlp neste sistema."),
    };
    let pid = child.id();
    {
        let mut d = job.data.lock().unwrap();
        d.pid = Some(pid);
        if d.cancelled {
            kill_tree(pid);
        }
    }

    let stderr = child.stderr.take();
    let stdout = child.stdout.take();
    let job_err = Arc::clone(job);
    let err_thread = std::thread::spawn(move || {
        if let Some(se) = stderr {
            let mut reader = BufReader::new(se);
            let mut buf: Vec<u8> = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf).trim_end().to_string();
                        if !line.is_empty() {
                            job_err.data.lock().unwrap().push_log(line);
                        }
                    }
                }
            }
        }
    });

    let mut last_emit: Option<Instant> = None;
    if let Some(so) = stdout {
        let mut reader = BufReader::new(so);
        let mut buf: Vec<u8> = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let line = String::from_utf8_lossy(&buf).to_string();
                    let (changed, processing) = {
                        let mut d = job.data.lock().unwrap();
                        let c = parse_line(&mut d, &line);
                        (c, d.state == State::Processing)
                    };
                    if changed {
                        let due = match last_emit {
                            None => true,
                            Some(t) => t.elapsed() > Duration::from_millis(150),
                        };
                        if due || processing {
                            last_emit = Some(Instant::now());
                            on_update();
                        }
                    }
                }
            }
        }
    }
    let status = child.wait();
    let _ = err_thread.join();

    let mut d = job.data.lock().unwrap();
    d.pid = None;
    if d.cancelled {
        d.state = State::Cancelled;
        return RunResult::Cancelled;
    }
    let ok = status.as_ref().map(|s| s.success()).unwrap_or(false);
    if ok {
        d.state = State::Done;
        d.percent = 100.0;
        return RunResult::Done;
    }
    let code = status.ok().and_then(|s| s.code());
    let text = d.log_text();
    let (msg, heal) = friendly_error(&text, code);
    d.message = msg;
    d.heal_hint = heal;
    d.state = State::Error;
    RunResult::Error
}

// ------------------------------------------------------------------ testes

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options {
            mode: Mode::Video,
            quality: "1080".into(),
            audio_format: "mp3".into(),
            playlist: false,
            folder: PathBuf::from("C:\\Downloads"),
            cookies_browser: String::new(),
        }
    }

    #[test]
    fn urls_valid() {
        assert_eq!(validate_url(" https://youtu.be/abc ").unwrap(), "https://youtu.be/abc");
        assert_eq!(validate_url("youtube.com/watch?v=1").unwrap(), "https://youtube.com/watch?v=1");
    }

    #[test]
    fn urls_invalid() {
        for bad in [
            "", "   ", "--exec=rm -rf ~", "file:///etc/passwd", "javascript:alert(1)",
            "ftp://x.com/a", "https://", "http://a.com/ b", "-o /etc/x", "https://a.com/\u{0}",
            "C:\\Windows\\system32",
        ] {
            assert!(validate_url(bad).is_err(), "deveria rejeitar: {:?}", bad);
        }
    }

    #[test]
    fn urls_many() {
        let (urls, errs) = extract_urls("https://a.com/1\nhttps://a.com/1 https://b.com/2 lixo://x");
        assert_eq!(urls, vec!["https://a.com/1".to_string(), "https://b.com/2".to_string()]);
        assert_eq!(errs.len(), 1);
    }

    #[test]
    fn args_are_safe() {
        let a = build_args("https://a.com/v", &opts(), &["node:C:\\n\\node.exe".to_string()], None).unwrap();
        assert_eq!(a[a.len() - 2], "--");
        assert_eq!(a[a.len() - 1], "https://a.com/v");
        assert!(a.contains(&"--ignore-config".to_string()));
        assert!(!a.contains(&"--exec".to_string()));
        assert!(a.contains(&"--no-playlist".to_string()));
    }

    #[test]
    fn args_audio_and_bad_options() {
        let mut o = opts();
        o.mode = Mode::Audio;
        let a = build_args("https://a.com/v", &o, &[], None).unwrap();
        assert!(a.contains(&"-x".to_string()));
        assert!(a.contains(&"mp3".to_string()));
        o.quality = "9999".into();
        assert!(build_args("https://a.com/v", &o, &[], None).is_err());
        let mut o2 = opts();
        o2.cookies_browser = "--exec".into();
        assert!(build_args("https://a.com/v", &o2, &[], None).is_err());
    }

    #[test]
    fn progress_with_pipe_in_title() {
        let mut d = JobData::new();
        assert!(parse_line(&mut d, "FLX|downloading|2500|10000|NA|1048576|7|NA|NA|A | B\r\n"));
        assert_eq!(d.title, "A | B");
        assert!((d.percent - 25.0).abs() < 1e-9);
        let t = status_text(&d);
        assert!(t.contains("25%"), "{}", t);
        assert!(t.contains("1.0 MB/s"), "{}", t);
        parse_line(&mut d, "FLX|downloading|100|5000|NA|10|1|NA|NA|A | B");
        assert_eq!(d.part, 2);
    }

    #[test]
    fn progress_estimate_and_playlist() {
        let mut d = JobData::new();
        parse_line(&mut d, "FLX|downloading|500|NA|1000|NA|NA|3|12|T");
        assert_eq!((d.index, d.count), (Some(3), Some(12)));
        assert!((d.percent - 50.0).abs() < 1e-9);
        assert!(status_text(&d).contains("Item 3 de 12"));
    }

    #[test]
    fn postprocess_and_done() {
        let mut d = JobData::new();
        assert!(parse_line(&mut d, "FLXPP|started|Merger"));
        assert_eq!(d.state, State::Processing);
        assert!(parse_line(&mut d, "FLXDONE|C:\\Downloads\\x.mp4"));
        assert_eq!(d.files_done, 1);
    }

    #[test]
    fn garbage_ignored() {
        let mut d = JobData::new();
        assert!(!parse_line(&mut d, "FLX|quebrado"));
        assert!(!parse_line(&mut d, ""));
        assert!(!parse_line(&mut d, "linha qualquer"));
        assert_eq!(d.log.len(), 1);
    }

    #[test]
    fn errors_are_friendly() {
        assert_eq!(friendly_error("ERROR: [generic] Unsupported URL: x", Some(1)).1, true);
        assert_eq!(friendly_error("ERROR: ffmpeg not found", Some(1)).1, false);
        let (msg, heal) = friendly_error("ERROR: coisa estranha", Some(1));
        assert!(heal);
        assert!(msg.contains("coisa estranha"));
    }

    #[test]
    fn cancel_queued() {
        let j = Job::new("https://a.com".into(), opts());
        j.cancel();
        assert_eq!(j.data.lock().unwrap().state, State::Cancelled);
    }
}
