//! Work done off the UI thread: finding installs, listing releases, installing.
//!
//! Each task posts a [`Msg`] to the shared inbox and asks egui to repaint; the UI drains the inbox
//! every frame. Installs report their stage through a shared [`JobState`] so the progress bar
//! moves while they run.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::catalog::{AppDef, Catalog};
use crate::detect::{self, Installed};
use crate::install::{self, Outcome, Plan, Stage};
use crate::net::{self, Release};
use crate::platform::Platform;
use crate::selfupdate::{self, Method};
use crate::system;

pub enum Msg {
    Catalog(Catalog),
    Detected { app: String, installs: Vec<Installed>, running: bool },
    Releases { app: String, result: Result<Vec<Release>, String> },
    Finished { job: u64 },
    /// A message from the tray or from a second start of the launcher.
    Remote(crate::agent::Cmd),
    /// A newer launcher (or none) on GitHub.
    LauncherRelease(Result<Option<Release>, String>),
    /// The launcher's own update was put in place (or failed).
    LauncherApplied { version: String, result: Result<(), String> },
}

#[derive(Clone)]
pub struct Inbox {
    msgs: Arc<Mutex<Vec<Msg>>>,
    ctx: egui::Context,
}

impl Inbox {
    pub fn new(ctx: egui::Context) -> Self {
        Self { msgs: Arc::default(), ctx }
    }

    pub fn post(&self, msg: Msg) {
        if let Ok(mut m) = self.msgs.lock() {
            m.push(msg);
        }
        self.ctx.request_repaint();
    }

    pub fn drain(&self) -> Vec<Msg> {
        self.msgs.lock().map(|mut m| std::mem::take(&mut *m)).unwrap_or_default()
    }

    pub fn ctx(&self) -> &egui::Context {
        &self.ctx
    }
}

fn spawn(name: &str, f: impl FnOnce() + Send + 'static) {
    if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(f) {
        log::error!("couldn't start {name}: {e}");
    }
}

/// Find the copies of `app` here and whether one runs; also clears leftovers of earlier updates.
pub fn detect(inbox: &Inbox, app: AppDef, platform: Platform) {
    let inbox = inbox.clone();
    spawn("detect", move || {
        let installs = detect::detect(&app, &platform);
        install::clean_leftovers(&app, &installs);
        let running = installs.iter().any(|i| !system::running(&app, i).is_empty());
        inbox.post(Msg::Detected { app: app.id.clone(), installs, running });
    });
}

pub fn releases(inbox: &Inbox, app: AppDef) {
    let inbox = inbox.clone();
    spawn("releases", move || {
        let result = net::releases(&app.repo);
        inbox.post(Msg::Releases { app: app.id, result });
    });
}

pub fn catalog(inbox: &Inbox) {
    let inbox = inbox.clone();
    spawn("catalog", move || {
        if let Some(c) = Catalog::fetch() {
            inbox.post(Msg::Catalog(c));
        }
    });
}

/// Look for a newer launcher.
pub fn launcher_check(inbox: &Inbox, method: Method, platform: Platform, prereleases: bool) {
    let inbox = inbox.clone();
    spawn("self-check", move || inbox.post(Msg::LauncherRelease(selfupdate::check(&method, &platform, prereleases))));
}

/// Download and put in place the launcher `release`; `progress` follows the download (0..1).
pub fn launcher_apply(inbox: &Inbox, release: Release, method: Method, platform: Platform, progress: Arc<Mutex<Option<f32>>>) {
    let inbox = inbox.clone();
    spawn("self-update", move || {
        let ctx = inbox.ctx().clone();
        let stop = AtomicBool::new(false);
        let mut report = |t: net::Transfer| {
            if let Ok(mut p) = progress.lock() {
                *p = (t.total > 0).then(|| t.done as f32 / t.total as f32);
            }
            ctx.request_repaint();
        };
        let result = selfupdate::apply(&release, &method, &platform, &stop, &mut report);
        if let Err(e) = &result {
            log::error!("self-update to {}: {e}", release.version_label());
        }
        inbox.post(Msg::LauncherApplied { version: release.version_label(), result });
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Update,
    Uninstall,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Running(Stage),
    Done(Option<Outcome>),
    Failed(String),
    Cancelled,
}

pub struct JobState {
    pub status: Status,
}

pub struct Job {
    pub id: u64,
    pub app: String,
    pub app_name: String,
    pub action: Action,
    /// The version installed or removed.
    pub version: String,
    pub state: Arc<Mutex<JobState>>,
    pub stop: Arc<AtomicBool>,
}

impl Job {
    pub fn status(&self) -> Status {
        self.state.lock().map(|s| s.status.clone()).unwrap_or(Status::Failed("lost".into()))
    }

    pub fn is_running(&self) -> bool {
        matches!(self.status(), Status::Running(_))
    }

    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

static NEXT_JOB: AtomicU64 = AtomicU64::new(1);

fn new_job(app: &AppDef, action: Action, version: String) -> Job {
    Job {
        id: NEXT_JOB.fetch_add(1, Ordering::Relaxed),
        app: app.id.clone(),
        app_name: app.name.clone(),
        action,
        version,
        state: Arc::new(Mutex::new(JobState { status: Status::Running(Stage::Preparing) })),
        stop: Arc::default(),
    }
}

/// Install or update `app` to `release` by `plan`.
pub fn install(inbox: &Inbox, app: AppDef, release: Release, plan: Plan, existing: Option<Installed>) -> Job {
    let action = if existing.is_some() { Action::Update } else { Action::Install };
    let job = new_job(&app, action, release.version_label());
    let (state, stop, id, inbox) = (job.state.clone(), job.stop.clone(), job.id, inbox.clone());
    spawn("install", move || {
        let ctx = inbox.ctx().clone();
        let mut report = |stage: Stage| {
            if let Ok(mut s) = state.lock() {
                s.status = Status::Running(stage);
            }
            ctx.request_repaint();
        };
        let mut c = install::Ctx { stop: &stop, report: &mut report };
        let result = install::execute(&app, &release, &plan, existing.as_ref(), &mut c);
        let status = match result {
            Ok(outcome) => Status::Done(Some(outcome)),
            Err(_) if stop.load(Ordering::Relaxed) => Status::Cancelled,
            Err(e) => {
                log::error!("{} {}: {e}", app.name, release.version_label());
                Status::Failed(e)
            }
        };
        if let Ok(mut s) = state.lock() {
            s.status = status;
        }
        inbox.post(Msg::Finished { job: id });
    });
    job
}

pub fn uninstall(inbox: &Inbox, app: AppDef, installed: Installed) -> Job {
    let version = installed.version.as_ref().map(ToString::to_string).unwrap_or_default();
    let job = new_job(&app, Action::Uninstall, version);
    let (state, id, inbox) = (job.state.clone(), job.id, inbox.clone());
    if let Ok(mut s) = state.lock() {
        s.status = Status::Running(Stage::Installing);
    }
    spawn("uninstall", move || {
        let status = match install::uninstall(&app, &installed) {
            Ok(()) => Status::Done(None),
            Err(e) => Status::Failed(e),
        };
        if let Ok(mut s) = state.lock() {
            s.status = status;
        }
        inbox.post(Msg::Finished { job: id });
    });
    job
}
