// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Минимальное расширение RustGIS на одном инструменте: «9.01 Длина
//! линий». Слой линий на входе, на выходе тот же слой с полем `length` -
//! длиной в метрах.
//!
//! - `cargo run --example minimal -- manifest` - `plugin.json` с разделом
//!   `tools`, собранным из описания инструмента;
//! - `cargo run --example minimal -- rustgis-plugin` - то, что запускает
//!   RustGIS: сессия или одна команда на вводе и выводе;
//! - `cargo test --example minimal` - прогон инструмента запросом RustGIS.

use std::process::ExitCode;

use rustgis_kit::catalog::Tool;
use rustgis_kit::runner::no_command;
use rustgis_kit::{Call, Catalog, Failure, Lang, Options, Product, Store};
use serde_json::{Value as J, json};

/// Описание инструмента - как `tools.json` модуля QGIS
/// (`tools/export_tools.py`). У настоящего продукта это файл модуля.
const TOOLS_JSON: &str = r#"{
  "format": 1,
  "module": "minimal",
  "version": "0.1.0",
  "tools": [{
    "code": "9.01",
    "name": "line_length",
    "title": {"ru": "Длина линий", "en": "Line length"},
    "help": {"ru": "Длина каждой линии в метрах. Поле length.",
             "en": "The length of each line in metres. The length field."},
    "params": [
      {"key": "INPUT", "kind": "lines",
       "label": {"ru": "Линии", "en": "Lines"},
       "hint": {"ru": "Слой линий проекта.", "en": "A line layer of the project."}},
      {"key": "OUTPUT", "kind": "sink",
       "label": {"ru": "Длины линий", "en": "Line lengths"}}
    ],
    "messages": [{"ru": "Линий: %d.", "en": "Lines: %d."}]
  }]
}"#;

/// Основа `plugin.json`: всё, кроме раздела `tools`.
const MANIFEST: &str = r#"{
  "id": "minimal",
  "name": "Minimal",
  "version": "0.1.0",
  "description": "One tool: line length.",
  "runtime": "process",
  "session": true,
  "run": { "windows": ["bin/minimal.exe", "rustgis-plugin"] },
  "minRustgisVersion": "0.5.0",
  "commands": [
    { "id": "about", "label": "About", "help": "About the extension." }
  ],
  "i18n": {
    "ru": {
      "About": "О расширении",
      "About the extension.": "О расширении."
    }
  }
}
"#;

const OPTIONS: Options = Options {
    toolbox: "Minimal",
    offered: &["line_length"],
    xy_tables: &[],
};

struct Minimal {
    catalog: Catalog,
}

impl Product for Minimal {
    fn catalog(&self) -> &Catalog {
        &self.catalog
    }

    fn options(&self) -> &Options {
        &OPTIONS
    }

    /// Высоты итогов: у этого инструмента итог без своей геометрии, но
    /// хранилище нужно всегда.
    fn store(&self) -> Store {
        Store::new(rustgis_kit::store::temp_home("minimal").join("bodies"))
    }

    /// Значения - в формате прогонов модуля: линии
    /// `[{"id", "parts": [[[x, y, z]]], "attrs"}]` в метрах зоны UTM.
    /// Выход - по ключу `OUTPUT`: объекты источника с полем `length`.
    fn compute(&self, tool: &Tool, values: &J, lang: Lang) -> Result<(J, Vec<String>), Failure> {
        let Some(lines) = values["INPUT"].as_array() else {
            return Err(Failure::Values("INPUT".into()));
        };
        let features: Vec<J> = lines
            .iter()
            .map(|f| {
                let length: f64 = f["parts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(J::as_array)
                    .map(|part| {
                        part.windows(2)
                            .map(|w| {
                                let d = |i: usize| {
                                    w[1][i].as_f64().unwrap_or(0.0)
                                        - w[0][i].as_f64().unwrap_or(0.0)
                                };
                                d(0).hypot(d(1))
                            })
                            .sum::<f64>()
                    })
                    .sum();
                let id = f["id"].as_i64().unwrap_or(0);
                json!({"source": id, "values": [length], "geometry": {"source": id}})
            })
            .collect();
        let message = tool
            .translate("Линий: %d.", lang)
            .replace("%d", &features.len().to_string());
        let out = json!({"OUTPUT": {
            "geometry": "source",
            "keep_source": "INPUT",
            "fields": [{"name": "length", "type": "double"}],
            "features": features,
        }});
        Ok((out, vec![message]))
    }

    /// Свои команды: только «О расширении».
    fn command(&self, _host: &mut dyn Call, req: &J, lang: Lang) -> J {
        match req["id"].as_str().unwrap_or("") {
            "about" => json!({"ok": {"title": "Minimal", "version": env!("CARGO_PKG_VERSION")}}),
            id => no_command(id, lang),
        }
    }
}

/// `plugin.json` с разделом `tools` и переводами его строк.
fn manifest(catalog: &Catalog) -> Result<String, String> {
    let (tools, ru) = rustgis_kit::manifest::tools_section(catalog, &OPTIONS);
    rustgis_kit::manifest::splice(MANIFEST, &tools, &ru)
}

fn main() -> ExitCode {
    let catalog = match Catalog::parse(TOOLS_JSON) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("tools.json: {e}");
            return ExitCode::FAILURE;
        }
    };
    match std::env::args().nth(1).as_deref() {
        Some("rustgis-plugin") => rustgis_kit::serve_stdio(&Minimal { catalog }),
        Some("manifest") => match manifest(&catalog) {
            Ok(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("minimal rustgis-plugin | minimal manifest");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Запрос RustGIS одним запуском: слой в градусах, ответ - тот же слой
    /// с длиной в метрах.
    #[test]
    fn line_length_through_rustgis() {
        let p = Minimal {
            catalog: Catalog::parse(TOOLS_JSON).unwrap_or_else(|e| unreachable!("{e}")),
        };
        // 0.001 градуса долготы на широте 54.7 - около 64.4 м
        let fc = json!({"type": "FeatureCollection", "features": [{"type": "Feature",
            "properties": {"name": "a"},
            "geometry": {"type": "LineString", "coordinates": [[56.0, 54.7], [56.001, 54.7]]}}]});
        let req = json!({"abi": 1, "kind": "tool", "id": "line-length", "lang": "ru",
            "params": {"input": "Дороги"}, "layers": {"Дороги": fc}});
        let mut out = Vec::new();
        let input = format!("{req}\n");
        rustgis_kit::serve(&p, input.as_bytes(), &mut out).unwrap_or_else(|e| unreachable!("{e}"));
        let reply: J = serde_json::from_slice(&out).unwrap_or_else(|e| unreachable!("{e}"));
        let layer = &reply["ok"]["layers"][0];
        assert_eq!(layer["name"], "Длины линий");
        let f = &layer["geojson"]["features"][0];
        let length = f["properties"]["length"].as_f64().unwrap_or(0.0);
        assert!((length - 64.4).abs() < 0.1, "{length}");
        assert_eq!(f["geometry"]["type"], "LineString");
        assert_eq!(reply["ok"]["messages"][0], "Линий: 1.");
        // раздел tools манифеста собирается из описания
        let m: J =
            serde_json::from_str(&manifest(&p.catalog).unwrap_or_default()).unwrap_or_default();
        assert_eq!(m["tools"][0]["id"], "line-length");
        assert_eq!(m["tools"][0]["params"][0]["kind"], "vectorLayer");
    }
}
