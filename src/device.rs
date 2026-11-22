//! Lo que se hace en el celular, todo por adb.
//!
//! Bloquear un paquete:
//! - Si ya hay un candado nuestro: se asegura de que esté suspendido.
//! - App de fábrica (YouTube en casi todos los Android): no se puede
//!   reemplazar, así que se deja instalada y se **suspende**. La tienda la ve
//!   instalada y al abrirla sale "app pausada".
//! - App instalada normal: se desinstala y se pone el candado.
//! - No instalada: se pone el candado.
//!
//! El candado y la suspensión hechos por el shell de adb solo se quitan con
//! adb: desde el celular no hay forma.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::apk;

#[derive(Clone, Debug)]
pub struct Device {
    pub serial: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PkgState {
    /// No está en el celular: se podría instalar.
    Absent,
    /// La app real, usable.
    Live,
    /// Candado nuestro o app de fábrica suspendida.
    Blocked,
}

#[derive(Clone, Debug, Default)]
struct Info {
    present: bool,
    installed: bool,
    system: bool,
    suspended: bool,
    ours: bool,
}

/// adb del PATH o, si no está (una app abierta desde el Finder o el menú de
/// Inicio no hereda el PATH de la terminal), de los sitios de siempre.
fn adb_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    use std::sync::OnceLock;
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        if let Some(p) = std::env::var_os("CERROJO_ADB") {
            return PathBuf::from(p);
        }
        let exe = if cfg!(windows) { "adb.exe" } else { "adb" };
        if let Some(paths) = std::env::var_os("PATH") {
            for dir in std::env::split_paths(&paths) {
                if dir.join(exe).is_file() {
                    return dir.join(exe);
                }
            }
        }
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
        let mut candidates = vec![
            home.join("Android/Sdk/platform-tools").join(exe),
            home.join("Library/Android/sdk/platform-tools").join(exe),
            home.join("platform-tools").join(exe),
            PathBuf::from("/opt/homebrew/bin/adb"),
            PathBuf::from("/usr/local/bin/adb"),
        ];
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            candidates.push(local.join("Android/Sdk/platform-tools/adb.exe"));
            candidates.push(local.join("Microsoft/WinGet/Links/adb.exe"));
        }
        // Junto a cerrojo.exe (quien descomprima platform-tools en la misma carpeta).
        if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.to_path_buf())) {
            candidates.push(dir.join(exe));
            candidates.push(dir.join("platform-tools").join(exe));
        }
        candidates.into_iter().find(|c| c.is_file()).unwrap_or_else(|| PathBuf::from(exe))
    })
    .clone()
}

