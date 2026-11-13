// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Инструмент RustGIS по `tools.json`: запрос - в значения прогона
//! модуля, выходы прогона - в слои и таблицы RustGIS.
//!
//! Запрос RustGIS (docs/plugins.md, «The ABI»):
//! `{"kind": "tool", "id", "params", "layers", "lang"}` - поле-слой несёт
//! имя слоя, его объекты - GeoJSON в `layers`, в градусах WGS 84 и без Z.
//! Ответ - `{"layers": [{"name", "geojson"}], "tables": [...],
//! "messages": [...]}`, RustGIS кладёт их в проект с отменой.
//!
//! [`prepare`] переводит запрос в значения в формате прогонов модуля
//! (`tool_runs.json`: `values` вместе с `input`):
//!
//! - градусы - в метры зоны UTM по середине входа ([`crate::geo`]);
//! - слой-итог (тела, линии, точки с высотой) берёт высоты из хранилища
//!   ([`crate::store`]), поле-тело без итога - отказ с объяснением;
//! - линии и объекты - `[{"id", "parts", "attrs"}]`, у полигона ещё
//!   `polygons` - части с внешним кольцом и дырами, точки и таблица -
//!   `[{"id", "xyz", "attrs"}]`, тело - `{"verts", "faces"}`, слой тел -
//!   `[{"id", "attrs", "verts", "faces"}]`, сведения о слое - `"@КЛЮЧ"`;
//! - пункт списка - номер, несколько пунктов - список номеров, число,
//!   поле, текст, флажок - как есть.
//!
//! Продукт считает по этим значениям и отдаёт выходы в формате `output`
//! прогонов модуля, [`finish`] переводит их в ответ RustGIS: тело - планом
//! (верхние грани) с полями расчёта и отметками `z_min`, `z_max`, итог
//! с высотой - в хранилище и в список для окна продукта, таблица без
//! геометрии - в `tables`, куб - точками ячеек в окно продукта.

use std::collections::HashMap;

use serde_json::{Value as J, json};

use crate::catalog::{Catalog, Lang, Param, Tool};
use crate::geo::{Utm, epsg};
use crate::geom::{self, Frame};
use crate::i18n::tr;
use crate::store::Store;

/// Настройки инструментов продукта.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Группа инструментов в панели RustGIS: имя продукта.
    pub toolbox: &'static str,
    /// Инструменты в RustGIS по имени Processing, в порядке панели.
    pub offered: &'static [&'static str],
    /// Слои-таблицы с X и Y в метрах своей системы: (инструмент, ключ
    /// поля). Таблица без геометрии считается в своих метрах, код системы -
    /// поле [`CRS_PARAM`] окна RustGIS, итог уходит в этих метрах.
    pub xy_tables: &'static [(&'static str, &'static str)],
}

/// Поле окна RustGIS, которого нет в `tools.json`: код системы X и Y
/// таблицы (у инструментов из [`Options::xy_tables`]).
pub const CRS_PARAM: &str = "crs";

/// Геометрии входов по ключу поля-слоя и номеру объекта: их берут выходы
/// «объект источника».
pub type Sources = HashMap<String, HashMap<i64, J>>;

/// Запрос, готовый к расчёту.
#[derive(Debug, Clone)]
pub struct Prepared<'a> {
    pub tool: &'a Tool,
    /// Значения в формате прогонов модуля: поля, слои, `@КЛЮЧ`.
    pub values: J,
    /// Система итога.
    pub frame: Frame,
    /// Зона UTM входа.
    pub utm: Utm,
    sources: Sources,
    /// Поля объектов входов (properties) по ключу поля-слоя и номеру: с них
    /// начинается выход с `keep_source`.
    source_props: Sources,
}

/// Номер объекта: `row` RustGIS, иначе `id`, иначе порядковый.
pub fn feature_id(f: &J, i: usize) -> i64 {
    f["row"]
        .as_i64()
        .or_else(|| f["id"].as_i64())
        .unwrap_or(i as i64)
}

/// Зона UTM по середине всех вершин слоёв.
pub fn zone_of(layers: &[&J]) -> Utm {
    let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
    for fc in layers {
        for f in fc["features"].as_array().into_iter().flatten() {
            for p in geom::parts(&f["geometry"]).iter().flatten() {
                if p[0].is_finite() && p[1].is_finite() {
                    sx += p[0];
                    sy += p[1];
                    n += 1.0;
                }
            }
        }
    }
    if n > 0.0 {
        Utm::at(sx / n, sy / n)
    } else {
        Utm::at(0.0, 0.0)
    }
}

