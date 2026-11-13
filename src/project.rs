// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Слои проекта RustGIS для окна продукта (сцена, глобус) и их система
//! координат.
//!
//! Система - как у карты (решение автора от 13 ноября 2026 года): выбор
//! пользователя в окне продукта (файл с кодом EPSG), иначе проекция карты
//! RustGIS (`host.view`, `"EPSG:n"`), иначе None - зона UTM по середине
//! слоёв. Проекции карты `mercator` и `globe` - не система для метров.
//!
//! Слои в систему переводит сам RustGIS: `share.exportLayer` с `crs`
//! пишет Shapefile ([`crate::shp`]). Система в градусах, итог с высотой
//! в слоях или неудачная выгрузка - слои в градусах `host.features`,
//! продукт переводит их через зону UTM ([`crate::geo`]).

use std::path::Path;

use serde_json::{Value as J, json};

use crate::geo::epsg;
use crate::host::{Call, slash};
use crate::store::Store;

/// Слои проекта и их система: `crs` - код системы, в метрах которой
/// лежат объекты, None - объекты в градусах WGS 84.
#[derive(Debug, Clone, PartialEq)]
pub struct Layers {
    /// (имя слоя, FeatureCollection) - видимые векторные, сверху вниз.
    pub list: Vec<(String, J)>,
    pub crs: Option<String>,
}

/// Система для окна: файл выбора `chosen`, иначе проекция карты RustGIS.
pub fn scene_crs(host: &mut dyn Call, chosen: &Path) -> Option<String> {
    if let Some(c) = std::fs::read_to_string(chosen).ok().and_then(|t| epsg(&t)) {
        return Some(c);
    }
    let view = host.call("host.view", json!({})).ok()?;
    view["projection"].as_str().and_then(epsg)
}

/// Все координаты в пределах градусов: слой в долготе и широте.
pub fn degrees(fc: &J) -> bool {
    let mut all = true;
    let mut any = false;
    for f in fc["features"].as_array().into_iter().flatten() {
        for p in crate::geom::parts(&f["geometry"]).iter().flatten() {
            any = true;
            all &= p[0].abs() <= 180.0 && p[1].abs() <= 90.0;
        }
    }
    any && all
}

/// Слои `ids` в метрах системы `target` выгрузкой RustGIS в папку `dir`.
/// None - система в градусах или выгрузка не удалась.
pub fn export_layers(
    host: &mut dyn Call,
    ids: &[(i64, String)],
    target: &str,
    dir: &Path,
) -> Option<Vec<(String, J)>> {
    std::fs::create_dir_all(dir).ok()?;
    let mut out = Vec::new();
    for (id, name) in ids {
        let shp = dir.join(format!("layer-{id}.shp"));
        for ext in ["shp", "shx", "dbf", "prj", "cpg"] {
            let _ = std::fs::remove_file(shp.with_extension(ext));
        }
        host.call(
            "share.exportLayer",
            json!({"layer": id, "path": slash(&shp), "crs": target}),
        )
        .ok()?;
        let fc = std::fs::read(&shp)
            .ok()
            .and_then(|b| crate::shp::read(&b).ok())?;
        // система в градусах - не метры
        if out.is_empty() && degrees(&fc) {
            return None;
        }
        out.push((name.clone(), fc));
    }
    Some(out)
}

/// Видимые векторные слои проекта для окна продукта.
///
/// `store` - хранилище высот итогов: такой слой идёт с высотой и держит
/// окно в градусах (тела в градусах в другую систему перевести нечем).
/// `chosen` - файл выбора системы окна, `export_dir` - папка выгрузки.
pub fn project_layers(
    host: &mut dyn Call,
    store: &Store,
    chosen: &Path,
    export_dir: &Path,
) -> Result<Layers, String> {
    let list = host.call("host.layers", json!({}))?;
    let mut out = Vec::new();
    let mut ids = Vec::new();
    let mut solids = false;
    for l in list.as_array().into_iter().flatten() {
        let visible = l["visible"].as_bool().unwrap_or(true);
        let raster = l["raster"].as_bool().unwrap_or(false);
        let Some(id) = l["id"].as_i64() else { continue };
        if !visible || raster {
            continue;
        }
        let fc = host.call("host.features", json!({"layer": id}))?;
        let name = l["name"].as_str().unwrap_or("").to_string();
        let with = store.with_3d(&name, fc.clone());
        solids |= with != fc;
        ids.push((id, name.clone()));
        out.push((name, with));
    }
    let target = if solids || ids.is_empty() {
        None
    } else {
        scene_crs(host, chosen)
    };
    if let Some(target) = target
        && let Some(list) = export_layers(host, &ids, &target, export_dir)
    {
        return Ok(Layers {
            list,
            crs: Some(target),
        });
    }
    Ok(Layers {
        list: out,
        crs: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RustGIS по сценарию: ответы на запросы по порядку.
    struct Script(Vec<J>, Vec<String>);

    impl Call for Script {
        fn call(&mut self, op: &str, _params: J) -> Result<J, String> {
            self.1.push(op.to_string());
            if self.0.is_empty() {
                return Err("closed".into());
            }
            Ok(self.0.remove(0))
        }
    }

    /// Карта в Меркаторе: слои в градусах, без выгрузки. Растр и скрытый
    /// слой не идут.
    #[test]
    fn layers_in_degrees_for_a_mercator_map() {
        let fc = json!({"type": "FeatureCollection", "features": [
            {"type": "Feature", "geometry": {"type": "Point", "coordinates": [56.0, 54.7]}}]});
        let mut s = Script(
            vec![
                json!([{"id": 1, "name": "a", "visible": true},
                       {"id": 2, "name": "r", "visible": true, "raster": true},
                       {"id": 3, "name": "h", "visible": false}]),
                fc.clone(),
                json!({"projection": "mercator"}),
            ],
            Vec::new(),
        );
        let dir = crate::store::temp_home("rustgis-kit-project");
        let store = Store::new(dir.join("bodies"));
        let l = project_layers(&mut s, &store, &dir.join("none.crs"), &dir.join("export"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(l.crs, None);
        assert_eq!(l.list, vec![("a".to_string(), fc.clone())]);
        assert_eq!(s.1, ["host.layers", "host.features", "host.view"]);
        assert!(degrees(&fc));
        // выбор в окне важнее карты
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(dir.join("pick.crs"), "28410").ok();
        let mut s = Script(Vec::new(), Vec::new());
        assert_eq!(
            scene_crs(&mut s, &dir.join("pick.crs")).as_deref(),
            Some("EPSG:28410")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