fn adb() -> Command {
    #[allow(unused_mut)]
    let mut c = Command::new(adb_path());
    #[cfg(windows)]
    {
        // Sin esto, cada llamada a adb abre una consola negra un instante.
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    c
}

pub fn adb_available() -> bool {
    adb().arg("version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Equipos listos (los "unauthorized" no cuentan: hay que aceptar en el celular).
pub fn devices() -> (Vec<Device>, usize) {
    let Ok(out) = adb().args(["devices", "-l"]).stderr(Stdio::null()).output() else {
        return (Vec::new(), 0);
    };
    let mut list = Vec::new();
    let mut unauthorized = 0;
    for line in String::from_utf8_lossy(&out.stdout).lines().skip(1) {
        let mut it = line.split_whitespace();
        let (Some(serial), Some(state)) = (it.next(), it.next()) else { continue };
        match state {
            "device" => {
                let model = it
                    .find_map(|t| t.strip_prefix("model:"))
                    .unwrap_or(serial)
                    .replace('_', " ");
                list.push(Device { serial: serial.into(), model: nice_model(serial, &model) });
            }
            "unauthorized" => unauthorized += 1,
            _ => {}
        }
    }
    (list, unauthorized)
}

/// "SM X800" no le dice nada a nadie: se pregunta el nombre comercial.
fn nice_model(serial: &str, fallback: &str) -> String {
    for prop in ["ro.product.marketname", "ro.config.marketing_name"] {
        if let Ok(o) = shell(serial, &format!("getprop {prop}")) {
            let v = o.trim();
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    if let Ok(o) = shell(serial, "settings get global device_name") {
        let v = o.trim();
        if !v.is_empty() && v != "null" {
            return v.to_string();
        }
    }
    fallback.to_string()
}

fn shell(serial: &str, cmd: &str) -> Result<String> {
    let out = adb()
        .args(["-s", serial, "shell", cmd])
        .stdin(Stdio::null())
        .output()
        .context("no se pudo ejecutar adb")?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr))
}

/// Estado de varios paquetes con una sola llamada a adb.
fn infos(serial: &str, packages: &[&str]) -> Result<HashMap<String, Info>> {
    let mut script = String::new();
    for p in packages {
        script.push_str(&format!(
            "echo '@@{p}'; dumpsys package {p} | grep -E 'Package \\[|versionName=|pkgFlags=|User 0:'; "
        ));
    }
    let out = shell(serial, &script)?;
    let mut map: HashMap<String, Info> = HashMap::new();
    let mut current: Option<String> = None;
    for line in out.lines() {
        let line = line.trim();
        if let Some(p) = line.strip_prefix("@@") {
            current = Some(p.to_string());
            map.insert(p.to_string(), Info::default());
            continue;
        }
        let Some(p) = &current else { continue };
        let info = map.get_mut(p).unwrap();
        if line.starts_with(&format!("Package [{p}]")) {
            info.present = true;
        } else if let Some(v) = line.strip_prefix("versionName=") {
            // Si hay varias versiones (de fábrica + actualización), basta una.
            info.ours |= v.starts_with(apk::MARK);
        } else if line.starts_with("pkgFlags=") {
            info.system |= line.contains(" SYSTEM ");
        } else if line.starts_with("User 0:") && line.contains("installed=") {
            // Con versión de fábrica + actualización hay dos bloques: basta uno.
            info.installed |= line.contains("installed=true");
            info.suspended |= line.contains("suspended=true");
        }
    }
    Ok(map)
}

fn classify(i: &Info) -> PkgState {
    if !i.present || !i.installed {
        // De fábrica pero quitada para el usuario: la tienda la reinstala con
        // un toque, así que cuenta como libre.
        PkgState::Absent
    } else if i.ours || i.suspended {
        PkgState::Blocked
    } else {
        PkgState::Live
    }
}

pub fn states(serial: &str, packages: &[&str]) -> Result<HashMap<String, PkgState>> {
    Ok(infos(serial, packages)?.into_iter().map(|(p, i)| (p, classify(&i))).collect())
}

fn install_stub(serial: &str, package: &str) -> Result<()> {
    let bytes = apk::build(package)?;
    let path = std::env::temp_dir().join(format!("cerrojo-{}-{package}.apk", std::process::id()));
    std::fs::File::create(&path)?.write_all(&bytes)?;
    let out = adb().arg("-s").arg(serial).arg("install").arg("-r").arg(&path).stdin(Stdio::null()).output();
    let _ = std::fs::remove_file(&path);
    let out = out?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || !text.contains("Success") {
        bail!("no se pudo instalar el candado de {package}: {}", text.trim());
    }
    Ok(())
}

fn suspend(serial: &str, package: &str, on: bool) -> Result<()> {
    let verb = if on { "suspend" } else { "unsuspend" };
    let out = shell(serial, &format!("cmd package {verb} --user 0 {package}"))?;
    if !out.contains("new suspended state") {
        bail!("no se pudo {} {package}: {}", if on { "suspender" } else { "reactivar" }, out.trim());
    }
    Ok(())
}

pub fn block(serial: &str, package: &str) -> Result<()> {
    let info = infos(serial, &[package])?.remove(package).unwrap_or_default();
    if info.present && info.system && !info.ours {
        if !info.installed {
            shell(serial, &format!("cmd package install-existing --user 0 {package}"))?;
        }
        return suspend(serial, package, true);
    }
    if info.present && info.ours {
        if !info.installed {
            shell(serial, &format!("cmd package install-existing --user 0 {package}"))?;
        }
        return suspend(serial, package, true);
    }
    if info.present {
        let out = adb().args(["-s", serial, "uninstall", package]).stdin(Stdio::null()).output()?;
        if !String::from_utf8_lossy(&out.stdout).contains("Success") {
            // Puede quedar para otros usuarios; con quitarla del principal basta.
            shell(serial, &format!("pm uninstall --user 0 {package}"))?;
        }
    }
    install_stub(serial, package)?;
    suspend(serial, package, true)
}

pub fn unblock(serial: &str, package: &str) -> Result<()> {
    let info = infos(serial, &[package])?.remove(package).unwrap_or_default();
    if !info.present {
        return Ok(());
    }
    if info.ours {
        let out = adb().args(["-s", serial, "uninstall", package]).stdin(Stdio::null()).output()?;
        if !String::from_utf8_lossy(&out.stdout).contains("Success") {
            bail!("no se pudo quitar el candado de {package}");
        }
        return Ok(());
    }
    if info.suspended {
        suspend(serial, package, false)?;
    }
    Ok(())
}

// ---- Filtro web ----
//
// Una VPN local (android/) que solo ve el DNS y responde "no existe" a los
// dominios de las apps bloqueadas. Todo por adb, sin tocar el celular:
// - appops ACTIVATE_VPN: el permiso de VPN sin el diálogo de "¿confías?".
// - deviceidle whitelist: que el ahorro de batería no la duerma.
// - always_on_vpn_app: Android la vuelve a encender al reiniciar.
// - DNS privado apagado: si no, el celular preguntaría por fuera del túnel.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebState {
    /// No está instalado.
    Absent,
    /// Instalado pero sin la VPN puesta (la quitaste en Ajustes, por ejemplo).
    Off,
    On,
}

pub fn web_state(serial: &str) -> Result<WebState> {
    let p = apk::FILTER_PKG;
    let out = shell(
        serial,
        &format!(
            "pm path {p}; echo @@; dumpsys package {p} | grep -m1 versionCode=; echo @@; \
             dumpsys connectivity | grep -c '10.111.222.2/32'"
        ),
    )?;
    let parts: Vec<&str> = out.split("@@").collect();
    if parts.len() < 3 || !parts[0].contains("package:") {
        return Ok(WebState::Absent);
    }
    let version: u32 = parts[1]
        .split("versionCode=")
        .nth(1)
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if version < apk::FILTER_VERSION {
        return Ok(WebState::Absent);
    }
    let vpn = parts[2].trim().lines().last().and_then(|n| n.trim().parse::<u32>().ok()).unwrap_or(0);
    Ok(if vpn > 0 { WebState::On } else { WebState::Off })
}

fn install_filter(serial: &str) -> Result<()> {
    let bytes = apk::build_filter()?;
    let path = std::env::temp_dir().join(format!("cerrojo-{}-filtro.apk", std::process::id()));
    std::fs::File::create(&path)?.write_all(&bytes)?;
    let install = || adb().arg("-s").arg(serial).arg("install").arg("-r").arg(&path).stdin(Stdio::null()).output();
    let mut out = install()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    if text.contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE") {
        // Lo puso Cerrojo desde otro PC (otra firma): se cambia por el nuestro.
        shell(serial, &format!("pm uninstall {}", apk::FILTER_PKG))?;
        out = install()?;
        text = String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    }
    let _ = std::fs::remove_file(&path);
    if !out.status.success() || !text.contains("Success") {
        bail!("no se pudo instalar el filtro web: {}", text.trim());
    }
    Ok(())
}

/// Pone (o actualiza) el filtro con estos dominios.
pub fn web_on(serial: &str, domains: &[String]) -> Result<()> {
    let p = apk::FILTER_PKG;
    if web_state(serial)? == WebState::Absent {
        install_filter(serial)?;
    }
    let list: Vec<&str> = domains
        .iter()
        .map(String::as_str)
        .filter(|d| d.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '!' | '*')))
        .collect();
    let out = shell(
        serial,
        &format!(
            "appops set {p} ACTIVATE_VPN allow; cmd deviceidle whitelist +{p} >/dev/null; \
             settings put global private_dns_mode off; settings put secure always_on_vpn_app {p}; \
             am start -n {p}/.Orden --es dominios '{}'",
            list.join(" ")
        ),
    )?;
    if out.contains("Error") || out.contains("Exception") {
        bail!("no se pudo encender el filtro web: {}", out.trim());
    }
    Ok(())
}