/// Слой RustGIS в значение прогона модуля по виду поля, в метрах зоны.
pub fn layer_value(p: &Param, fc: &J, utm: &Utm) -> J {
    let feats = fc["features"].as_array().cloned().unwrap_or_default();
    match p.kind.as_str() {
        // одно тело из всех объектов слоя
        "mesh" => {
            let gs: Vec<&J> = feats.iter().map(|f| &f["geometry"]).collect();
            return geom::body_of(&gs, utm).unwrap_or(J::Null);
        }
        // тело на объект
        "meshes" => {
            return J::Array(
                feats
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let mut o = json!({"id": feature_id(f, i), "attrs": f["properties"]});
                        if let Some(b) = geom::body_of(&[&f["geometry"]], utm) {
                            o["verts"] = b["verts"].clone();
                            o["faces"] = b["faces"].clone();
                        }
                        o
                    })
                    .collect(),
            );
        }
        _ => {}
    }
    let xyz = |pt: [f64; 3]| {
        json!([
            pt[0],
            pt[1],
            if pt[2].is_finite() {
                json!(pt[2])
            } else {
                J::Null
            }
        ])
    };
    J::Array(
        feats
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let id = feature_id(f, i);
                let ps = geom::parts_m(&f["geometry"], utm);
                if matches!(p.kind.as_str(), "points" | "table") {
                    let first = ps.first().and_then(|q| q.first()).copied();
                    json!({"id": id, "attrs": f["properties"], "xyz": first.map(xyz)})
                } else {
                    let parts: Vec<Vec<J>> = ps
                        .iter()
                        .map(|q| q.iter().map(|&pt| xyz(pt)).collect())
                        .collect();
                    let mut o = json!({"id": id, "attrs": f["properties"], "parts": parts});
                    // полигон: части и дыры как в слое, внешнее кольцо первым
                    let polys = geom::polygons_m(&f["geometry"], utm);
                    if !polys.is_empty() {
                        o["polygons"] = json!(
                            polys
                                .iter()
                                .map(|rings| rings
                                    .iter()
                                    .map(|r| r.iter().map(|&pt| xyz(pt)).collect::<Vec<_>>())
                                    .collect::<Vec<_>>())
                                .collect::<Vec<_>>()
                        );
                    }
                    o
                }
            })
            .collect(),
    )
}

/// Сведения о слое `@КЛЮЧ`, как их даёт модуль: имя, геометрия, Z, поля
/// по порядку первого появления, числовые поля.
pub fn layer_info(name: &str, fc: &J) -> J {
    let feats = fc["features"].as_array().cloned().unwrap_or_default();
    let geometry = feats
        .iter()
        .find_map(|f| f["geometry"]["type"].as_str())
        .map_or("none", |t| match t {
            "Point" | "MultiPoint" => "point",
            "LineString" | "MultiLineString" => "line",
            "Polygon" | "MultiPolygon" => "polygon",
            _ => "none",
        });
    let mut fields: Vec<String> = Vec::new();
    let mut text: Vec<String> = Vec::new();
    for f in &feats {
        for (k, v) in f["properties"].as_object().into_iter().flatten() {
            if !fields.contains(k) {
                fields.push(k.clone());
            }
            if v.is_string() && !text.contains(k) {
                text.push(k.clone());
            }
        }
    }
    let numeric: Vec<&String> = fields.iter().filter(|k| !text.contains(k)).collect();
    // Z есть только у итогов из хранилища, слой RustGIS его не держит
    let hasz = feats.iter().any(|f| geom::has_z(&f["geometry"]));
    json!({"name": name, "geometry": geometry, "hasz": hasz, "fields": fields, "numeric": numeric})
}

/// Пункт списка: номер, английский или русский текст пункта.
pub fn choice(p: &Param, v: &J) -> Option<usize> {
    if let Some(i) = v.as_u64() {
        return usize::try_from(i).ok();
    }
    let s = v.as_str()?.trim();
    p.options
        .iter()
        .position(|o| o.en == s || o.ru == s)
        .or_else(|| s.parse().ok())
}

/// Число: число или текст числа (поле RustGIS могло прийти строкой).
pub fn number(v: &J) -> Option<f64> {
    v.as_f64().or_else(|| {
        v.as_str()
            .and_then(|s| s.trim().replace(',', ".").parse().ok())
    })
}

