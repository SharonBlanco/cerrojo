//! Las apps que Cerrojo conoce. Cada una puede tener varios paquetes
//! (versión Lite, versión de otra región, clientes parcheados…): bloquearla
//! bloquea todos.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct App {
    pub name: String,
    pub hint: String,
    pub packages: Vec<String>,
}

fn app(name: &str, hint: &str, packages: &[&str]) -> App {
    App {
        name: name.into(),
        hint: hint.into(),
        packages: packages.iter().map(|p| p.to_string()).collect(),
    }
}

pub fn builtin() -> Vec<App> {
    vec![
        app("YouTube", "Shorts", &["com.google.android.youtube"]),
        app("YouTube Music", "Samples", &["com.google.android.apps.youtube.music"]),
        app(
            "YouTube parcheado",
            "Morphe, ReVanced",
            &[
                "app.morphe.android.youtube",
                "app.morphe.android.apps.youtube.music",
                "app.morphe.manager",
                "app.revanced.android.youtube",
                "app.revanced.android.apps.youtube.music",
                "app.revanced.manager.flutter",
            ],
        ),
        app("TikTok", "Para ti", &["com.zhiliaoapp.musically", "com.zhiliaoapp.musically.go", "com.ss.android.ugc.trill"]),
        app("Instagram", "Reels", &["com.instagram.android", "com.instagram.lite"]),
        app("Threads", "Videos en el feed", &["com.instagram.barcelona"]),
        app("Facebook", "Reels", &["com.facebook.katana", "com.facebook.lite"]),
        app("Snapchat", "Spotlight", &["com.snapchat.android"]),
        app("Pinterest", "Videos", &["com.pinterest"]),
        app("Kwai", "Videos cortos", &["com.kwai.video"]),
        app("Likee", "Videos cortos", &["video.like"]),
        app("Snaptube", "Cliente de YouTube", &["com.snaptube.premium"]),
    ]
}

pub fn config_dir() -> PathBuf {
    if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(x).join("cerrojo");
    }
    if cfg!(windows) {
        if let Some(app) = std::env::var_os("APPDATA") {
            return PathBuf::from(app).join("Cerrojo");
        }
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).unwrap_or_default();
    if cfg!(target_os = "macos") {
        return PathBuf::from(home).join("Library/Application Support/Cerrojo");
    }
    PathBuf::from(home).join(".config/cerrojo")
}

/// Las del catálogo más las que añadiste tú (~/.config/cerrojo/extra.json).
pub fn all() -> Vec<App> {
    let mut apps = builtin();
    apps.extend(load_extra());
    apps
}

pub fn load_extra() -> Vec<App> {
    std::fs::read(config_dir().join("extra.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn add_extra(package: &str, name: &str) -> anyhow::Result<()> {
    let mut extra = load_extra();
    if all().iter().any(|a| a.packages.iter().any(|p| p == package)) {
        return Ok(());
    }
    extra.push(app(name, package, &[package]));
    std::fs::create_dir_all(config_dir())?;
    std::fs::write(config_dir().join("extra.json"), serde_json::to_vec_pretty(&extra)?)?;
    Ok(())
}
