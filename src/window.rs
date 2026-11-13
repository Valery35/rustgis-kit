// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Окно продукта (сцена, глобус) своим процессом рядом с RustGIS.
//!
//! RustGIS останавливает расширение-процесс вскоре после ответа, окно
//! должно его пережить: [`spawn_detached`] запускает его без консоли, вне
//! задания RustGIS, если задание это разрешает (как глобус RustPlanetX).
//!
//! Замок окна - файл `<пакет>.lock`, который окно держит открытым без
//! общего доступа, пока живо ([`hold_lock`]). Пока замок занят
//! ([`window_open_for`]), команда только переписывает файл данных окна,
//! окно перечитывает его само, второе окно не открывается. Замок есть
//! только у Windows, на других системах окно открывается каждый раз.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Запустить программу отдельным процессом без консоли и вне задания
/// RustGIS. Сначала с CREATE_BREAKAWAY_FROM_JOB, не вышло - без него.
pub fn spawn_detached(exe: &Path, args: &[std::ffi::OsString]) -> Result<(), String> {
    let spawn = |flags: u32| {
        let mut cmd = Command::new(exe);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(flags);
        }
        #[cfg(not(windows))]
        let _ = flags;
        cmd.spawn().map(|_| ())
    };
    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP, сначала
    // с CREATE_BREAKAWAY_FROM_JOB
    const DETACHED: u32 = 0x0000_0008 | 0x0000_0200;
    const BREAKAWAY: u32 = 0x0100_0000;
    spawn(DETACHED | BREAKAWAY)
        .or_else(|_| spawn(DETACHED))
        .map_err(|e| e.to_string())
}

/// Файл замка окна с данными `data`.
pub fn lock_path(data: &Path) -> PathBuf {
    data.with_extension("lock")
}

/// Открыто ли окно с данными `data`: замок занят.
#[cfg(windows)]
pub fn window_open_for(data: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    let path = lock_path(data);
    path.exists()
        && std::fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(&path)
            .is_err()
}

#[cfg(not(windows))]
pub fn window_open_for(_data: &Path) -> bool {
    false
}

/// Занять замок окна с данными `data` на время жизни окна. None - не
/// вышло, окно живёт без замка.
#[cfg(windows)]
pub fn hold_lock(data: &Path) -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    let path = lock_path(data);
    std::fs::create_dir_all(path.parent()?).ok()?;
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .share_mode(0)
        .open(path)
        .ok()
}

#[cfg(not(windows))]
pub fn hold_lock(_data: &Path) -> Option<std::fs::File> {
    None
}

/// Записать файл заменой: сначала рядом `.part`, потом переименование,
/// чтобы окно не прочло его наполовину.
pub fn write_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    std::fs::write(&part, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_while_held() {
        let dir = std::env::temp_dir().join(format!("rustgis-kit-lock-{}", std::process::id()));
        let data = dir.join("scene.pkg");
        assert!(!window_open_for(&data));
        let held = hold_lock(&data);
        if cfg!(windows) {
            assert!(held.is_some() && window_open_for(&data));
        }
        drop(held);
        assert!(!window_open_for(&data));
        assert!(write_replace(&data, b"x").is_ok());
        assert_eq!(std::fs::read(&data).ok().as_deref(), Some(&b"x"[..]));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