/// Список из текста через запятую или точку с запятой.
fn split(s: &str) -> Vec<String> {
    s.split([',', ';'])
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect()
}

/// Отказ разбора: ключ поля не того вида или слой без тел.
enum Bad {
    Key(String),
    NoBodies(String),
}

/// Значения прогона, зона, геометрии и поля объектов входов.
type Parsed = (J, Utm, Sources, Sources);

fn values(tool: &Tool, req: &J, store: &Store) -> Result<Parsed, Bad> {
    let params = &req["params"];
    let layers = &req["layers"];
    let mut inputs: Vec<(&Param, J)> = Vec::new();
    for p in tool.params.iter().filter(|p| p.is_layer()) {
        let name = params[p.rg_name()].as_str().unwrap_or("");
        if name.is_empty() {
            continue;
        }
        let fc = layers.get(name).ok_or_else(|| Bad::Key(p.key.clone()))?;
        if p.is_mesh() && !store.has(name) {
            return Err(Bad::NoBodies(name.to_string()));
        }
        inputs.push((p, store.with_3d(name, fc.clone())));
    }
    let utm = zone_of(&inputs.iter().map(|(_, fc)| fc).collect::<Vec<_>>());
    let mut out = serde_json::Map::new();
    let mut sources: Sources = HashMap::new();
    let mut source_props: Sources = HashMap::new();
    for (p, fc) in &inputs {
        let name = params[p.rg_name()].as_str().unwrap_or("");
        out.insert(format!("@{}", p.key), layer_info(name, fc));
        out.insert(p.key.clone(), layer_value(p, fc, &utm));
        let by_id = sources.entry(p.key.clone()).or_default();
        let props_by_id = source_props.entry(p.key.clone()).or_default();
        for (i, f) in fc["features"].as_array().into_iter().flatten().enumerate() {
            let id = feature_id(f, i);
            by_id.entry(id).or_insert_with(|| f["geometry"].clone());
            props_by_id
                .entry(id)
                .or_insert_with(|| f["properties"].clone());
        }
    }
    for p in &tool.params {
        let v = &params[p.rg_name()];
        if v.is_null() || p.is_layer() {
            continue;
        }
        let bad = || Bad::Key(p.key.clone());
        let got = match p.kind.as_str() {
            "enum" if p.multiple => {
                let items: Vec<J> = match v {
                    J::Array(a) => a.clone(),
                    J::String(s) => split(s).into_iter().map(J::String).collect(),
                    _ => return Err(bad()),
                };
                let idx: Option<Vec<usize>> = items.iter().map(|x| choice(p, x)).collect();
                json!(idx.ok_or_else(bad)?)
            }
            "enum" => json!(choice(p, v).ok_or_else(bad)?),
            "number" => json!(number(v).ok_or_else(bad)?),
            "integer" | "band" => json!(number(v).ok_or_else(bad)?.round() as i64),
            "field" | "string" => match v.as_str().map(str::trim) {
                Some("") if p.kind == "field" => continue,
                Some(s) if p.multiple => json!(split(s)),
                Some(s) => json!(s),
                None => return Err(bad()),
            },
            _ => v.clone(),
        };
        out.insert(p.key.clone(), got);
    }
    Ok((J::Object(out), utm, sources, source_props))
}

/// Запрос RustGIS - в значения прогона. Ошибка - готовый ответ
/// `{"error": ...}` на языке RustGIS.
pub fn prepare<'a>(
    catalog: &'a Catalog,
    opts: &Options,
    store: &Store,
    req: &J,
    lang: Lang,
) -> Result<Prepared<'a>, J> {
    let id = req["id"].as_str().unwrap_or("");
    let tool = catalog
        .by_rg_id(id)
        .filter(|t| opts.offered.contains(&t.name.as_str()))
        .ok_or_else(|| json!({"error": tr(lang, "Инструмента «%s» нет.", &[id.to_string()])}))?;
    let (values, utm, sources, source_props) = values(tool, req, store).map_err(|b| match b {
        Bad::NoBodies(layer) => json!({"error": tr(
            lang,
            "Слой «%s» без тел с высотой. Слой RustGIS высоты не хранит, тела дают итоги инструментов расширения.",
            &[layer],
        )}),
        Bad::Key(key) => {
            let label = tool
                .param(&key)
                .map_or(key.clone(), |p| p.label.get(lang).to_string());
            let why = tr(lang, "%s: значение не того вида", &[label]);
            json!({"error": tr(lang, "Значения не годятся: %s", &[why])})
        }
    })?;
    // таблица с X и Y без геометрии - метры своей системы с кодом EPSG
    let table_xy = opts.xy_tables.iter().any(|(n, k)| {
        *n == tool.name && values.get(*k).is_some() && values[format!("@{k}")]["geometry"] == "none"
    });
    let frame = if table_xy {
        let code = req["params"][CRS_PARAM].as_str().and_then(epsg).ok_or_else(|| {
            json!({"error": tr(lang, "Нужен код системы координат X и Y таблицы, например EPSG:28410.", &[])})
        })?;
        Frame::Native(code)
    } else {
        Frame::Utm(utm)
    };
    Ok(Prepared {
        tool,
        values,
        frame,
        utm,
        sources,
        source_props,
    })
}

