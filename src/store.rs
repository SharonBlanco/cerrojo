//! Qué apps quieres bloqueadas en cada equipo (~/.config/cerrojo/equipos.json).
//! Sirve para volver a poner el candado solo cuando el celular se conecta y
//! falta alguno (por ejemplo, si lo desinstalaste desde la Play Store).

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::config_dir;

#[derive(Default)]
pub struct Store {
    pub wanted: BTreeMap<String, BTreeSet<String>>,
    /// Equipos en los que apagaste el filtro web (~/.config/cerrojo/web.json).
    pub web_off: BTreeSet<String>,
    /// Las apps que en cada equipo van en modo «solo los Shorts»
    /// (~/.config/cerrojo/parcial.json).
    pub partial: BTreeMap<String, BTreeSet<String>>,
    /// Las opciones extra que apagaste en cada equipo (encendidas si no están
    /// aquí; ~/.config/cerrojo/opciones.json).
    pub extras_off: BTreeMap<String, BTreeSet<String>>,
}

impl Store {
    pub fn load() -> Self {
        let wanted = std::fs::read(config_dir().join("equipos.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let web_off = std::fs::read(config_dir().join("web.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let partial = std::fs::read(config_dir().join("parcial.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let extras_off = std::fs::read(config_dir().join("opciones.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Store { wanted, web_off, partial, extras_off }
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        if let Ok(b) = serde_json::to_vec_pretty(&self.wanted) {
            let _ = std::fs::write(config_dir().join("equipos.json"), b);
        }
        if let Ok(b) = serde_json::to_vec_pretty(&self.web_off) {
            let _ = std::fs::write(config_dir().join("web.json"), b);
        }
        if let Ok(b) = serde_json::to_vec_pretty(&self.partial) {
            let _ = std::fs::write(config_dir().join("parcial.json"), b);
        }
        if let Ok(b) = serde_json::to_vec_pretty(&self.extras_off) {
            let _ = std::fs::write(config_dir().join("opciones.json"), b);
        }
    }

    /// Encender o apagar una opción extra (los directos) en este equipo.
    pub fn set_extra(&mut self, serial: &str, key: &str, on: bool) {
        let set = self.extras_off.entry(serial.to_string()).or_default();
        if on {
            set.remove(key);
        } else {
            set.insert(key.to_string());
        }
        self.save();
    }

    pub fn extra_on(&self, serial: &str, key: &str) -> bool {
        !self.extras_off.get(serial).is_some_and(|s| s.contains(key))
    }

    /// Elegir entre bloquear la app entera o solo sus Shorts/Reels.
    pub fn set_partial(&mut self, serial: &str, app: &str, on: bool) {
        let set = self.partial.entry(serial.to_string()).or_default();
        if on {
            set.insert(app.to_string());
        } else {
            set.remove(app);
        }
        self.save();
    }

    /// ¿Esta app va en modo «solo los Shorts» en este equipo? (Solo si la app lo admite.)
    pub fn is_partial(&self, serial: &str, app: &crate::catalog::App) -> bool {
        match &app.partial {
            Some(p) if p.only => true,
            Some(_) => self.partial.get(serial).is_some_and(|s| s.contains(&app.name)),
            None => false,
        }
    }

    /// Lo que debe vigilar el vigía en este equipo ("youtube", "instagram").
    pub fn guard_keys(&self, serial: &str) -> Vec<String> {
        crate::catalog::all()
            .into_iter()
            .filter(|a| self.wants(serial, &a.name) && self.is_partial(serial, a))
            .filter_map(|a| a.partial)
            .flat_map(|p| {
                let extra = p.extra.map(|e| e.key).filter(|k| self.extra_on(serial, k));
                std::iter::once(p.key).chain(extra)
            })
            .collect()
    }

    /// Encender o apagar el filtro web en este equipo.
    pub fn set_web(&mut self, serial: &str, on: bool) {
        if on {
            self.web_off.remove(serial);
        } else {
            self.web_off.insert(serial.to_string());
        }
        self.save();
    }

    pub fn web_enabled(&self, serial: &str) -> bool {
        !self.web_off.contains(serial)
    }

    pub fn set(&mut self, serial: &str, app: &str, on: bool) {
        let set = self.wanted.entry(serial.to_string()).or_default();
        if on {
            set.insert(app.to_string());
        } else {
            set.remove(app);
        }
        self.save();
    }

    pub fn wants(&self, serial: &str, app: &str) -> bool {
        self.wanted.get(serial).is_some_and(|s| s.contains(app))
    }
}

impl Store {
    /// Los dominios que deben estar bloqueados en este equipo: los de las apps
    /// que quieres bloqueadas y, si hay alguno, los DNS cifrados que servirían
    /// para saltarse el filtro.
    pub fn web_domains(&self, serial: &str) -> Vec<String> {
        if !self.web_enabled(serial) {
            return Vec::new();
        }
        let mut out: Vec<String> = crate::catalog::all()
            .into_iter()
            // En modo «solo los Shorts» la app sigue funcionando y necesita su web.
            .filter(|a| self.wants(serial, &a.name) && !self.is_partial(serial, a))
            .flat_map(|a| a.domains)
            .collect();
        if out.iter().any(|d| !d.starts_with('!')) {
            out.extend(crate::catalog::BYPASS.iter().map(|d| d.to_string()));
        } else {
            out.clear();
        }
        out
    }
}

/// Deja el ayudante del celular como dice la configuración: el filtro web con
/// esos dominios y el vigía con esas apps. Si no hace falta ninguno de los
/// dos, lo desinstala. Devuelve cuántos dominios quedan bloqueados.
pub fn sync_web(serial: &str) -> anyhow::Result<usize> {
    let store = Store::load();
    let domains = store.web_domains(serial);
    let guard = store.guard_keys(serial);
    if domains.is_empty() {
        crate::device::web_off(serial)?;
    } else {
        crate::device::web_on(serial, &domains)?;
    }
    if guard.is_empty() {
        crate::device::guard_off(serial)?;
    } else {
        crate::device::guard_on(serial, &guard)?;
    }
    if domains.is_empty() && guard.is_empty() {
        crate::device::helper_remove(serial)?;
    }
    Ok(domains.iter().filter(|d| !d.starts_with('!')).count())
}
