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
