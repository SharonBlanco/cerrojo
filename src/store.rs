//! Qué apps quieres bloqueadas en cada equipo (~/.config/cerrojo/equipos.json).
//! Sirve para volver a poner el candado solo cuando el celular se conecta y
//! falta alguno (por ejemplo, si lo desinstalaste desde la Play Store).

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::config_dir;

#[derive(Default)]
pub struct Store {
    pub wanted: BTreeMap<String, BTreeSet<String>>,
}

impl Store {
    pub fn load() -> Self {
        let wanted = std::fs::read(config_dir().join("equipos.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Store { wanted }
    }

    pub fn save(&self) {
        let _ = std::fs::create_dir_all(config_dir());
        if let Ok(b) = serde_json::to_vec_pretty(&self.wanted) {
            let _ = std::fs::write(config_dir().join("equipos.json"), b);
        }
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
        let mut out: Vec<String> = crate::catalog::all()
            .into_iter()
            .filter(|a| self.wants(serial, &a.name))
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

/// Deja el filtro web como dice equipos.json: puesto con esos dominios, o
/// quitado si no queda nada por bloquear. Devuelve cuántos dominios quedan.
pub fn sync_web(serial: &str) -> anyhow::Result<usize> {
    let domains = Store::load().web_domains(serial);
    if domains.is_empty() {
        crate::device::web_off(serial)?;
    } else {
        crate::device::web_on(serial, &domains)?;
    }
    Ok(domains.iter().filter(|d| !d.starts_with('!')).count())
}