/// Apaga el filtro web (la app se queda si el vigía la necesita; ver
/// helper_remove).
pub fn web_off(serial: &str) -> Result<()> {
    let p = apk::FILTER_PKG;
    if !shell(serial, &format!("pm path {p}"))?.contains("package:") {
        return Ok(());
    }
    shell(
        serial,
        &format!(
            "am start -n {p}/.Orden --ez apagar true; \
             [ \"$(settings get secure always_on_vpn_app)\" = {p} ] && settings delete secure always_on_vpn_app; \
             settings put global private_dns_mode opportunistic; cmd deviceidle whitelist -{p} >/dev/null"
        ),
    )?;
    Ok(())
}

/// Desinstala el ayudante (cuando ya no hace falta ni el filtro ni el vigía).
pub fn helper_remove(serial: &str) -> Result<()> {
    let p = apk::FILTER_PKG;
    shell(serial, &format!("pm path {p} | grep -q package: && sleep 1 && pm uninstall {p}"))?;
    Ok(())
}

// ---- Vigía de Shorts y Reels ----
//
// Un servicio de accesibilidad del mismo ayudante (android/…/Vigia.java) que
// cierra el reproductor de Shorts/Reels y tapa su pestaña. Se enciende sin
// tocar el celular escribiendo enabled_accessibility_services (el shell de adb
// puede); si lo apagas en Ajustes › Accesibilidad, Cerrojo lo vuelve a poner
// al conectar. ACCESS_RESTRICTED_SETTINGS evita el aviso de "ajuste
// restringido" de Android 13+ con apps que no vienen de una tienda.

