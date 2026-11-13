// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Хранилище высот итогов.
//!
//! Слой RustGIS высоты Z не держит. Итог инструмента с высотой (тела,
//! линии, точки) расширение кладёт в проект планом, а геометрию с высотой
//! хранит здесь: файл `<папка>/<слой>.json` с первыми точками плана
//! и геометриями (градусы с Z, рядом метры и зона). Слой узнаётся по
//! имени и первым точкам объектов: инструмент, получивший такой слой,
//! и сцена берут геометрию с высотой. Переименованный или правленный слой
//! высоты теряет.

use std::path::{Path, PathBuf};

use serde_json::{Value as J, json};

use crate::geom::first_point;

/// Хранилище в папке `dir`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Имя файла слоя: имя без знаков, которых не любят папки.
    fn file(&self, layer: &str) -> PathBuf {
        let safe: String = layer
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.dir.join(format!("{safe}.json"))
    }

    /// Сохранить 3D итога слоя `layer`: план, который ушёл в RustGIS
    /// (объекты GeoJSON), и геометрии с высотой по тем же объектам. Не
    /// вышло - сцена покажет план, расчёт от этого не страдает.
    pub fn keep(&self, layer: &str, plan: &[J], geoms: Vec<J>) {
        let firsts: Vec<J> = plan.iter().map(|f| first_point(&f["geometry"])).collect();
        let doc = json!({"plan": firsts, "geoms": geoms});
        let _ = std::fs::create_dir_all(&self.dir);
        let _ = std::fs::write(self.file(layer), doc.to_string());
    }

    /// Есть ли у слоя 3D итога.
    pub fn has(&self, layer: &str) -> bool {
        self.file(layer).is_file()
    }

    /// Слой проекта `fc` (FeatureCollection RustGIS): если это итог (то же
    /// имя, те же первые точки объектов), геометрии - с высотой, поля -
    /// слоя RustGIS. Иначе слой как есть.
    pub fn with_3d(&self, layer: &str, fc: J) -> J {
        let Ok(text) = std::fs::read_to_string(self.file(layer)) else {
            return fc;
        };
        let Ok(doc) = serde_json::from_str::<J>(&text) else {
            return fc;
        };
        let feats = fc["features"].as_array().cloned().unwrap_or_default();
        let plan = doc["plan"].as_array().cloned().unwrap_or_default();
        let close = |a: &J, b: &J| match (a.as_array(), b.as_array()) {
            (Some(a), Some(b)) if a.len() >= 2 && b.len() >= 2 => (0..2).all(|i| {
                let (x, y) = (a[i].as_f64(), b[i].as_f64());
                x.zip(y).is_some_and(|(x, y)| (x - y).abs() < 1e-7)
            }),
            _ => false,
        };
        let same = feats.len() == plan.len()
            && feats
                .iter()
                .zip(&plan)
                .all(|(f, p)| close(&first_point(&f["geometry"]), p));
        if !same {
            return fc;
        }
        let geoms = doc["geoms"].as_array().cloned().unwrap_or_default();
        let features: Vec<J> = feats
            .into_iter()
            .zip(geoms)
            .map(|(mut f, g)| {
                if !g.is_null() {
                    f["geometry"] = g;
                }
                f
            })
            .collect();
        let mut out = fc;
        out["features"] = J::Array(features);
        out
    }

    /// Папка рядом с хранилищем: пакеты объёмных итогов и прочее.
    pub fn beside(&self, name: &str) -> PathBuf {
        self.dir
            .parent()
            .map_or_else(|| PathBuf::from(name), |p| p.join(name))
    }
}

/// Папка данных продукта во временной папке системы: `<temp>/<name>`.
/// В тестах - своя папка на процесс, живые данные не задеты.
pub fn temp_home(name: &str) -> PathBuf {
    let dir = if cfg!(test) {
        format!("{name}-test-{}", std::process::id())
    } else {
        name.to_string()
    };
    std::env::temp_dir().join(dir)
}

/// Путь `p` в той же папке с другим расширением.
pub fn sibling(p: &Path, ext: &str) -> PathBuf {
    p.with_extension(ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_and_find() {
        let s = Store::new(temp_home("rustgis-kit-store").join("bodies"));
        let plan = vec![json!({"type": "Feature", "properties": {"a": 1},
            "geometry": {"type": "Point", "coordinates": [56.0, 54.7]}})];
        s.keep(
            "Итог: тела",
            &plan,
            vec![json!({"type": "Point", "coordinates": [56.0, 54.7, -5.0]})],
        );
        assert!(s.has("Итог: тела"));
        let fc = json!({"type": "FeatureCollection", "features": plan});
        let got = s.with_3d("Итог: тела", fc.clone());
        assert_eq!(got["features"][0]["geometry"]["coordinates"][2], -5.0);
        assert_eq!(got["features"][0]["properties"]["a"], 1);
        // другое имя или другие объекты - слой как есть
        assert_eq!(s.with_3d("Другой", fc.clone()), fc);
        let moved = json!({"type": "FeatureCollection", "features": [
            {"type": "Feature", "geometry": {"type": "Point", "coordinates": [10.0, 10.0]}}]});
        assert_eq!(s.with_3d("Итог: тела", moved.clone()), moved);
        let _ = std::fs::remove_dir_all(temp_home("rustgis-kit-store"));
    }
}
