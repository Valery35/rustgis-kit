// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Ретранслятор MCP RustGIS: живая связь без сессий расширений.
//!
//! RustGIS 0.6.0 держит ретранслятор, когда пользователь включил команду
//! «MCP Relay» (Ctrl+K) или флажок «Let your Agents Control RustGIS»
//! в настройках помощника: 127.0.0.1, порт [`DEFAULT_PORT`] или
//! `RUSTGIS_MCP_PORT`, ключ - файл [`token_path`], его пишет RustGIS.
//! Первая строка - `rustgis/attach` с ключом, дальше - сообщения MCP
//! JSON-RPC по строке. Так связан глобус RustPlanetX
//! (`planetx-lite`, `ui::rustgislink::Relay`).
//!
//! Что даёт сверх протокола расширений:
//!
//! - любую команду RustGIS (`run_command`), в том числе `share.exportLayer`
//!   с кодом системы слоя, - и процессу инструмента, которому запросы
//!   к RustGIS по протоколу расширений запрещены;
//! - список слоёв (`list_layers`) - окно продукта может следить за слоями
//!   опросом. Событий ретранслятор не шлёт.
//!
//! [`Relay`] - тот же [`Call`], что сессия: `host.layers`,
//! `host.features` (через `data.convert` во временный файл) и любая
//! команда. Ретранслятор выключен - [`Relay::open`] отвечает
//! [`RELAY_OFF`], продукт работает без него.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use serde_json::{Value as J, json};

use crate::host::{Call, slash};

/// Причина «нет связи», когда файла ключа нет: ретранслятор не включён.
pub const RELAY_OFF: &str = "the MCP relay of RustGIS is off";
/// Порт ретранслятора RustGIS по умолчанию.
pub const DEFAULT_PORT: u16 = 8767;
/// Секунд на ответ RustGIS.
const REPLY_SECONDS: u64 = 60;
/// Строка ответа не длиннее, байт.
const MAX_LINE: u64 = 64 << 20;
/// Файл объектов слоя не больше, байт.
const MAX_FILE: u64 = 512 << 20;

/// Порт: `RUSTGIS_MCP_PORT` или [`DEFAULT_PORT`], как у RustGIS.
pub fn port() -> u16 {
    std::env::var("RUSTGIS_MCP_PORT")
        .ok()
        .and_then(|p| p.trim().parse().ok())
        .filter(|p| *p > 0)
        .unwrap_or(DEFAULT_PORT)
}

/// Файл ключа ретранслятора на порту `port` - как `token_path` RustGIS:
/// папка `rustgis-mcp-<пользователь>` во временной папке системы или
/// `$XDG_RUNTIME_DIR/rustgis`.
pub fn token_path(port: u16) -> PathBuf {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_dir())
    {
        return runtime.join("rustgis").join(format!("{port}.token"));
    }
    let user: String = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let user = if user.is_empty() {
        "user".to_string()
    } else {
        user
    };
    std::env::temp_dir()
        .join(format!("rustgis-mcp-{user}"))
        .join(format!("{port}.token"))
}

/// Включён ли ретранслятор: файл ключа есть.
pub fn available() -> bool {
    token_path(port()).is_file()
}

/// Соединение MCP с ретранслятором.
pub struct Relay {
    writer: TcpStream,
    reader: BufReader<TcpStream>,
    next: u64,
    /// Папка временных файлов объектов слоя.
    temp: PathBuf,
}

impl Relay {
    /// Подключиться к ретранслятору на `port` с ключом из [`token_path`].
    /// `client` - имя программы для RustGIS, `temp` - папка временных
    /// файлов объектов слоя.
    pub fn open(port: u16, client: &str, temp: PathBuf) -> Result<Self, String> {
        let token = std::fs::read_to_string(token_path(port)).map_err(|_| RELAY_OFF.to_string())?;
        Self::connect(port, token.trim(), client, temp)
    }

    /// Подключиться с данным ключом.
    pub fn connect(port: u16, token: &str, client: &str, temp: PathBuf) -> Result<Self, String> {
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
            .map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(REPLY_SECONDS)))
            .map_err(|e| e.to_string())?;
        let _ = stream.set_nodelay(true);
        let writer = stream.try_clone().map_err(|e| e.to_string())?;
        let mut s = Self {
            writer,
            reader: BufReader::new(stream),
            next: 1,
            temp,
        };
        // access - чтение и запись файлов: объекты слоя идут временным
        // файлом, ответ команды длиной ограничен
        s.send(&json!({"jsonrpc": "2.0", "method": "rustgis/attach",
            "params": {"access": true, "token": token, "timeout_s": REPLY_SECONDS}}))?;
        s.request(
            "initialize",
            json!({"protocolVersion": "2025-06-18", "capabilities": {},
                   "clientInfo": {"name": client, "version": env!("CARGO_PKG_VERSION")}}),
        )?;
        s.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(s)
    }

    fn send(&mut self, msg: &J) -> Result<(), String> {
        let mut line = msg.to_string();
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .map_err(|e| e.to_string())
    }

    /// Запрос и его ответ. Уведомления без номера пропускаются.
    fn request(&mut self, method: &str, params: J) -> Result<J, String> {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))?;
        loop {
            let mut line = String::new();
            let n = (&mut self.reader)
                .take(MAX_LINE)
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("RustGIS closed the connection".to_string());
            }
            let Ok(reply) = serde_json::from_str::<J>(&line) else {
                continue;
            };
            if reply["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(e) = reply.get("error") {
                return Err(e["message"].as_str().unwrap_or("error").to_string());
            }
            return Ok(reply["result"].clone());
        }
    }

    /// Инструмент MCP: текст ответа. Ответ с `isError` - ошибка с текстом.
    pub fn tool(&mut self, name: &str, arguments: J) -> Result<String, String> {
        let result = self.request("tools/call", json!({"name": name, "arguments": arguments}))?;
        let text = result["content"]
            .as_array()
            .and_then(|c| c.iter().find_map(|p| p["text"].as_str()))
            .unwrap_or("")
            .to_string();
        if result["isError"] == true {
            Err(text)
        } else {
            Ok(text)
        }
    }

    /// Команда RustGIS: ответ JSON (текст не JSON - строкой).
    pub fn command(&mut self, command: &str, params: J) -> Result<J, String> {
        let text = self.tool("run_command", json!({"command": command, "params": params}))?;
        Ok(serde_json::from_str(&text).unwrap_or(J::String(text)))
    }

    /// Слои проекта, как `host.layers` сессии: id, имя, видимость, растр.
    pub fn layers(&mut self) -> Result<J, String> {
        let text = self.tool("list_layers", json!({"include_fields": false}))?;
        let info: J = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        Ok(layers_of(&info))
    }

    /// Объекты слоя `id` GeoJSON в градусах: `data.convert` во временный
    /// файл, файл читается и удаляется.
    pub fn features(&mut self, id: i64) -> Result<J, String> {
        std::fs::create_dir_all(&self.temp).map_err(|e| e.to_string())?;
        let path = self.temp.join(format!("layer-{id}.geojson"));
        self.command(
            "data.convert",
            json!({"layer": id, "format": "geojson", "path": slash(&path)}),
        )?;
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if size > MAX_FILE {
            let _ = std::fs::remove_file(&path);
            return Err("the layer file is too large".to_string());
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string());
        let _ = std::fs::remove_file(&path);
        serde_json::from_slice(&bytes?).map_err(|e| e.to_string())
    }
}