const GUARD: &str = "dev.cerrojo.filtro/.Vigia";
/// La app de YouTube, a la que se le quita el picture-in-picture con el vigía.
const YT: &str = "com.google.android.youtube";

/// ¿Está el ayudante instalado y al día?
fn helper_installed(serial: &str) -> Result<bool> {
    let p = apk::FILTER_PKG;
    let out = shell(serial, &format!("pm path {p}; echo @@; dumpsys package {p} | grep -m1 versionCode="))?;
    let (path, version) = out.split_once("@@").unwrap_or((&out, ""));
    let version: u32 = version
        .split("versionCode=")
        .nth(1)
        .and_then(|v| v.split_whitespace().next())
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Ok(path.contains("package:") && version >= apk::FILTER_VERSION)
}

/// ¿Está el vigía encendido?
pub fn guard_state(serial: &str) -> Result<bool> {
    let p = apk::FILTER_PKG;
    let on = shell(serial, "settings get secure enabled_accessibility_services")?.contains(&format!("{p}/"));
    Ok(on && helper_installed(serial)?)
}

/// Enciende el vigía para estas apps ("youtube", "instagram").
pub fn guard_on(serial: &str, keys: &[String]) -> Result<()> {
    let (p, yt) = (apk::FILTER_PKG, YT);
    if !helper_installed(serial)? {
        install_filter(serial)?;
    }
    let keys: Vec<&str> =
        keys.iter().map(String::as_str).filter(|k| k.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).collect();
    let out = shell(
        serial,
        &format!(
            "appops set {p} ACCESS_RESTRICTED_SETTINGS allow 2>/dev/null; \
             am start -n {p}/.Orden --es vigia '{keys}' >/dev/null; \
             s=$(settings get secure enabled_accessibility_services); \
             case \"$s\" in *{p}/*) ;; ''|null) settings put secure enabled_accessibility_services {GUARD} ;; \
             *) settings put secure enabled_accessibility_services \"$s:{GUARD}\" ;; esac; \
             settings put secure accessibility_enabled 1; \
             appops set {yt} PICTURE_IN_PICTURE {pip}",
            keys = keys.join(" "),
            // Con YouTube vigilada, sin ventana flotante: un Short en
            // picture-in-picture quedaría fuera del alcance del vigía.
            pip = if keys.contains(&"youtube") { "ignore" } else { "default" },
        ),
    )?;
    if out.contains("Exception") {
        bail!("no se pudo encender el vigía: {}", out.trim());
    }
    Ok(())
}

/// Apaga el vigía (sin tocar otros servicios de accesibilidad).
pub fn guard_off(serial: &str) -> Result<()> {
    let (p, yt) = (apk::FILTER_PKG, YT);
    if !shell(serial, &format!("pm path {p}"))?.contains("package:") {
        return Ok(());
    }
    shell(
        serial,
        &format!(
            "am start -n {p}/.Orden --es vigia '' >/dev/null; \
             s=$(settings get secure enabled_accessibility_services); \
             case \"$s\" in *{p}/*) \
               n=$(echo \"$s\" | sed -e 's#{p}/[^:]*##g' -e 's/::*/:/g' -e 's/^://' -e 's/:$//'); \
               if [ -z \"$n\" ]; then settings delete secure enabled_accessibility_services; \
               else settings put secure enabled_accessibility_services \"$n\"; fi ;; esac; \
             appops set {yt} PICTURE_IN_PICTURE default"
        ),
    )?;
    Ok(())
}
