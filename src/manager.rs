//! Fila de downloads, atualização automática e "autocura" do motor.
//! Não depende da interface: ela só passa uma função `repaint` para pedir redesenho.
use crate::downloader::{run_job, Job, Options, RunResult, State};
use crate::engine::{Engine, UpdateResult};
use crate::paths::now_secs;
use crate::settings::Settings;
use std::cell::Cell;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CHECK_INTERVAL_SECS: f64 = 6.0 * 3600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Busy,
    Ok,
    Err,
}

#[derive(Clone, Debug)]
pub struct EngineStatus {
    pub kind: Kind,
    pub text: String,
    /// Mensagem passageira (faixa colorida) visível até este instante.
    pub banner_until: Option<Instant>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Installed,
    Updated,
    Current,
    Failed,
}

pub struct Manager {
    pub settings: Arc<Mutex<Settings>>,
    pub engine: Arc<Engine>,
    pub jobs: Mutex<Vec<Arc<Job>>>,
    pub status: Mutex<EngineStatus>,
    tx: Mutex<Sender<Arc<Job>>>,
    repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Manager {
    pub fn new(
        settings: Arc<Mutex<Settings>>,
        engine: Arc<Engine>,
        repaint: Arc<dyn Fn() + Send + Sync>,
    ) -> Arc<Manager> {
        let (tx, rx) = channel::<Arc<Job>>();
        let mgr = Arc::new(Manager {
            settings,
            engine,
            jobs: Mutex::new(Vec::new()),
            status: Mutex::new(EngineStatus {
                kind: Kind::Busy,
                text: "Verificando o motor de download…".to_string(),
                banner_until: None,
            }),
            tx: Mutex::new(tx),
            repaint,
        });
        let worker = Arc::clone(&mgr);
        std::thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                let cancelled = {
                    let d = job.data.lock().unwrap();
                    d.cancelled || d.state == State::Cancelled
                };
                if cancelled {
                    job.data.lock().unwrap().state = State::Cancelled;
                } else {
                    worker.process(&job);
                }
                (worker.repaint)();
            }
        });
        mgr
    }

    // ------------------------------------------------------------ status
    fn set_status(&self, kind: Kind, text: &str, banner_secs: Option<u64>) {
        {
            let mut s = self.status.lock().unwrap();
            s.kind = kind;
            s.text = text.to_string();
            s.banner_until = banner_secs.map(|n| Instant::now() + Duration::from_secs(n));
        }
        (self.repaint)();
    }

    pub fn status_snapshot(&self) -> EngineStatus {
        self.status.lock().unwrap().clone()
    }

    fn progress_cb<'a>(&'a self, prefix: &'a str) -> impl Fn(u64, u64) + 'a {
        let last = Cell::new(101u64);
        move |done: u64, total: u64| {
            if total > 0 {
                let pct = done * 100 / total;
                if pct != last.get() {
                    last.set(pct);
                    self.set_status(Kind::Busy, &format!("{} {}%", prefix, pct), None);
                }
            }
        }
    }

    // ------------------------------------------------------------- motor
    fn update_engine(&self, reason: &str) -> Outcome {
        self.set_status(Kind::Busy, reason, None);
        let channel = self.settings.lock().unwrap().channel.clone();
        self.engine.set_channel(&channel);
        let progress = self.progress_cb("Baixando o motor de download…");
        match self.engine.check_and_update(&progress) {
            Err(e) => {
                self.set_status(Kind::Err, &e, Some(12));
                Outcome::Failed
            }
            Ok(res) => {
                {
                    let mut s = self.settings.lock().unwrap();
                    s.last_check = now_secs();
                    s.save();
                }
                let ver = self.engine.version(true).unwrap_or_else(|| "?".to_string());
                let (text, outcome) = match res {
                    UpdateResult::Installed => (format!("Motor de download instalado (yt-dlp {}).", ver), Outcome::Installed),
                    UpdateResult::Updated => (format!("Motor de download atualizado para {}.", ver), Outcome::Updated),
                    UpdateResult::Current => (format!("yt-dlp {}", ver), Outcome::Current),
                };
                let banner = if outcome == Outcome::Current { None } else { Some(6) };
                self.set_status(Kind::Ok, &text, banner);
                outcome
            }
        }
    }

    /// ffmpeg (juntar vídeo+áudio e converter para MP3) e Deno (YouTube). Falhas não são fatais.
    fn ensure_tools(&self) {
        if self.engine.ffmpeg_dir().is_none() {
            self.set_status(Kind::Busy, "Baixando o ffmpeg (só na primeira vez, pode demorar)…", None);
            let progress = self.progress_cb("Baixando o ffmpeg (só na primeira vez)…");
            if let Err(e) = self.engine.ensure_ffmpeg(&progress) {
                self.set_status(Kind::Err, &format!("ffmpeg: {}", e), Some(12));
                return;
            }
        }
        if self.engine.js_runtimes().is_empty() {
            self.set_status(Kind::Busy, "Baixando o interpretador do YouTube (só na primeira vez)…", None);
            let progress = self.progress_cb("Baixando o interpretador do YouTube…");
            if let Err(e) = self.engine.ensure_deno(&progress) {
                self.set_status(Kind::Err, &format!("Interpretador do YouTube: {}", e), Some(12));
                return;
            }
        }
        let ver = self.engine.cached_version().unwrap_or_else(|| "?".to_string());
        self.set_status(Kind::Ok, &format!("yt-dlp {}", ver), None);
    }

    /// Chamado ao abrir o app.
    pub fn start(self: &Arc<Self>) {
        let me = Arc::clone(self);
        std::thread::spawn(move || {
            let (auto, last) = {
                let s = me.settings.lock().unwrap();
                (s.auto_update, s.last_check)
            };
            if !me.engine.installed() {
                me.update_engine("Preparando o motor de download (só na primeira vez)…");
            } else {
                let ver = me.engine.version(true).unwrap_or_else(|| "?".to_string());
                me.set_status(Kind::Ok, &format!("yt-dlp {}", ver), None);
                if auto && now_secs() - last > CHECK_INTERVAL_SECS {
                    me.update_engine("Verificando atualizações do motor de download…");
                }
            }
            me.ensure_tools();
        });
    }

    pub fn update_now(self: &Arc<Self>) {
        let me = Arc::clone(self);
        std::thread::spawn(move || {
            me.update_engine("Verificando atualizações do motor de download…");
        });
    }

    // -------------------------------------------------------------- fila
    pub fn add(&self, url: String, opts: Options) -> Arc<Job> {
        let job = Job::new(url, opts);
        self.jobs.lock().unwrap().push(Arc::clone(&job));
        let _ = self.tx.lock().unwrap().send(Arc::clone(&job));
        (self.repaint)();
        job
    }

    pub fn retry(&self, job: &Arc<Job>) {
        {
            let mut d = job.data.lock().unwrap();
            if d.state.is_active() {
                return;
            }
            d.reset_for_retry();
        }
        let _ = self.tx.lock().unwrap().send(Arc::clone(job));
        (self.repaint)();
    }

    pub fn cancel(&self, job: &Arc<Job>) {
        job.cancel();
        (self.repaint)();
    }

    pub fn cancel_all(&self) {
        let jobs: Vec<Arc<Job>> = self.jobs.lock().unwrap().clone();
        for j in jobs {
            if j.is_active() {
                j.cancel();
            }
        }
    }

    pub fn has_active(&self) -> bool {
        let jobs: Vec<Arc<Job>> = self.jobs.lock().unwrap().clone();
        jobs.iter().any(|j| j.is_active())
    }

    pub fn clear_finished(&self) {
        let mut jobs = self.jobs.lock().unwrap();
        jobs.retain(|j| j.is_active());
    }

    // ------------------------------------------------------ processamento
    fn fail(&self, job: &Arc<Job>, msg: &str) {
        let mut d = job.data.lock().unwrap();
        d.state = State::Error;
        d.message = msg.to_string();
    }

    fn process(&self, job: &Arc<Job>) {
        {
            let mut d = job.data.lock().unwrap();
            d.state = State::Preparing;
            d.message.clear();
        }
        (self.repaint)();

        if self.engine.command().is_none() {
            job.data.lock().unwrap().message = "Baixando o motor de download pela primeira vez…".to_string();
            (self.repaint)();
            let out = self.update_engine("Preparando o motor de download…");
            if out == Outcome::Failed || self.engine.command().is_none() {
                self.fail(job, "Não consegui obter o motor de download. Verifique a internet e tente de novo.");
                return;
            }
        }
        if self.engine.ffmpeg_dir().is_none() || self.engine.js_runtimes().is_empty() {
            job.data.lock().unwrap().message = "Preparando as ferramentas (ffmpeg)… só na primeira vez.".to_string();
            (self.repaint)();
            self.ensure_tools();
        }

        let mut healed = false;
        let mut no_js = false;
        loop {
            let program = match self.engine.command() {
                Some(p) => p,
                None => {
                    self.fail(job, "O motor de download não está disponível.");
                    return;
                }
            };
            let js: Vec<String> = if no_js { Vec::new() } else { self.engine.js_runtimes() };
            let ffmpeg = self.engine.ffmpeg_dir();
            let repaint = Arc::clone(&self.repaint);
            let result = run_job(job, &program, &js, ffmpeg.as_deref(), &move || (repaint)());
            if result != RunResult::Error {
                break;
            }
            let (log_lower, heal_hint) = {
                let d = job.data.lock().unwrap();
                (d.log_text().to_lowercase(), d.heal_hint)
            };
            if !js.is_empty() && log_lower.contains("no such option") && log_lower.contains("js-runtimes") {
                no_js = true;
                job.data.lock().unwrap().log.clear();
                continue;
            }
            if heal_hint && !healed {
                // "Autocura": o site provavelmente mudou -> atualiza o motor e tenta de novo.
                healed = true;
                let original = {
                    let mut d = job.data.lock().unwrap();
                    let o = d.message.clone();
                    d.state = State::Preparing;
                    d.message = "O site parece ter mudado — atualizando o motor e tentando de novo…".to_string();
                    o
                };
                (self.repaint)();
                if self.update_engine("Um site mudou: procurando correção do yt-dlp…") == Outcome::Updated {
                    let mut d = job.data.lock().unwrap();
                    let was_cancelled = d.cancelled;
                    d.reset_for_retry();
                    d.cancelled = was_cancelled;
                    continue;
                }
                let stable = self.settings.lock().unwrap().channel == "stable";
                let mut d = job.data.lock().unwrap();
                d.state = State::Error;
                d.message = original;
                if stable {
                    d.message.push_str(" (O motor já está atualizado. Se persistir, ative o canal “Nightly” em Configurações.)");
                }
            }
            break;
        }
    }
}
