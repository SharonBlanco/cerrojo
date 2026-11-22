//! Cerrojo: bloquea en el celular las apps de reels y video vertical, de
//! forma que solo se pueden volver a activar desde el PC.
//!
//! Sin argumentos abre la ventana. También:
//!   cerrojo estado [SERIAL]
//!   cerrojo bloquear PAQUETE... [-s SERIAL]
//!   cerrojo desbloquear PAQUETE... [-s SERIAL]
//!   cerrojo web [SERIAL]            (pone el filtro web como en la ventana)
//!   cerrojo web apagar|encender [-s SERIAL]
//!   cerrojo solo youtube|instagram si|no [-s SERIAL]   (solo sin Shorts/Reels)
//!   cerrojo apk PAQUETE ARCHIVO     (solo genera el candado)

// En Windows, sin consola al abrir la ventana; desde una terminal se engancha
// a la de la terminal para que las órdenes impriman (ver attach_console).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod apk;
mod catalog;
mod device;
mod gui;
mod keys;
mod store;

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        return gui::run();
    };
    attach_console();
    let mut serial = None;
    let mut rest = Vec::new();
    let mut it = args[1..].iter();
    while let Some(a) = it.next() {
        if a == "-s" {
            serial = it.next().cloned();
        } else {
            rest.push(a.clone());
        }
    }
    let pick = |serial: Option<String>| -> Result<String> {
        if let Some(s) = serial {
            return Ok(s);
        }
        let (list, _) = device::devices();
        match list.as_slice() {
            [d] => Ok(d.serial.clone()),
            [] => bail!("no hay ningún celular conectado por adb"),
            _ => bail!("hay varios equipos: elige uno con -s SERIAL"),
        }
    };
    match cmd.as_str() {
        "apk" if rest.len() == 2 => std::fs::write(&rest[1], apk::build(&rest[0])?)?,
        "estado" => {
            let serial = pick(serial.or(rest.first().cloned()))?;
            for app in catalog::all() {
                let pkgs: Vec<&str> = app.packages.iter().map(String::as_str).collect();
                let st = device::states(&serial, &pkgs)?;
                for p in &app.packages {
                    println!("{:<20} {:<40} {:?}", app.name, p, st.get(p).copied().unwrap_or(device::PkgState::Absent));
                }
            }
            println!("vigía de Shorts y Reels: {}", if device::guard_state(&serial)? { "encendido" } else { "apagado" });
        }
        "solo" if rest.len() == 2 => {
            let serial = pick(serial)?;
            let Some(app) = catalog::all().into_iter().find(|a| a.partial.as_ref().is_some_and(|p| p.key == rest[0])) else {
                bail!("«solo» sirve para: youtube, instagram, whatsapp, linkedin");
            };
            let on = matches!(rest[1].as_str(), "si" | "sí" | "on");
            if !on && app.partial.as_ref().is_some_and(|p| p.only) {
                bail!("{} no se bloquea entera; para que vuelvan sus historias: cerrojo desbloquear", app.name);
            }
            let mut st = store::Store::load();
            st.set_partial(&serial, &app.name, on);
            st.set(&serial, &app.name, true);
            let pp = app.partial.as_ref().unwrap();
            for p in &app.packages {
                let r = if on && pp.packages.contains(p) { device::unblock(&serial, p) } else { device::block(&serial, p) };
                if let Err(e) = r {
                    println!("{p}: {e:#}");
                }
            }
            store::sync_web(&serial)?;
            println!(
                "{}: {}; vigía {}",
                app.name,
                if on { format!("solo sin {}", pp.what) } else { "bloqueada entera".into() },
                if device::guard_state(&serial)? { "encendido" } else { "apagado" }
            );
        }
        "bloquear" | "desbloquear" if !rest.is_empty() => {
            let serial = pick(serial)?;
            for p in &rest {
                let r = if cmd == "bloquear" { device::block(&serial, p) } else { device::unblock(&serial, p) };
                match r {
                    Ok(()) => println!("{p}: ok"),
                    Err(e) => println!("{p}: {e:#}"),
                }
            }
        }
        "web" => {
            let action = rest.iter().find(|a| *a == "apagar" || *a == "encender").cloned();
            let serial = pick(serial.or(rest.iter().find(|a| Some(*a) != action.as_ref()).cloned()))?;
            if let Some(a) = action {
                let mut st = store::Store::load();
                st.set_web(&serial, a == "encender");
            }
            let n = store::sync_web(&serial)?;
            // La VPN tarda un instante en levantarse.
            std::thread::sleep(std::time::Duration::from_millis(3500));
            println!("filtro web: {n} dominios bloqueados; estado {:?}", device::web_state(&serial)?);
            println!("vigía de Shorts y Reels: {}", if device::guard_state(&serial)? { "encendido" } else { "apagado" });
        }
        "--ventana" | "--gui" => return gui::run(),
        _ => {
            eprintln!("uso: cerrojo [estado | bloquear PAQ... | desbloquear PAQ... | web | solo APP si|no | apk PAQ ARCHIVO] [-s SERIAL]");
            std::process::exit(2);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn attach_console() {
    unsafe extern "system" {
        fn AttachConsole(pid: u32) -> i32;
    }
    // ATTACH_PARENT_PROCESS
    unsafe { AttachConsole(u32::MAX) };
}

#[cfg(not(windows))]
fn attach_console() {}