/// Слои из ответа `list_layers` (`info.project`) в виде `host.layers`.
pub fn layers_of(info: &J) -> J {
    J::Array(
        info["layers"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|l| {
                let kind = l["type"].as_str().unwrap_or("");
                json!({"id": l["id"], "name": l["name"], "visible": l["visible"],
                       "raster": kind.eq_ignore_ascii_case("raster")})
            })
            .collect(),
    )
}

impl Call for Relay {
    /// `host.layers` и `host.features` - своими путями, `host.notify` -
    /// без ответа, остальное - командой RustGIS. `host.view` у ретранслятора
    /// нет: продукт берёт зону UTM.
    fn call(&mut self, op: &str, params: J) -> Result<J, String> {
        match op {
            "host.layers" => self.layers(),
            "host.features" => self.features(params["layer"].as_i64().unwrap_or(-1)),
            "host.view" => Err("host.view is not served by the MCP relay".to_string()),
            "host.notify" => Ok(J::Null),
            "host.addLayer" => self.command(
                "layer.addDataText",
                json!({"fileName": "layer.geojson", "text": params["geojson"].to_string(),
                       "name": params["name"]}),
            ),
            _ => self.command(op, params),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// Ретранслятор по сценарию: attach, initialize, ответы на tools/call.
    fn fake(answers: Vec<J>) -> (u16, std::thread::JoinHandle<Vec<J>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap_or_else(|e| unreachable!("{e}"));
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            let Ok((stream, _)) = listener.accept() else {
                return seen;
            };
            let mut writer = stream.try_clone().unwrap_or_else(|e| unreachable!("{e}"));
            let mut reader = BufReader::new(stream);
            let mut answers = answers.into_iter();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let msg: J = serde_json::from_str(&line).unwrap_or(J::Null);
                seen.push(msg.clone());
                let result = match msg["method"].as_str() {
                    Some("initialize") => json!({}),
                    Some("tools/call") => answers.next().unwrap_or(J::Null),
                    _ => continue,
                };
                let reply = json!({"jsonrpc": "2.0", "id": msg["id"], "result": result});
                let _ = writeln!(writer, "{reply}");
            }
            seen
        });
        (port, handle)
    }

    #[test]
    fn relay_calls_and_layers() {
        let info = json!({"layers": [{"id": 1, "name": "a", "visible": true, "type": "Polyline"},
                                     {"id": 2, "name": "dem", "visible": true, "type": "Raster"}]});
        let text = |v: J| json!({"content": [{"type": "text", "text": v.to_string()}]});
        let (port, handle) = fake(vec![
            text(info),
            text(json!({"crs": "EPSG:32637"})),
            json!({"content": [{"type": "text", "text": "no such layer"}], "isError": true}),
        ]);
        let temp = crate::store::temp_home("rustgis-kit-relay");
        let mut r = Relay::connect(port, "secret", "test", temp.clone())
            .unwrap_or_else(|e| unreachable!("{e}"));
        let layers = r.call("host.layers", json!({})).unwrap_or(J::Null);
        assert_eq!(
            layers[0],
            json!({"id": 1, "name": "a", "visible": true, "raster": false})
        );
        assert_eq!(layers[1]["raster"], true);
        let e = r
            .call("share.exportLayer", json!({"layer": 1}))
            .unwrap_or(J::Null);
        assert_eq!(e["crs"], "EPSG:32637");
        assert_eq!(
            r.call("share.exportLayer", json!({"layer": 9}))
                .err()
                .as_deref(),
            Some("no such layer")
        );
        assert!(r.call("host.view", json!({})).is_err());
        drop(r);
        let seen = handle.join().unwrap_or_default();
        assert_eq!(seen[0]["method"], "rustgis/attach");
        assert_eq!(seen[0]["params"]["token"], "secret");
        // attach, initialize, initialized, list_layers, затем команда
        assert_eq!(
            seen[4]["params"]["arguments"]["command"],
            "share.exportLayer"
        );
        let _ = std::fs::remove_dir_all(temp);
    }
}
