//! Ventana: a la izquierda el candado grande y «Bloquear todo»; a la derecha
//! la lista de apps con un interruptor cada una. Mismo cristal que Encaje.
//!
//! adb se usa en un hilo aparte; la ventana solo recibe mensajes. Cuando un
//! celular se conecta, se mira su estado y, si falta un candado que querías
//! (lo quitaste desde la Play Store, por ejemplo), se vuelve a poner solo.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;
use gtk::prelude::*;
use gtk::{cairo, gdk, glib};
use gtk4 as gtk;

use crate::catalog::{self, App};
use crate::device::{self, Device, PkgState};
use crate::store::Store;

const WIN_W: i32 = 780;
const WIN_H: i32 = 480;
const TEAL: (f64, f64, f64) = (0.0, 0.765, 0.89);
const RED: (f64, f64, f64) = (1.0, 0.27, 0.33);

const CSS: &str = r#"
window.cerrojo, window.cerrojo label { font-family: "Inter", "Cantarell", "Noto Sans", sans-serif; }
window.cerrojo { background: transparent; }
window.cerrojo.csd { border-radius: 28px; box-shadow: 0 22px 60px rgba(0,0,0,0.50), 0 0 0 1px rgba(255,255,255,0.10); }
.glass {
  border-radius: 28px; background-color: rgba(24, 26, 36, 0.80);
  border: 1px solid rgba(255,255,255,0.16); box-shadow: inset 0 1px 0 rgba(255,255,255,0.22);
}
.frost { border-radius: 28px; background-image: linear-gradient(160deg, rgba(255,255,255,0.10), rgba(255,255,255,0.02) 45%, rgba(255,255,255,0.06)); }
.topbar { min-height: 34px; }
.brand { font-size: 15px; font-weight: 700; letter-spacing: 0.3px; color: #F4F5F8; }
.pill {
  padding: 4px 12px 4px 10px; border-radius: 999px; font-size: 12px; font-weight: 600; color: #E6E8EE;
  background-color: rgba(0,0,0,0.22); border: 1px solid rgba(255,255,255,0.14);
}
.pdot { min-width: 8px; min-height: 8px; border-radius: 999px; background-color: #8B93A7; }
.pill.ok .pdot { background-color: #3FD07A; box-shadow: 0 0 8px rgba(63,208,122,0.9); }
.pill.wait .pdot { background-color: #FFB454; animation: pulse 1.4s ease-in-out infinite; }
@keyframes pulse { 0% { opacity: 1; } 50% { opacity: 0.25; } 100% { opacity: 1; } }
button.light { min-width: 13px; min-height: 13px; padding: 0; margin: 0; border-radius: 999px; border: none;
  box-shadow: inset 0 0 0 1px rgba(0,0,0,0.22); background-image: none; outline: none; }
button.light.close { background-color: #ff5f57; }
button.light.min { background-color: #febc2e; }
button.light.zoom { background-color: rgba(255,255,255,0.18); }
button.light:hover { filter: brightness(1.15); }
.hero {
  background-color: rgba(255,255,255,0.07);
  background-image: linear-gradient(170deg, rgba(255,255,255,0.14), rgba(255,255,255,0.02));
  border: 1px solid rgba(255,255,255,0.14); border-radius: 24px; padding: 16px;
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.20), 0 10px 26px rgba(0,0,0,0.28);
}
.hero.safe { border-color: rgba(0,195,227,0.85); box-shadow: 0 0 0 1px rgba(0,195,227,0.40), 0 0 34px rgba(0,195,227,0.30), inset 0 1px 0 rgba(255,255,255,0.28); }
.hero.open { border-color: rgba(255,69,84,0.85); box-shadow: 0 0 0 1px rgba(255,69,84,0.40), 0 0 34px rgba(255,69,84,0.30), inset 0 1px 0 rgba(255,255,255,0.28); }
.big { font-size: 22px; font-weight: 700; color: #F4F5F8; }
.sub { font-size: 12.5px; color: rgba(230,232,238,0.72); }
.row {
  background-color: rgba(255,255,255,0.06); border: 1px solid rgba(255,255,255,0.10);
  border-radius: 16px; padding: 8px 12px;
}
.row-name { font-size: 14.5px; font-weight: 700; color: #F4F5F8; }
.row-hint { font-size: 11.5px; color: rgba(230,232,238,0.60); }
.chip { padding: 3px 10px; border-radius: 999px; font-size: 11.5px; font-weight: 600;
  background-color: rgba(255,255,255,0.10); color: rgba(244,245,248,0.80); }
.chip.blocked { background-color: rgba(0,195,227,0.20); color: #9DEBF7; }
.chip.live { background-color: rgba(255,69,84,0.22); color: #FFB3BA; }
.seg { margin-top: 5px; }
.seg button { min-height: 22px; padding: 0 10px; border-radius: 999px; font-size: 11px; font-weight: 600;
  background-image: none; box-shadow: none; background-color: rgba(255,255,255,0.05);
  color: rgba(244,245,248,0.70); border: 1px solid rgba(255,255,255,0.14); }
.seg button:hover { background-color: rgba(255,255,255,0.10); }
.seg button:checked { background-color: rgba(0,195,227,0.22); border-color: rgba(0,195,227,0.7); color: #9DEBF7; }
checkbutton.extra { margin-top: 3px; font-size: 11.5px; color: rgba(230,232,238,0.75); }
checkbutton.extra check { min-width: 14px; min-height: 14px; border-radius: 4px; background-image: none;
  background-color: rgba(255,255,255,0.08); border: 1px solid rgba(255,255,255,0.22); box-shadow: none; }
checkbutton.extra check:checked { background-color: #00C3E3; border-color: #00C3E3; color: #0B2A33; }
switch { border-radius: 999px; background-color: rgba(255,255,255,0.16); border: none; }
switch:checked { background-color: #00C3E3; }
switch slider { border-radius: 999px; background-color: #F4F5F8; border: none; box-shadow: none; }
.devchip { min-height: 26px; padding: 0 12px; border-radius: 999px; background-image: none; box-shadow: none;
  background-color: rgba(0,0,0,0.22); border: 1px solid rgba(255,255,255,0.14); color: #E6E8EE; font-size: 12px; font-weight: 600; }
.devchip.on { background-color: rgba(0,195,227,0.22); border-color: rgba(0,195,227,0.7); color: #9DEBF7; }
entry.pkg { min-height: 34px; border-radius: 12px; background-color: rgba(0,0,0,0.25); color: #F4F5F8;
  border: 1px solid rgba(255,255,255,0.14); font-family: monospace; font-size: 12.5px; box-shadow: none; }
button.primary, button.ghost { min-height: 40px; padding: 0 20px; border-radius: 14px; background-image: none; font-weight: 600; box-shadow: none; }
button.primary { background-color: #00C3E3; color: #0B2A33; border: none; }
button.primary:hover { background-color: #3DD3EC; }
button.primary:disabled { background-color: rgba(255,255,255,0.12); color: rgba(244,245,248,0.45); }
button.ghost { background-color: transparent; color: #E6E8EE; border: 1px solid rgba(255,255,255,0.22); }
button.ghost:hover { background-color: rgba(255,255,255,0.08); }
button.ghost.small { min-height: 34px; padding: 0 14px; border-radius: 12px; }
.footer { padding-top: 10px; border-top: 1px solid rgba(255,255,255,0.10); }
.foot-hint { font-size: 12px; color: rgba(230,232,238,0.62); }
.empty-title { font-size: 20px; font-weight: 700; color: #F4F5F8; }
.empty-step { font-size: 13.5px; color: #D3D6DE; }
scrolledwindow, scrolledwindow > viewport { background: none; }
scrolledwindow undershoot, scrolledwindow overshoot { background: none; box-shadow: none; border: none; }
.confirm { background-color: rgba(14,15,22,0.86); border-radius: 28px; }
.confirm-card { background-color: rgba(40,42,54,0.96); border: 1px solid rgba(255,255,255,0.16); border-radius: 22px; padding: 22px; }
"#;

const SOLID_CSS: &str = r#"
window.cerrojo { background-color: #181A24; }
window.cerrojo.csd { border-radius: 0; box-shadow: none; }
.glass, .frost, .confirm { border-radius: 0; }
"#;

const APP_WHY: &str =
    "Se podrá volver a instalar desde la Play Store. Si lo haces por un rato, acuérdate de bloquearla después.";
const WEB_WHY: &str = "Instagram, TikTok, Facebook… volverán a abrir en el navegador del celular. \
     Si lo haces por un rato, acuérdate de activarlo después.";

// ---- Mensajes entre la ventana y el hilo de adb ----

enum Job {
    Refresh(String),
    /// Pone las apps como se quieren: con candado o, en modo «solo los
    /// Shorts», usables y con el vigía.
    Block { serial: String, apps: Vec<App>, auto: bool },
    Unblock { serial: String, app: App },
    /// Poner el filtro web como dice equipos.json.
    Web(String),
}

enum Msg {
    Devices(Vec<Device>, usize, bool),
    States(String, HashMap<String, PkgState>),
    Busy(String, String),
    Done(String, String),
    Web(String, device::WebState),
    /// ¿Está encendido el vigía de Shorts y Reels?
    Guard(String, bool),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppState {
    Blocked,
    Live,
    Free,
}

fn app_state(app: &App, st: &HashMap<String, PkgState>) -> AppState {
    let get = |p: &String| st.get(p).copied().unwrap_or(PkgState::Absent);
    if app.packages.iter().any(|p| get(p) == PkgState::Live) {
        AppState::Live
    } else if app.packages.iter().all(|p| get(p) == PkgState::Blocked) {
        AppState::Blocked
    } else {
        AppState::Free
    }
}

/// ¿Hay que tocar algo en el celular para que esta app quede como se quiere?
fn needs_apply(app: &App, st: &HashMap<String, PkgState>, partial: bool) -> bool {
    let get = |p: &String| st.get(p).copied().unwrap_or(PkgState::Absent);
    match app.partial.as_ref().filter(|_| partial) {
        // Lo que se deja usar, libre; lo demás (la versión Lite…), con candado.
        Some(pp) => app.packages.iter().any(|p| (get(p) == PkgState::Blocked) == pp.packages.contains(p)),
        None => app_state(app, st) != AppState::Blocked,
    }
}

fn worker(jobs: mpsc::Receiver<Job>, tx: mpsc::Sender<Msg>) {
    let refresh = |serial: &str| {
        let apps = catalog::all();
        let pkgs: Vec<&str> = apps.iter().flat_map(|a| a.packages.iter().map(String::as_str)).collect();
        match device::states(serial, &pkgs) {
            Ok(st) => {
                let _ = tx.send(Msg::States(serial.to_string(), st));
            }
            Err(e) => {
                let _ = tx.send(Msg::Done(serial.to_string(), format!("No se pudo leer el estado: {e}")));
            }
        }
    };
    let web = |serial: &str, errors: &mut Vec<String>| {
        let _ = tx.send(Msg::Busy(serial.to_string(), "Ajustando páginas web y vigía…".into()));
        if let Err(e) = crate::store::sync_web(serial) {
            errors.push(format!("ayudante: {e:#}"));
        }
        // La VPN tarda un instante en levantarse.
        std::thread::sleep(Duration::from_millis(2500));
        if let Ok(w) = device::web_state(serial) {
            let _ = tx.send(Msg::Web(serial.to_string(), w));
        }
        if let Ok(g) = device::guard_state(serial) {
            let _ = tx.send(Msg::Guard(serial.to_string(), g));
        }
    };
    while let Ok(job) = jobs.recv() {
        match job {
            Job::Refresh(serial) => {
                refresh(&serial);
                if let Ok(w) = device::web_state(&serial) {
                    let _ = tx.send(Msg::Web(serial.clone(), w));
                }
                if let Ok(g) = device::guard_state(&serial) {
                    let _ = tx.send(Msg::Guard(serial, g));
                }
            }
            Job::Web(serial) => {
                let mut errors = Vec::new();
                web(&serial, &mut errors);
                let _ = tx.send(Msg::Done(serial, errors.first().map(|e| format!("Falló: {e}")).unwrap_or_default()));
            }
            Job::Block { serial, apps, auto } => {
                let mut errors = Vec::new();
                let store = Store::load();
                for app in &apps {
                    let partial = app.partial.as_ref().filter(|_| store.is_partial(&serial, app));
                    let text = match partial {
                        Some(pp) => format!("Dejando {} sin {}…", app.name, pp.what),
                        None => format!("Bloqueando {}…", app.name),
                    };
                    let _ = tx.send(Msg::Busy(serial.clone(), text));
                    for p in &app.packages {
                        let r = if partial.is_some_and(|pp| pp.packages.contains(p)) {
                            device::unblock(&serial, p)
                        } else {
                            device::block(&serial, p)
                        };
                        if let Err(e) = r {
                            errors.push(format!("{p}: {e}"));
                        }
                    }
                }
                web(&serial, &mut errors);
                let text = if !errors.is_empty() {
                    format!("Falló: {}", errors.join("; "))
                } else if auto {
                    let names: Vec<&str> = apps.iter().map(|a| a.name.as_str()).collect();
                    format!("Se volvió a bloquear: {}", names.join(", "))
                } else {
                    String::new()
                };
                refresh(&serial);
                let _ = tx.send(Msg::Done(serial, text));
            }
            Job::Unblock { serial, app } => {
                let _ = tx.send(Msg::Busy(serial.clone(), format!("Desbloqueando {}…", app.name)));
                let mut errors = Vec::new();
                for p in &app.packages {
                    if let Err(e) = device::unblock(&serial, p) {
                        errors.push(format!("{p}: {e}"));
                    }
                }
                web(&serial, &mut errors);
                refresh(&serial);
                let text = if errors.is_empty() {
                    format!("{} desbloqueada: ya se puede instalar", app.name)
                } else {
                    format!("Falló: {}", errors.join("; "))
                };
                let _ = tx.send(Msg::Done(serial, text));
            }
        }
    }
}

fn poller(tx: mpsc::Sender<Msg>) {
    let mut last: Option<(Vec<String>, usize)> = None;
    loop {
        let has_adb = device::adb_available();
        let (list, unauthorized) = if has_adb { device::devices() } else { (Vec::new(), 0) };
        let key = (list.iter().map(|d| d.serial.clone()).collect::<Vec<_>>(), unauthorized);
        if last.as_ref() != Some(&key) {
            last = Some(key);
            if tx.send(Msg::Devices(list, unauthorized, has_adb)).is_err() {
                return;
            }
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

// ---- Estado de la ventana ----

struct Row {
    app: App,
    chip: gtk::Label,
    switch: gtk::Switch,
    /// «Toda la app» / «Solo los Shorts», si la app lo admite.
    modes: Option<(gtk::ToggleButton, gtk::ToggleButton)>,
    /// La casilla de la opción extra («Quitar también los directos»).
    extra: Option<gtk::CheckButton>,
}

struct Ui {
    pill: gtk::Box,
    pill_text: gtk::Label,
    devchips: gtk::Box,
    stack: gtk::Stack,
    hero: gtk::Box,
    lock: gtk::DrawingArea,
    count: gtk::Label,
    count_sub: gtk::Label,
    block_all: gtk::Button,
    list: gtk::Box,
    entry: gtk::Entry,
    empty_title: gtk::Label,
    empty_steps: gtk::Label,
    foot: gtk::Label,
    confirm: gtk::Box,
    confirm_text: gtk::Label,
    confirm_why: gtk::Label,
    confirm_go: gtk::Button,
}

struct State {
    jobs: mpsc::Sender<Job>,
    store: Store,
    devices: Vec<Device>,
    unauthorized: usize,
    has_adb: bool,
    current: Option<String>,
    states: HashMap<String, HashMap<String, PkgState>>,
    busy: Option<String>,
    note: String,
    /// Equipos ya revisados en esta conexión (para no reponer en bucle).
    checked: HashSet<String>,
    rows: Vec<Row>,
    pending_unblock: Option<App>,
    /// Se está confirmando pasar una app a «solo los Shorts».
    pending_partial: Option<App>,
    /// Se está confirmando apagar el filtro web.
    pending_web: bool,
    web: HashMap<String, device::WebState>,
    web_row: Option<(gtk::Label, gtk::Switch)>,
    guard: HashMap<String, bool>,
    tint: Option<(f64, f64, f64)>,
}

pub fn run() -> Result<()> {
    let app = gtk::Application::builder().application_id("dev.cerrojo").build();
    app.connect_activate(build);
    app.run_with_args::<&str>(&[]);
    Ok(())
}

fn build(app: &gtk::Application) {
    if let Some(w) = app.active_window() {
        w.present();
        return;
    }
    let provider = gtk::CssProvider::new();
    // Fuera de Linux no se confía en la transparencia de la ventana: fondo
    // sólido y sin esquinas redondeadas (si no, salen negras).
    let css = if cfg!(target_os = "linux") {
        CSS.to_string()
    } else {
        format!("{CSS}{SOLID_CSS}")
    };
    #[allow(deprecated)]
    provider.load_from_data(&css);
    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().expect("sin pantalla"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
    );

    let (tx, rx) = mpsc::channel();
    let (jobs_tx, jobs_rx) = mpsc::channel();
    {
        let tx = tx.clone();
        std::thread::spawn(move || worker(jobs_rx, tx));
    }
    std::thread::spawn(move || poller(tx));

    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .default_width(WIN_W)
        .default_height(WIN_H)
        .resizable(false)
        .icon_name("cerrojo")
        .title("Cerrojo")
        .build();
    window.add_css_class("cerrojo");
    window.set_titlebar(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));

    let state = Rc::new(RefCell::new(State {
        jobs: jobs_tx,
        store: Store::load(),
        devices: Vec::new(),
        unauthorized: 0,
        has_adb: true,
        current: None,
        states: HashMap::new(),
        busy: None,
        note: String::new(),
        checked: HashSet::new(),
        rows: Vec::new(),
        pending_unblock: None,
        pending_partial: None,
        pending_web: false,
        web: HashMap::new(),
        web_row: None,
        guard: HashMap::new(),
        tint: None,
    }));

    let start = Instant::now();
    let glass = gtk::Overlay::new();
    glass.add_css_class("glass");
    glass.set_overflow(gtk::Overflow::Hidden);
    glass.set_size_request(WIN_W, WIN_H);
    let aurora = gtk::DrawingArea::new();
    {
        let state = state.clone();
        aurora.set_draw_func(move |_, cr, w, h| {
            let tint = state.borrow().tint;
            draw_aurora(cr, w as f64, h as f64, start.elapsed().as_secs_f64(), tint);
        });
    }
    glass.set_child(Some(&aurora));
    let frost = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frost.add_css_class("frost");
    frost.set_can_target(false);
    glass.add_overlay(&frost);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 14);
    content.set_margin_top(16);
    content.set_margin_start(22);
    content.set_margin_end(22);
    content.set_margin_bottom(16);
    glass.add_overlay(&content);

    // Barra superior: semáforos, marca, estado.
    let lights = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for (class, action) in [("close", 0), ("min", 1), ("zoom", 2)] {
        let b = gtk::Button::new();
        b.add_css_class("light");
        b.add_css_class(class);
        b.set_valign(gtk::Align::Center);
        let w = window.clone();
        let state = state.clone();
        b.connect_clicked(move |_| match action {
            0 => w.close(),
            1 => w.minimize(),
            _ => {
                let s = state.borrow();
                if let Some(serial) = &s.current {
                    let _ = s.jobs.send(Job::Refresh(serial.clone()));
                }
            }
        });
        if action == 2 {
            b.set_tooltip_text(Some("Actualizar"));
        }
        lights.append(&b);
    }
    let topbar = gtk::CenterBox::new();
    topbar.add_css_class("topbar");
    topbar.set_start_widget(Some(&lights));
    let brand = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let mark = gtk::DrawingArea::new();
    mark.set_content_width(18);
    mark.set_content_height(18);
    mark.set_valign(gtk::Align::Center);
    mark.set_draw_func(|_, cr, w, h| draw_lock(cr, w as f64, h as f64, TEAL, true, 1.0));
    brand.append(&mark);
    let name = gtk::Label::new(Some("Cerrojo"));
    name.add_css_class("brand");
    brand.append(&name);
    topbar.set_center_widget(Some(&brand));
    let pill = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    pill.add_css_class("pill");
    pill.set_valign(gtk::Align::Center);
    let pill_dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    pill_dot.add_css_class("pdot");
    pill_dot.set_valign(gtk::Align::Center);
    pill.append(&pill_dot);
    let pill_text = gtk::Label::new(Some("Buscando…"));
    pill_text.set_max_width_chars(34);
    pill_text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    pill.append(&pill_text);
    topbar.set_end_widget(Some(&pill));
    content.append(&topbar);

    let devchips = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    devchips.set_halign(gtk::Align::Center);
    devchips.set_visible(false);
    content.append(&devchips);

    let stack = gtk::Stack::new();
    stack.set_vexpand(true);
    content.append(&stack);

    // Pantalla sin celular.
    let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
    empty.set_valign(gtk::Align::Center);
    empty.set_halign(gtk::Align::Center);
    let big_lock = gtk::DrawingArea::new();
    big_lock.set_content_width(90);
    big_lock.set_content_height(90);
    big_lock.set_draw_func(|_, cr, w, h| draw_lock(cr, w as f64, h as f64, (0.55, 0.58, 0.66), true, 0.8));
    empty.append(&big_lock);
    let empty_title = gtk::Label::new(Some("Conecta el celular por USB"));
    empty_title.add_css_class("empty-title");
    empty.append(&empty_title);
    let empty_steps = gtk::Label::new(None);
    empty_steps.add_css_class("empty-step");
    empty_steps.set_justify(gtk::Justification::Center);
    empty_steps.set_wrap(true);
    empty_steps.set_max_width_chars(84);
    empty.append(&empty_steps);
    stack.add_named(&empty, Some("empty"));

    // Pantalla principal.
    let main = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    let hero = gtk::Box::new(gtk::Orientation::Vertical, 10);
    hero.add_css_class("hero");
    hero.set_size_request(250, -1);
    hero.set_margin_bottom(8);
    let lock = gtk::DrawingArea::new();
    lock.set_content_width(200);
    lock.set_content_height(150);
    lock.set_vexpand(true);
    {
        let state = state.clone();
        lock.set_draw_func(move |_, cr, w, h| {
            let s = state.borrow();
            let (color, closed) = match s.tint {
                Some(c) if c == RED => (RED, false),
                Some(c) => (c, true),
                None => ((0.55, 0.58, 0.66), true),
            };
            let pulse = if s.busy.is_some() { 0.7 + 0.3 * (start.elapsed().as_secs_f64() * 4.0).sin().abs() } else { 1.0 };
            draw_lock(cr, w as f64, h as f64, color, closed, pulse);
        });
    }
    hero.append(&lock);
    let count = gtk::Label::new(None);
    count.add_css_class("big");
    hero.append(&count);
    let count_sub = gtk::Label::new(None);
    count_sub.add_css_class("sub");
    count_sub.set_wrap(true);
    count_sub.set_justify(gtk::Justification::Center);
    count_sub.set_max_width_chars(28);
    hero.append(&count_sub);
    let block_all = gtk::Button::with_label("Bloquear todo");
    block_all.add_css_class("primary");
    {
        let state = state.clone();
        block_all.connect_clicked(move |_| {
            let mut s = state.borrow_mut();
            let Some(serial) = s.current.clone() else { return };
            let st = s.states.get(&serial).cloned().unwrap_or_default();
            // Respeta el modo de cada app (YouTube «solo los Shorts», por ejemplo).
            let apps: Vec<App> = catalog::all()
                .into_iter()
                .filter(|a| !s.store.wants(&serial, &a.name) || needs_apply(a, &st, s.store.is_partial(&serial, a)))
                .collect();
            for a in catalog::all() {
                s.store.set(&serial, &a.name, true);
            }
            s.store.set_web(&serial, true);
            if !apps.is_empty() {
                let _ = s.jobs.send(Job::Block { serial, apps, auto: false });
            } else {
                let _ = s.jobs.send(Job::Web(serial));
            }
        });
    }
    hero.append(&block_all);
    main.append(&hero);

    let right = gtk::Box::new(gtk::Orientation::Vertical, 8);
    right.set_hexpand(true);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_propagate_natural_height(false);
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    list.set_margin_end(10);
    scroll.set_child(Some(&list));
    right.append(&scroll);
    let add = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let entry = gtk::Entry::new();
    entry.add_css_class("pkg");
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some("Otra app: nombre de paquete (com.ejemplo.app)"));
    add.append(&entry);
    let add_btn = gtk::Button::with_label("Añadir y bloquear");
    add_btn.add_css_class("ghost");
    add_btn.add_css_class("small");
    add.append(&add_btn);
    add.set_margin_end(10);
    add.set_margin_bottom(8);
    right.append(&add);
    main.append(&right);
    stack.add_named(&main, Some("main"));

    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    footer.add_css_class("footer");
    let foot = gtk::Label::new(None);
    foot.add_css_class("foot-hint");
    foot.set_hexpand(true);
    foot.set_xalign(0.0);
    foot.set_ellipsize(gtk::pango::EllipsizeMode::End);
    footer.append(&foot);
    let version = gtk::Label::new(Some(concat!("v", env!("CARGO_PKG_VERSION"))));
    version.add_css_class("foot-hint");
    footer.append(&version);
    content.append(&footer);

    // Confirmación para desbloquear: que cueste un poco.
    let confirm = gtk::Box::new(gtk::Orientation::Vertical, 0);
    confirm.add_css_class("confirm");
    confirm.set_visible(false);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 14);
    card.add_css_class("confirm-card");
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Center);
    card.set_vexpand(true);
    card.set_size_request(420, -1);
    let confirm_text = gtk::Label::new(None);
    confirm_text.add_css_class("empty-title");
    confirm_text.set_wrap(true);
    confirm_text.set_max_width_chars(34);
    card.append(&confirm_text);
    let why = gtk::Label::new(Some(APP_WHY));
    why.add_css_class("sub");
    why.set_wrap(true);
    why.set_max_width_chars(46);
    card.append(&why);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    buttons.set_halign(gtk::Align::End);
    let keep = gtk::Button::with_label("Mejor no");
    keep.add_css_class("primary");
    let go = gtk::Button::with_label("Desbloquear");
    go.add_css_class("ghost");
    buttons.append(&go);
    buttons.append(&keep);
    card.append(&buttons);
    confirm.append(&card);
    glass.add_overlay(&confirm);

    window.set_child(Some(&glass));

    let ui = Rc::new(Ui {
        pill,
        pill_text,
        devchips,
        stack,
        hero,
        lock,
        count,
        count_sub,
        block_all,
        list,
        entry,
        empty_title,
        empty_steps,
        foot,
        confirm,
        confirm_text,
        confirm_why: why,
        confirm_go: go.clone(),
    });

    {
        let (state, ui) = (state.clone(), ui.clone());
        keep.connect_clicked(move |_| {
            let mut s = state.borrow_mut();
            s.pending_unblock = None;
            s.pending_partial = None;
            s.pending_web = false;
            drop(s);
            ui.confirm.set_visible(false);
            render(&ui, &state);
        });
    }
    {
        let (state, ui) = (state.clone(), ui.clone());
        go.connect_clicked(move |_| {
            let mut s = state.borrow_mut();
            ui.confirm.set_visible(false);
            if std::mem::take(&mut s.pending_web) {
                let Some(serial) = s.current.clone() else { return };
                s.store.set_web(&serial, false);
                let _ = s.jobs.send(Job::Web(serial));
                return;
            }
            if let (Some(app), Some(serial)) = (s.pending_partial.take(), s.current.clone()) {
                s.store.set_partial(&serial, &app.name, true);
                s.store.set(&serial, &app.name, true);
                let _ = s.jobs.send(Job::Block { serial, apps: vec![app], auto: false });
                return;
            }
            let (Some(app), Some(serial)) = (s.pending_unblock.take(), s.current.clone()) else { return };
            s.store.set(&serial, &app.name, false);
            let _ = s.jobs.send(Job::Unblock { serial, app });
        });
    }
    {
        let entry = ui.entry.clone();
        let (state, ui) = (state.clone(), ui.clone());
        let add_action = move || {
            let pkg = ui.entry.text().trim().to_string();
            if !crate::apk::valid_package(&pkg) {
                state.borrow_mut().note = "Escribe un nombre de paquete válido, como com.ejemplo.app".into();
                render(&ui, &state);
                return;
            }
            let label = pkg.rsplit('.').next().unwrap_or(&pkg).to_string();
            let mut label = label.chars();
            let label = label.next().map(|c| c.to_uppercase().collect::<String>() + label.as_str()).unwrap_or_default();
            if let Err(e) = catalog::add_extra(&pkg, &label) {
                state.borrow_mut().note = format!("No se pudo guardar: {e}");
                render(&ui, &state);
                return;
            }
            ui.entry.set_text("");
            let mut s = state.borrow_mut();
            if let Some(serial) = s.current.clone() {
                if let Some(app) = catalog::all().into_iter().find(|a| a.packages.contains(&pkg)) {
                    s.store.set(&serial, &app.name, true);
                    let _ = s.jobs.send(Job::Block { serial, apps: vec![app], auto: false });
                }
            }
            drop(s);
            rebuild_rows(&ui, &state);
        };
        let add_action = Rc::new(add_action);
        let a = add_action.clone();
        add_btn.connect_clicked(move |_| a());
        entry.connect_activate(move |_| add_action());
    }

    rebuild_rows(&ui, &state);
    render(&ui, &state);

    {
        let (state, ui, aurora) = (state.clone(), ui.clone(), aurora.clone());
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let mut changed = false;
            while let Ok(msg) = rx.try_recv() {
                on_msg(&mut state.borrow_mut(), msg);
                changed = true;
            }
            if changed {
                render(&ui, &state);
            }
            aurora.queue_draw();
            if state.borrow().busy.is_some() {
                ui.lock.queue_draw();
            }
            glib::ControlFlow::Continue
        });
    }
    window.present();
}

fn on_msg(s: &mut State, msg: Msg) {
    match msg {
        Msg::Devices(list, unauthorized, has_adb) => {
            let serials: HashSet<String> = list.iter().map(|d| d.serial.clone()).collect();
            s.checked.retain(|x| serials.contains(x));
            s.states.retain(|k, _| serials.contains(k));
            if s.current.as_ref().is_none_or(|c| !serials.contains(c)) {
                s.current = list.first().map(|d| d.serial.clone());
                s.busy = None;
            }
            for d in &list {
                if !s.checked.contains(&d.serial) {
                    let _ = s.jobs.send(Job::Refresh(d.serial.clone()));
                }
            }
            s.devices = list;
            s.unauthorized = unauthorized;
            s.has_adb = has_adb;
        }
        Msg::States(serial, st) => {
            let apps = catalog::all();
            if !s.checked.contains(&serial) {
                s.checked.insert(serial.clone());
                // Lo que ya está bloqueado (por ejemplo, a mano antes de tener
                // Cerrojo) pasa a ser "lo que quieres".
                for a in &apps {
                    if app_state(a, &st) == AppState::Blocked && !s.store.wants(&serial, &a.name) {
                        s.store.set(&serial, &a.name, true);
                    }
                }
                let missing: Vec<App> = apps
                    .iter()
                    .filter(|a| s.store.wants(&serial, &a.name) && needs_apply(a, &st, s.store.is_partial(&serial, a)))
                    .cloned()
                    .collect();
                // Bloquear también repone el filtro web; si no falta ningún
                // candado, se repone aparte (por si quitaste la VPN).
                if !missing.is_empty() {
                    let _ = s.jobs.send(Job::Block { serial: serial.clone(), apps: missing, auto: true });
                } else {
                    let _ = s.jobs.send(Job::Web(serial.clone()));
                }
            }
            s.states.insert(serial, st);
        }
        Msg::Busy(serial, text) => {
            if s.current.as_deref() == Some(&serial) {
                s.busy = Some(text);
            }
        }
        Msg::Web(serial, w) => {
            s.web.insert(serial, w);
        }
        Msg::Guard(serial, g) => {
            s.guard.insert(serial, g);
        }
        Msg::Done(serial, text) => {
            if s.current.as_deref() == Some(&serial) {
                s.busy = None;
                s.note = text;
            }
        }
    }
}

fn rebuild_rows(ui: &Rc<Ui>, state: &Rc<RefCell<State>>) {
    while let Some(c) = ui.list.first_child() {
        ui.list.remove(&c);
    }
    // Primera fila: el filtro web, que no es una app.
    {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("row");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let name = gtk::Label::new(Some("Páginas web"));
        name.add_css_class("row-name");
        name.set_xalign(0.0);
        text.append(&name);
        let hint = gtk::Label::new(Some("Los sitios de las apps bloqueadas, en cualquier navegador"));
        hint.add_css_class("row-hint");
        hint.set_xalign(0.0);
        text.append(&hint);
        row.append(&text);
        let chip = gtk::Label::new(None);
        chip.add_css_class("chip");
        chip.set_valign(gtk::Align::Center);
        row.append(&chip);
        let switch = gtk::Switch::new();
        switch.set_valign(gtk::Align::Center);
        {
            let (state, ui) = (state.clone(), ui.clone());
            switch.connect_state_set(move |sw, on| {
                if sw.widget_name() == "sync" {
                    return glib::Propagation::Proceed;
                }
                let mut s = state.borrow_mut();
                let Some(serial) = s.current.clone() else { return glib::Propagation::Stop };
                if on {
                    s.store.set_web(&serial, true);
                    let _ = s.jobs.send(Job::Web(serial));
                    glib::Propagation::Proceed
                } else {
                    s.pending_web = true;
                    ui.confirm_text.set_text("¿Desactivar el bloqueo de páginas web?");
                    ui.confirm_why.set_text(WEB_WHY);
                    ui.confirm_go.set_label("Desactivar");
                    ui.confirm.set_visible(true);
                    glib::Propagation::Stop
                }
            });
        }
        row.append(&switch);
        ui.list.append(&row);
        state.borrow_mut().web_row = Some((chip, switch));
    }
    let mut rows = Vec::new();
    for app in catalog::all() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("row");
        let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let name = gtk::Label::new(Some(&app.name));
        name.add_css_class("row-name");
        name.set_xalign(0.0);
        text.append(&name);
        let hint = if app.domains.is_empty() { app.hint.clone() } else { format!("{} · también su web", app.hint) };
        let hint = gtk::Label::new(Some(&hint));
        hint.add_css_class("row-hint");
        hint.set_xalign(0.0);
        text.append(&hint);
        let modes = app.partial.as_ref().filter(|pp| !pp.only).map(|pp| {
            let seg = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            seg.add_css_class("seg");
            let whole = gtk::ToggleButton::with_label("Toda la app");
            let only = gtk::ToggleButton::with_label(&format!("Solo los {}", pp.what));
            only.set_group(Some(&whole));
            whole.set_active(true);
            for (b, partial) in [(&whole, false), (&only, true)] {
                let (state, ui, app) = (state.clone(), ui.clone(), app.clone());
                b.connect_clicked(move |b| {
                    if b.widget_name() == "sync" {
                        return;
                    }
                    choose_mode(&ui, &state, &app, partial);
                });
                seg.append(b);
            }
            text.append(&seg);
            (whole, only)
        });
        let extra = app.partial.as_ref().and_then(|pp| pp.extra.clone()).map(|ex| {
            let check = gtk::CheckButton::with_label(&ex.label);
            check.add_css_class("extra");
            check.set_halign(gtk::Align::Start);
            let state = state.clone();
            check.connect_toggled(move |c| {
                if c.widget_name() == "sync" {
                    return;
                }
                let mut s = state.borrow_mut();
                let Some(serial) = s.current.clone() else { return };
                s.store.set_extra(&serial, &ex.key, c.is_active());
                // Solo cambia lo que vigila el vigía: basta con ajustar el ayudante.
                let _ = s.jobs.send(Job::Web(serial));
            });
            text.append(&check);
            check
        });
        row.append(&text);
        let chip = gtk::Label::new(None);
        chip.add_css_class("chip");
        chip.set_valign(gtk::Align::Center);
        row.append(&chip);
        let switch = gtk::Switch::new();
        switch.set_valign(gtk::Align::Center);
        {
            let (state, ui, app) = (state.clone(), ui.clone(), app.clone());
            switch.connect_state_set(move |sw, on| {
                // Los cambios que hace render() no cuentan como clic.
                if sw.widget_name() == "sync" {
                    return glib::Propagation::Proceed;
                }
                let mut s = state.borrow_mut();
                let Some(serial) = s.current.clone() else { return glib::Propagation::Stop };
                if on {
                    s.store.set(&serial, &app.name, true);
                    let _ = s.jobs.send(Job::Block { serial, apps: vec![app.clone()], auto: false });
                    glib::Propagation::Proceed
                } else {
                    s.pending_unblock = Some(app.clone());
                    let guarded = s.store.is_partial(&serial, &app);
                    let what = app.partial.as_ref().map(|pp| pp.what.clone()).unwrap_or_default();
                    if guarded {
                        ui.confirm_text.set_text(&format!("¿Volver a ver {what} en {}?", app.name));
                        ui.confirm_why.set_text(&format!(
                            "{} se seguirá usando igual, pero volverá a mostrar {what}. Si es por un rato, acuérdate de bloquearlo después.",
                            app.name
                        ));
                        ui.confirm_go.set_label("Que vuelvan");
                    } else {
                        ui.confirm_text.set_text(&format!("¿Desbloquear {}?", app.name));
                        ui.confirm_why.set_text(APP_WHY);
                        ui.confirm_go.set_label("Desbloquear");
                    }
                    ui.confirm.set_visible(true);
                    glib::Propagation::Stop
                }
            });
        }
        row.append(&switch);
        ui.list.append(&row);
        rows.push(Row { app, chip, switch, modes, extra });
    }
    state.borrow_mut().rows = rows;
}

/// Clic en «Toda la app» o «Solo los Shorts». Pasar a toda la app (o elegir
/// un modo con la app sin bloquear) aprieta, así que va directo; dejar una app
/// bloqueada en «solo los Shorts» afloja, así que se confirma.
fn choose_mode(ui: &Rc<Ui>, state: &Rc<RefCell<State>>, app: &App, partial: bool) {
    let mut s = state.borrow_mut();
    let Some(serial) = s.current.clone() else { return };
    let wants = s.store.wants(&serial, &app.name);
    let current = s.store.is_partial(&serial, app);
    if wants && current == partial {
        return;
    }
    if wants && partial {
        let what = app.partial.as_ref().map(|pp| pp.what.clone()).unwrap_or_default();
        s.pending_partial = Some(app.clone());
        ui.confirm_text.set_text(&format!("¿Dejar {} abierta, sin {}?", app.name, what));
        ui.confirm_why.set_text(&format!(
            "{} se podrá usar (si no está, instálala desde la Play Store). Cerrojo cerrará los {} \
             en cuanto se abran y tapará su pestaña.",
            app.name, what
        ));
        ui.confirm_go.set_label("Sí, solo sin ellos");
        ui.confirm.set_visible(true);
        drop(s);
        // Mientras se confirma, los botones vuelven a lo que hay.
        render(ui, state);
        return;
    }
    s.store.set_partial(&serial, &app.name, partial);
    s.store.set(&serial, &app.name, true);
    let _ = s.jobs.send(Job::Block { serial, apps: vec![app.clone()], auto: false });
}

fn set_classes(w: &impl IsA<gtk::Widget>, on: &str, all: &[&str]) {
    for c in all {
        if *c == on {
            w.add_css_class(c);
        } else {
            w.remove_css_class(c);
        }
    }
}

fn render(ui: &Rc<Ui>, state: &Rc<RefCell<State>>) {
    let mut s = state.borrow_mut();

    // Botones de equipo si hay más de uno.
    while let Some(c) = ui.devchips.first_child() {
        ui.devchips.remove(&c);
    }
    ui.devchips.set_visible(s.devices.len() > 1);
    if s.devices.len() > 1 {
        for d in &s.devices {
            let b = gtk::Button::with_label(&d.model);
            b.add_css_class("devchip");
            if s.current.as_deref() == Some(&d.serial) {
                b.add_css_class("on");
            }
            let serial = d.serial.clone();
            let st = state.clone();
            // render() se llama desde el temporizador; este clic solo cambia el
            // equipo y el siguiente ciclo repinta.
            b.connect_clicked(move |_| {
                let mut s = st.borrow_mut();
                s.current = Some(serial.clone());
                s.busy = None;
                s.note.clear();
                let _ = s.jobs.send(Job::Refresh(serial.clone()));
            });
            ui.devchips.append(&b);
        }
    }

    let device = s.current.as_ref().and_then(|c| s.devices.iter().find(|d| &d.serial == c)).cloned();
    let Some(device) = device else {
        ui.stack.set_visible_child_name("empty");
        s.tint = None;
        set_classes(&ui.pill, "wait", &["ok", "wait"]);
        if !s.has_adb {
            ui.empty_title.set_text("Falta adb en este PC");
            ui.empty_steps.set_text(adb_install_hint());
            ui.pill_text.set_text("Sin adb");
        } else if s.unauthorized > 0 {
            ui.empty_title.set_text("Acepta el aviso en el celular");
            ui.empty_steps.set_text("Sale un cuadro «¿Permitir depuración USB?».\nMarca «Permitir siempre desde este equipo» y toca Permitir.");
            ui.pill_text.set_text("Esperando permiso");
        } else {
            ui.empty_title.set_text("Conecta el celular por USB");
            ui.empty_steps.set_text(
                "1. Ajustes › Acerca del teléfono: toca 7 veces «Número de compilación»\n\
                 (Samsung: dentro de «Información de software»; Xiaomi: «Versión de HyperOS»).\n\
                 2. Ajustes › Sistema › Opciones de desarrollador: activa «Depuración USB».\n\
                 3. Conecta el cable y acepta el aviso «¿Permitir depuración USB?».",
            );
            ui.pill_text.set_text("Sin celular");
        }
        ui.foot.set_text("Cerrojo pone un candado a las apps de reels. Solo se quita desde este PC.");
        return;
    };
    ui.stack.set_visible_child_name("main");

    let apps = catalog::all();
    if apps.len() != s.rows.len() {
        drop(s);
        // El catálogo cambió (app añadida): se rehace la lista.
        rebuild_rows(ui, state);
        s = state.borrow_mut();
    }
    let st = s.states.get(&device.serial).cloned();
    let busy = s.busy.clone();
    let mut blocked = 0;
    let mut live = 0;
    let guard = s.guard.get(&device.serial).copied();
    for row in &s.rows {
        let partial = s.store.is_partial(&device.serial, &row.app);
        let guarded = partial && s.store.wants(&device.serial, &row.app.name);
        let what = row.app.partial.as_ref().map(|pp| pp.what.as_str()).unwrap_or_default();
        let sin = format!("Sin {what}");
        let con = format!("Con {what}");
        let (text, class, on) = if guarded && st.is_some() {
            // Modo «solo los Shorts»: lo que cuenta es el vigía.
            match guard {
                Some(true) => {
                    blocked += 1;
                    (sin.as_str(), "blocked", true)
                }
                Some(false) => {
                    live += 1;
                    ("Vigía quitado", "live", true)
                }
                None => ("…", "", true),
            }
        } else if partial && st.is_some() {
            // WhatsApp o LinkedIn sin vigía: se usan normal, con sus historias.
            (con.as_str(), "", false)
        } else {
            match st.as_ref().map(|m| app_state(&row.app, m)) {
                Some(AppState::Blocked) => {
                    blocked += 1;
                    ("Bloqueada", "blocked", true)
                }
                Some(AppState::Live) => {
                    live += 1;
                    ("Instalada", "live", false)
                }
                Some(AppState::Free) => ("Sin bloquear", "", false),
                None => ("…", "", false),
            }
        };
        if let Some((whole, only)) = &row.modes {
            let b = if partial { only } else { whole };
            if !b.is_active() {
                b.set_widget_name("sync");
                b.set_active(true);
                b.set_widget_name("");
            }
            whole.set_sensitive(busy.is_none() && st.is_some());
            only.set_sensitive(busy.is_none() && st.is_some());
        }
        if let (Some(check), Some(ex)) = (&row.extra, row.app.partial.as_ref().and_then(|pp| pp.extra.as_ref())) {
            // Solo tiene sentido en «Solo los Shorts».
            check.set_visible(partial);
            let on = s.store.extra_on(&device.serial, &ex.key);
            if check.is_active() != on {
                check.set_widget_name("sync");
                check.set_active(on);
                check.set_widget_name("");
            }
            check.set_sensitive(busy.is_none() && st.is_some());
        }
        row.chip.set_text(text);
        set_classes(&row.chip, class, &["blocked", "live"]);
        if row.switch.is_active() != on || row.switch.state() != on {
            row.switch.set_widget_name("sync");
            row.switch.set_state(on);
            row.switch.set_active(on);
            row.switch.set_widget_name("");
        }
        row.switch.set_sensitive(busy.is_none() && st.is_some());
    }
    if let Some((chip, switch)) = &s.web_row {
        let w = s.web.get(&device.serial).copied();
        let wanted = s.store.web_enabled(&device.serial) && !s.store.web_domains(&device.serial).is_empty();
        let (text, class) = match w {
            Some(device::WebState::On) => ("Activo", "blocked"),
            Some(device::WebState::Off | device::WebState::Absent) if wanted => ("Quitado", "live"),
            Some(_) => ("Apagado", ""),
            None => ("…", ""),
        };
        chip.set_text(text);
        set_classes(chip, class, &["blocked", "live"]);
        let on = s.store.web_enabled(&device.serial);
        if switch.is_active() != on || switch.state() != on {
            switch.set_widget_name("sync");
            switch.set_state(on);
            switch.set_active(on);
            switch.set_widget_name("");
        }
        switch.set_sensitive(busy.is_none() && w.is_some());
    }
    let total = s.rows.len();
    ui.count.set_text(&format!("{blocked} de {total} bloqueadas"));
    let all_blocked = st.is_some() && blocked == total;
    ui.count_sub.set_text(if st.is_none() {
        "Leyendo el celular…"
    } else if all_blocked {
        "No se pueden instalar. Solo este PC puede desbloquearlas."
    } else if live > 0 {
        "Hay apps de reels instaladas y abiertas."
    } else {
        "Las que no están bloqueadas se pueden instalar desde la tienda."
    });
    s.tint = Some(if st.is_none() || all_blocked || live == 0 { TEAL } else { RED });
    set_classes(&ui.hero, if all_blocked { "safe" } else if live > 0 { "open" } else { "" }, &["safe", "open"]);
    ui.block_all.set_sensitive(busy.is_none() && st.is_some() && !all_blocked);
    ui.block_all.set_label(if all_blocked { "Todo bloqueado" } else { "Bloquear todo" });
    ui.lock.queue_draw();

    if let Some(b) = &busy {
        set_classes(&ui.pill, "wait", &["ok", "wait"]);
        ui.pill_text.set_text(b);
    } else {
        set_classes(&ui.pill, "ok", &["ok", "wait"]);
        ui.pill_text.set_text(&device.model);
    }
    ui.foot.set_text(if s.note.is_empty() {
        "Si quitas un candado desde el celular, se vuelve a poner al conectarlo a este PC."
    } else {
        &s.note
    });
}

fn adb_install_hint() -> &'static str {
    if cfg!(windows) {
        "Abre PowerShell y escribe:\n   winget install Google.PlatformTools\nLuego vuelve a abrir Cerrojo."
    } else if cfg!(target_os = "macos") {
        "Abre la Terminal y escribe:\n   brew install android-platform-tools\n(sin Homebrew: brew.sh)"
    } else {
        "Fedora:  sudo dnf install android-tools\nUbuntu, Debian, Mint:  sudo apt install adb\nArch:  sudo pacman -S android-tools"
    }
}

// ---- Dibujo ----

fn rounded(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
    cr.close_path();
}

fn mix(a: (f64, f64, f64), b: (f64, f64, f64), k: f64) -> (f64, f64, f64) {
    (a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k, a.2 + (b.2 - a.2) * k)
}

fn draw_aurora(cr: &cairo::Context, w: f64, h: f64, t: f64, tint: Option<(f64, f64, f64)>) {
    let base = [(0.0, 0.62, 0.95), (0.62, 0.32, 0.98), (1.0, 0.33, 0.45), (0.15, 0.85, 0.75)];
    let blobs = [(0.18, 0.25, 0.55, 0.13), (0.82, 0.30, 0.50, 0.17), (0.65, 0.95, 0.60, 0.11), (0.25, 0.90, 0.45, 0.15)];
    for (i, ((bx, by, r, speed), color)) in blobs.iter().zip(base).enumerate() {
        let phase = i as f64 * 1.7;
        let x = w * (bx + 0.08 * (t * speed + phase).sin());
        let y = h * (by + 0.10 * (t * speed * 1.3 + phase).cos());
        let radius = w.max(h) * r;
        let c = match tint {
            Some(tc) => mix(color, tc, 0.55),
            None => color,
        };
        let g = cairo::RadialGradient::new(x, y, 0.0, x, y, radius);
        g.add_color_stop_rgba(0.0, c.0, c.1, c.2, 0.55);
        g.add_color_stop_rgba(0.55, c.0, c.1, c.2, 0.18);
        g.add_color_stop_rgba(1.0, c.0, c.1, c.2, 0.0);
        let _ = cr.set_source(&g);
        let _ = cr.paint();
    }
}

/// Candado: arco y cuerpo de cristal. Abierto, el arco se levanta y se gira.
fn draw_lock(cr: &cairo::Context, w: f64, h: f64, color: (f64, f64, f64), closed: bool, lit: f64) {
    let size = w.min(h);
    let s = size / 100.0;
    let ox = (w - size) / 2.0;
    let oy = (h - size) / 2.0;
    let _ = cr.save();
    cr.translate(ox, oy);
    cr.scale(s, s);

    // Arco.
    let lift = if closed { 0.0 } else { 12.0 };
    cr.set_line_width(9.0);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.new_path();
    let (cx, top) = (50.0, 22.0 - lift);
    if closed {
        cr.move_to(cx - 20.0, 48.0);
        cr.line_to(cx - 20.0, top + 20.0);
        cr.arc(cx, top + 20.0, 20.0, std::f64::consts::PI, 0.0);
        cr.line_to(cx + 20.0, 48.0);
    } else {
        cr.move_to(cx + 20.0, 40.0 - lift);
        cr.line_to(cx + 20.0, top + 20.0);
        cr.arc_negative(cx, top + 20.0, 20.0, 0.0, std::f64::consts::PI);
        cr.line_to(cx - 20.0, top + 30.0);
    }
    cr.set_source_rgba(0.92, 0.94, 0.97, 0.85);
    let _ = cr.stroke();

    // Cuerpo.
    rounded(cr, 18.0, 44.0, 64.0, 50.0, 12.0);
    let g = cairo::LinearGradient::new(0.0, 44.0, 0.0, 94.0);
    g.add_color_stop_rgba(0.0, color.0, color.1, color.2, 0.95 * lit);
    g.add_color_stop_rgba(1.0, color.0 * 0.6, color.1 * 0.6, color.2 * 0.6, 0.95 * lit);
    let _ = cr.set_source(&g);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.35);
    cr.set_line_width(1.5);
    let _ = cr.stroke();

    // Ojo de la llave: un «play» (el video, bajo llave).
    cr.move_to(44.0, 59.0);
    cr.line_to(44.0, 79.0);
    cr.line_to(61.0, 69.0);
    cr.close_path();
    cr.set_line_join(cairo::LineJoin::Round);
    cr.set_line_width(4.0);
    cr.set_source_rgba(0.04, 0.16, 0.2, 0.9);
    let _ = cr.fill_preserve();
    let _ = cr.stroke();
    let _ = cr.restore();
}