/// Значение поля выхода для GeoJSON: NaN - `null`.
fn out_value(v: &J) -> J {
    match v.as_f64() {
        Some(x) if !x.is_finite() => J::Null,
        _ => v.clone(),
    }
}

/// Ответ RustGIS по выходам прогона и итоги с высотой для окна продукта.
pub struct Finished {
    /// `{"ok": {"layers", "tables", "messages"}}`.
    pub reply: J,
    /// Итоги с высотой по слоям (имя слоя, FeatureCollection в градусах
    /// с Z): их показывает окно продукта.
    pub solids: Vec<(String, J)>,
}

/// Выходы прогона (`output` модуля по ключам) - в ответ RustGIS.
/// `messages` - сообщения расчёта уже на языке показа.
pub fn finish(
    p: &Prepared<'_>,
    outputs: &J,
    mut messages: Vec<String>,
    store: &Store,
    lang: Lang,
) -> Finished {
    let tool = p.tool;
    let mut layers = Vec::new();
    let mut tables = Vec::new();
    let mut solids: Vec<(String, J)> = Vec::new();
    let empty = HashMap::new();
    for (key, o) in outputs.as_object().into_iter().flatten() {
        // имя слоя - подпись выхода на языке RustGIS, иначе имя из расчёта
        let name = tool.param(key).map_or_else(
            || o["name"].as_str().unwrap_or(key).to_string(),
            |q| q.label.get(lang).to_string(),
        );
        let kind = o["geometry"].as_str().unwrap_or("");
        let feats = o["features"].as_array().cloned().unwrap_or_default();
        // куб слоем RustGIS не положить: непустые ячейки - в окно точками
        if kind == "grid" {
            if let Frame::Utm(u) = &p.frame {
                let pts: Vec<J> = feats
                    .iter()
                    .flat_map(|f| geom::grid_points(&f["geometry"]["grid"], u))
                    .collect();
                if !pts.is_empty() {
                    solids.push((
                        name.clone(),
                        json!({"type": "FeatureCollection", "features": pts}),
                    ));
                }
            }
            messages.push(tr(
                lang,
                "%s: куб - в окне сцены, ячейками точками.",
                &[name],
            ));
            continue;
        }
        // объекты источника - геометрией своего входа
        let src = o["keep_source"]
            .as_str()
            .and_then(|k| p.sources.get(k))
            .or_else(|| {
                tool.params
                    .iter()
                    .find(|q| q.is_layer() && p.sources.contains_key(&q.key))
                    .and_then(|q| p.sources.get(&q.key))
            })
            .unwrap_or(&empty);
        let fields: Vec<String> = o["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| {
                f["name"]
                    .as_str()
                    .or_else(|| f[0].as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        // keep_source модуля: выход начинается полями объекта источника,
        // кроме выхода, заменяющего поля (у него полный набор в named)
        let keep_props = o["keep_source"]
            .as_str()
            .filter(|_| !o["replace"].as_bool().unwrap_or(false))
            .and_then(|k| p.source_props.get(k));
        let features: Vec<J> = feats
            .iter()
            .map(|f| {
                let mut props = serde_json::Map::new();
                if let (Some(by_id), Some(id)) = (keep_props, f["source"].as_i64())
                    && let Some(src) = by_id.get(&id).and_then(J::as_object)
                {
                    for (k, v) in src {
                        props.insert(k.clone(), v.clone());
                    }
                }
                match &f["values"] {
                    // выход, заменяющий поля источника, - значения по имени
                    J::Object(m) => {
                        for (k, v) in m {
                            props.insert(k.clone(), out_value(v));
                        }
                    }
                    vals => {
                        for (field, v) in fields.iter().zip(vals.as_array().into_iter().flatten()) {
                            props.insert(field.clone(), out_value(v));
                        }
                    }
                }
                for (k, v) in f["named"].as_object().into_iter().flatten() {
                    props.insert(k.clone(), out_value(v));
                }
                if let Some(s) = f["source"].as_i64() {
                    props.entry("source").or_insert(json!(s));
                }
                // отметки тела и линии: высоты слой RustGIS не держит
                if let Some((lo, hi)) = geom::z_range(&f["geometry"]) {
                    props.entry("z_min").or_insert(json!(lo));
                    props.entry("z_max").or_insert(json!(hi));
                }
                json!({
                    "type": "Feature",
                    "properties": props,
                    "geometry": geom::plan(&f["geometry"], &p.frame, src),
                })
            })
            .collect();
        // итог с высотой: свой или объекты источника с высотой
        let src_z = kind == "source"
            && feats.iter().any(|f| {
                f["geometry"]["source"]
                    .as_i64()
                    .and_then(|id| src.get(&id))
                    .is_some_and(geom::has_z)
            });
        // свой итог - только если у него есть высота: точки и линии без Z
        // слой RustGIS держит и так
        let own_z = matches!(kind, "mesh" | "line" | "point" | "polygon")
            && feats.iter().any(|f| {
                let g = &f["geometry"];
                geom::z_range(g).is_some()
                    || g["xyz"]
                        .get(2)
                        .and_then(J::as_f64)
                        .is_some_and(f64::is_finite)
            });
        let solid = src_z || own_z;
        if solid && let Frame::Utm(u) = &p.frame {
            let g3: Vec<J> = feats
                .iter()
                .map(|f| geom::solid(&f["geometry"], u, src))
                .collect();
            store.keep(&name, &features, g3.clone());
            let feats3: Vec<J> = features
                .iter()
                .zip(g3)
                .map(|(f, g)| json!({"type": "Feature", "properties": f["properties"], "geometry": g}))
                .collect();
            solids.push((
                name.clone(),
                json!({"type": "FeatureCollection", "features": feats3}),
            ));
        }
        let mut fc = json!({"type": "FeatureCollection", "features": features});
        // метры таблицы: RustGIS читает член crs и пересчитывает сам
        if let Frame::Native(code) = &p.frame {
            fc["crs"] = json!({"type": "name", "properties": {"name": code}});
        }
        if kind == "none" {
            tables.push(json!({"name": name, "geojson": fc}));
        } else {
            layers.push(json!({"name": name, "geojson": fc}));
        }
        if kind == "mesh" {
            messages.push(tr(
                lang,
                "%s: тела в плане, верхними гранями, метры зоны UTM %s.",
                &[tool.display_name(lang), p.utm.label()],
            ));
        }
    }
    Finished {
        reply: json!({"ok": {"layers": layers, "tables": tables, "messages": messages}}),
        solids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        Catalog::parse(
            r#"{"tools": [{"code": "9.01", "name": "offset_line", "title": {"ru": "Сдвиг", "en": "Offset"},
            "params": [{"key": "INPUT", "kind": "lines", "label": {"ru": "Линии", "en": "Lines"}},
                       {"key": "DZ", "kind": "number", "label": {"ru": "Сдвиг, м", "en": "Shift, m"}},
                       {"key": "OUTPUT", "kind": "sink", "label": {"ru": "Сдвинутые", "en": "Shifted"}}]}]}"#,
        )
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    const OPTS: Options = Options {
        toolbox: "Test",
        offered: &["offset_line"],
        xy_tables: &[],
    };

    /// Запрос - в метры зоны, выход линии - план в градусах, отметки,
    /// высоты - в хранилище и в окно, слой-итог на входе - с высотой.
    #[test]
    fn a_line_tool_round_trip() {
        let cat = catalog();
        let store = Store::new(crate::store::temp_home("rustgis-kit-tool").join("bodies"));
        let fc = json!({"type": "FeatureCollection", "features": [
            {"type": "Feature", "row": 0, "properties": {"name": "a"},
             "geometry": {"type": "LineString", "coordinates": [[56.0, 54.7], [56.001, 54.7]]}}]});
        let req = json!({"kind": "tool", "id": "offset-line", "params": {"input": "Оси", "dz": "2,5"},
            "layers": {"Оси": fc}});
        let p =
            prepare(&cat, &OPTS, &store, &req, Lang::Ru).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(p.values["DZ"], 2.5);
        assert_eq!(p.values["@INPUT"]["geometry"], "line");
        let line = &p.values["INPUT"][0]["parts"][0];
        let (x0, y0) = (
            line[0][0].as_f64().unwrap_or(0.0),
            line[0][1].as_f64().unwrap_or(0.0),
        );
        // расчёт продукта: та же линия на 2.5 м выше
        let outputs = json!({"OUTPUT": {"geometry": "line", "fields": [{"name": "dz", "type": "double"}],
            "features": [{"source": 0, "values": [2.5],
                          "geometry": {"points": [[x0, y0, 2.5], [x0 + 64.0, y0, 2.5]]}}]}});
        let done = finish(&p, &outputs, vec!["Готово.".into()], &store, Lang::Ru);
        let f = &done.reply["ok"]["layers"][0]["geojson"]["features"][0];
        assert_eq!(done.reply["ok"]["layers"][0]["name"], "Сдвинутые");
        assert_eq!(f["properties"]["dz"], 2.5);
        assert_eq!(f["properties"]["z_max"], 2.5);
        assert!((f["geometry"]["coordinates"][0][0].as_f64().unwrap_or(0.0) - 56.0).abs() < 1e-9);
        assert_eq!(done.solids.len(), 1);
        // итог на входе следующего запуска - с высотой и прежними метрами
        let back = json!({"type": "FeatureCollection", "features": [f]});
        let req = json!({"kind": "tool", "id": "offset-line", "params": {"input": "Сдвинутые"},
            "layers": {"Сдвинутые": back}});
        let p2 =
            prepare(&cat, &OPTS, &store, &req, Lang::En).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(p2.values["INPUT"][0]["parts"][0][0], json!([x0, y0, 2.5]));
        // keep_source - поля объекта источника и поверх них свои, replace -
        // только свои
        let kept = json!({"OUTPUT": {"geometry": "source", "keep_source": "INPUT",
            "fields": [{"name": "dz", "type": "double"}],
            "features": [{"source": 0, "values": [1.0], "geometry": {"source": 0}}]}});
        let store3 = Store::new(crate::store::temp_home("rustgis-kit-tool").join("kept"));
        let done = finish(&p, &kept, Vec::new(), &store3, Lang::Ru);
        let props = &done.reply["ok"]["layers"][0]["geojson"]["features"][0]["properties"];
        assert_eq!(props["name"], "a");
        assert_eq!(props["dz"], 1.0);
        let mut replaced = kept.clone();
        replaced["OUTPUT"]["replace"] = json!(true);
        let done = finish(&p, &replaced, Vec::new(), &store3, Lang::Ru);
        let props = &done.reply["ok"]["layers"][0]["geojson"]["features"][0]["properties"];
        assert!(props.get("name").is_none());
        // точки и линии без Z - в проект, мимо хранилища и окна
        let flat = json!({"OUTPUT": {"geometry": "point", "fields": [], "features": [
            {"source": 0, "values": [], "geometry": {"xyz": [x0, y0, null]}}]}});
        let store2 = Store::new(crate::store::temp_home("rustgis-kit-tool").join("flat"));
        let done = finish(&p, &flat, Vec::new(), &store2, Lang::Ru);
        assert!(done.solids.is_empty() && !store2.has("Сдвинутые"));
        assert_eq!(
            done.reply["ok"]["layers"][0]["geojson"]["features"][0]["geometry"]["type"],
            "Point"
        );
        // чужой инструмент и не тот вид значения - ошибки
        let r = json!({"id": "nope"});
        assert_eq!(
            prepare(&cat, &OPTS, &store, &r, Lang::En).err(),
            Some(json!({"error": "No tool 'nope'."}))
        );
        let r = json!({"id": "offset-line", "params": {"dz": "много"}, "layers": {}});
        let e = prepare(&cat, &OPTS, &store, &r, Lang::En)
            .err()
            .unwrap_or(J::Null);
        assert_eq!(
            e["error"],
            "The values do not fit: Shift, m: wrong kind of value"
        );
        let _ = std::fs::remove_dir_all(crate::store::temp_home("rustgis-kit-tool"));
    }
}
