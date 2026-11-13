// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Геометрия: GeoJSON RustGIS и геометрия прогонов модуля.
//!
//! Геометрия объекта выхода модуля (`output` в `tool_runs.json`):
//!
//! - тело - `{"verts": [[x, y, z]], "faces": [[a, b, c]]}`;
//! - линия - `{"points": [[x, y, z]]}`;
//! - точка - `{"xyz": [x, y, z]}`;
//! - объект источника - `{"source": id}`;
//! - строка таблицы - `null`;
//! - куб - `{"grid": {"transform", "nx", "ny", "z0", "dz", "bands"}}`.
//!
//! Высота без значения - `null`. Координаты - метры системы расчёта.

use std::collections::HashMap;

use serde_json::{Value as J, json};

use crate::geo::Utm;

/// Система координат итога: метры зоны UTM (итог переводится обратно
/// в градусы) или метры системы с кодом EPSG (итог уходит в RustGIS
/// в метрах с членом `crs`, пересчитывает сам RustGIS).
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    Utm(Utm),
    Native(String),
}

impl Frame {
    /// Точка итога - в координаты GeoJSON для RustGIS (без высоты).
    pub fn coord(&self, p: &[f64; 3]) -> J {
        match self {
            Frame::Utm(u) => lonlat(u, p),
            Frame::Native(_) => json!([p[0], p[1]]),
        }
    }
}

/// Координаты GeoJSON: `[x, y]` или `[x, y, z]`, высоты нет - NaN.
pub fn position(v: &J) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    if a.len() < 2 {
        return None;
    }
    let z = a.get(2).and_then(J::as_f64).unwrap_or(f64::NAN);
    Some([a[0].as_f64()?, a[1].as_f64()?, z])
}

/// Части геометрии GeoJSON: линии - по линии на часть, точки и полигоны -
/// вершины по кольцам.
pub fn parts(g: &J) -> Vec<Vec<[f64; 3]>> {
    let c = &g["coordinates"];
    let line = |v: &J| -> Vec<[f64; 3]> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(position)
            .collect()
    };
    match g["type"].as_str().unwrap_or("") {
        "Point" => position(c).map(|p| vec![vec![p]]).unwrap_or_default(),
        "MultiPoint" | "LineString" => vec![line(c)],
        "MultiLineString" | "Polygon" => c.as_array().into_iter().flatten().map(line).collect(),
        "MultiPolygon" => c
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|p| p.as_array().into_iter().flatten().map(line))
            .collect(),
        _ => Vec::new(),
    }
}

/// Есть ли у геометрии GeoJSON высоты.
pub fn has_z(g: &J) -> bool {
    parts(g).iter().flatten().any(|p| p[2].is_finite())
}

/// Первая точка геометрии - метка объекта плана.
pub fn first_point(g: &J) -> J {
    let mut c = &g["coordinates"];
    while c
        .as_array()
        .is_some_and(|a| a.first().is_some_and(J::is_array))
    {
        c = &c[0];
    }
    c.clone()
}

/// Метры до микрометра: градусы туда и обратно дают шум, а ячейка расчёта
/// на ровной границе от него сдвигается.
pub fn snap(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

/// Точку зоны - в градусы GeoJSON без высоты.
pub fn lonlat(utm: &Utm, p: &[f64; 3]) -> J {
    let [lon, lat] = utm.inverse(p[0], p[1]);
    json!([lon, lat])
}

/// Точку зоны - в градусы GeoJSON с высотой, если она есть.
pub fn lonlat_z(utm: &Utm, v: &[f64; 3]) -> J {
    let [lon, lat] = utm.inverse(v[0], v[1]);
    if v[2].is_finite() {
        json!([lon, lat, v[2]])
    } else {
        json!([lon, lat])
    }
}

/// Метры итога рядом с градусами: член `m` (те же координаты в метрах
/// зоны) и `utm` (зона, север). Следующий инструмент в той же зоне берёт
/// метры как есть, без перевода туда и обратно.
pub fn with_metres(mut g: J, m: J, utm: &Utm) -> J {
    if g.is_object() {
        g["m"] = m;
        g["utm"] = json!([utm.zone, utm.north]);
    }
    g
}

/// Части геометрии GeoJSON в метрах зоны `utm`: свои метры итога, если
/// зона та же, иначе градусы через зону (до микрометра).
pub fn parts_m(g: &J, utm: &Utm) -> Vec<Vec<[f64; 3]>> {
    if g["utm"] == json!([utm.zone, utm.north]) {
        let mut own = g.clone();
        own["coordinates"] = g["m"].clone();
        return parts(&own);
    }
    parts(g)
        .into_iter()
        .map(|part| {
            part.into_iter()
                .map(|[x, y, z]| {
                    let [e, n] = utm.forward(x, y).map(snap);
                    [e, n, z]
                })
                .collect()
        })
        .collect()
}

/// Геометрия без высот и без метров итога: план для слоя RustGIS.
pub fn strip_z(g: &J) -> J {
    fn walk(v: &J) -> J {
        match v.as_array() {
            Some(a) if a.first().is_some_and(J::is_number) => {
                J::Array(a.iter().take(2).cloned().collect())
            }
            Some(a) => J::Array(a.iter().map(walk).collect()),
            None => v.clone(),
        }
    }
    let mut out = g.clone();
    if let Some(c) = g.get("coordinates") {
        out["coordinates"] = walk(c);
    }
    if let Some(o) = out.as_object_mut() {
        o.remove("m");
        o.remove("utm");
    }
    out
}

/// Тело из треугольников GeoJSON с высотой (кольца по 3 вершины, как
/// модуль пишет тела) в метрах зоны: `{"verts", "faces"}`. Одинаковые
/// вершины склеиваются: по номерам вершин расчёт проверяет замкнутость.
/// Треугольника нет - None.
pub fn body_of(geoms: &[&J], utm: &Utm) -> Option<J> {
    let mut verts: Vec<[f64; 3]> = Vec::new();
    let mut index: HashMap<[u64; 3], u32> = HashMap::new();
    let mut faces: Vec<[u32; 3]> = Vec::new();
    for g in geoms {
        for ring in parts_m(g, utm) {
            let ring: Vec<[f64; 3]> = if ring.len() == 4 && ring[0] == ring[3] {
                ring[..3].to_vec()
            } else {
                ring
            };
            if ring.len() != 3 || ring.iter().any(|p| !p[2].is_finite()) {
                continue;
            }
            let mut f = [0u32; 3];
            for (k, p) in ring.iter().enumerate() {
                let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
                let next = u32::try_from(verts.len()).ok()?;
                f[k] = *index.entry(key).or_insert_with(|| {
                    verts.push(*p);
                    next
                });
            }
            faces.push(f);
        }
    }
    (!faces.is_empty()).then(|| json!({"verts": verts, "faces": faces}))
}

/// Точка геометрии модуля: `[x, y, z]`, высота `null` - NaN.
fn mpoint(v: &J) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([
        a.first()?.as_f64()?,
        a.get(1)?.as_f64()?,
        a.get(2).and_then(J::as_f64).unwrap_or(f64::NAN),
    ])
}

fn mpoints(v: &J) -> Vec<[f64; 3]> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(mpoint)
        .collect()
}

/// Треугольники тела модуля: тройки вершин. Порченый номер - пропуск.
fn triangles(g: &J) -> Vec<[[f64; 3]; 3]> {
    let verts = mpoints(&g["verts"]);
    g["faces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| {
            let at = |i: usize| -> Option<[f64; 3]> {
                let k = usize::try_from(f.get(i)?.as_u64()?).ok()?;
                verts.get(k).copied()
            };
            Some([at(0)?, at(1)?, at(2)?])
        })
        .collect()
}

/// Отметки геометрии модуля: наименьшая и наибольшая Z тела или линии.
pub fn z_range(g: &J) -> Option<(f64, f64)> {
    let pts = if g.get("verts").is_some() {
        mpoints(&g["verts"])
    } else if g.get("points").is_some() {
        mpoints(&g["points"])
    } else {
        return None;
    };
    let z: Vec<f64> = pts.iter().map(|p| p[2]).filter(|z| z.is_finite()).collect();
    let lo = z.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = z.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (lo <= hi).then_some((lo, hi))
}

/// Геометрия объекта итога для слоя RustGIS в системе `frame`, без высот.
/// Тело - верхними гранями (контур в плане), объект источника - его
/// геометрия из `sources`, строка таблицы и куб - `null`.
pub fn plan(g: &J, frame: &Frame, sources: &HashMap<i64, J>) -> J {
    if g.get("verts").is_some() {
        let up = |t: &[[f64; 3]; 3]| {
            let (a, b, c) = (t[0], t[1], t[2]);
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.0
        };
        let polys: Vec<J> = triangles(g)
            .iter()
            .filter(|t| up(t))
            .map(|t| {
                json!([[
                    frame.coord(&t[0]),
                    frame.coord(&t[1]),
                    frame.coord(&t[2]),
                    frame.coord(&t[0])
                ]])
            })
            .collect();
        return json!({"type": "MultiPolygon", "coordinates": polys});
    }
    if g.get("points").is_some() {
        let pts: Vec<J> = mpoints(&g["points"])
            .iter()
            .map(|p| frame.coord(p))
            .collect();
        return json!({"type": "LineString", "coordinates": pts});
    }
    if let Some(p) = mpoint(&g["xyz"]) {
        return json!({"type": "Point", "coordinates": frame.coord(&p)});
    }
    if let Some(id) = g["source"].as_i64() {
        return strip_z(&sources.get(&id).cloned().unwrap_or(J::Null));
    }
    J::Null
}

/// Геометрия итога с высотой в градусах зоны `utm` с метрами рядом:
/// тело - треугольниками MultiPolygon, линия и точка - с Z. Объект
/// источника - геометрия входа из `sources`, остальное - `null`.
pub fn solid(g: &J, utm: &Utm, sources: &HashMap<i64, J>) -> J {
    let metre = |v: &[f64; 3]| {
        if v[2].is_finite() {
            json!([v[0], v[1], v[2]])
        } else {
            json!([v[0], v[1]])
        }
    };
    if g.get("verts").is_some() {
        let tris = triangles(g);
        let deg: Vec<J> = tris
            .iter()
            .map(|t| {
                json!([[
                    lonlat_z(utm, &t[0]),
                    lonlat_z(utm, &t[1]),
                    lonlat_z(utm, &t[2]),
                    lonlat_z(utm, &t[0])
                ]])
            })
            .collect();
        let m: Vec<J> = tris
            .iter()
            .map(|t| json!([[metre(&t[0]), metre(&t[1]), metre(&t[2]), metre(&t[0])]]))
            .collect();
        return with_metres(
            json!({"type": "MultiPolygon", "coordinates": deg}),
            json!(m),
            utm,
        );
    }
    if g.get("points").is_some() {
        let p = mpoints(&g["points"]);
        return with_metres(
            json!({"type": "LineString", "coordinates": p.iter().map(|q| lonlat_z(utm, q)).collect::<Vec<_>>()}),
            json!(p.iter().map(metre).collect::<Vec<_>>()),
            utm,
        );
    }
    if let Some(p) = mpoint(&g["xyz"]) {
        return with_metres(
            json!({"type": "Point", "coordinates": lonlat_z(utm, &p)}),
            metre(&p),
            utm,
        );
    }
    if let Some(id) = g["source"].as_i64() {
        return sources.get(&id).cloned().unwrap_or(J::Null);
    }
    J::Null
}

/// Непустые ячейки куба модуля точками в градусах с высотой: середина
/// ячейки, значение - полем `value`. Ноль и пусто не показываются.
pub fn grid_points(grid: &J, utm: &Utm) -> Vec<J> {
    let t: Vec<f64> = grid["transform"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(J::as_f64)
        .collect();
    if t.len() < 6 {
        return Vec::new();
    }
    let nx = grid["nx"].as_u64().unwrap_or(1).max(1) as usize;
    let (z0, dz) = (
        grid["z0"].as_f64().unwrap_or(0.0),
        grid["dz"].as_f64().unwrap_or(1.0),
    );
    let mut out = Vec::new();
    for (k, band) in grid["bands"].as_array().into_iter().flatten().enumerate() {
        let z = z0 + k as f64 * dz;
        for (at, v) in band.as_array().into_iter().flatten().enumerate() {
            let Some(v) = v.as_f64().filter(|v| *v != 0.0) else {
                continue;
            };
            let (row, col) = (at / nx, at % nx);
            let x = t[0] + (col as f64 + 0.5) * t[1];
            let y = t[3] + (row as f64 + 0.5) * t[5];
            out.push(json!({"type": "Feature", "properties": {"value": v},
                "geometry": {"type": "Point", "coordinates": lonlat_z(utm, &[x, y, z])}}));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Тело модуля: план - верхние грани, 3D - треугольники с метрами,
    /// обратно в тело - те же вершины, склеенные.
    #[test]
    fn body_round_trip() {
        let utm = Utm::at(56.0, 54.7);
        let [e, n] = utm.forward(56.0, 54.7).map(f64::round);
        let g = json!({"verts": [[e, n, -300.0], [e + 10.0, n, -300.0], [e, n + 10.0, -297.0],
                                 [e, n, -310.0]],
                       "faces": [[0, 1, 2], [0, 2, 3], [0, 3, 1], [1, 3, 2], [0, 1, 9]]});
        assert_eq!(z_range(&g), Some((-310.0, -297.0)));
        let p = plan(&g, &Frame::Utm(utm), &HashMap::new());
        assert_eq!(p["type"], "MultiPolygon");
        let s = solid(&g, &utm, &HashMap::new());
        assert_eq!(s["utm"], json!([40, true]));
        let b = body_of(&[&s], &utm).unwrap_or(J::Null);
        assert_eq!(b["verts"].as_array().map(Vec::len), Some(4));
        assert_eq!(b["faces"].as_array().map(Vec::len), Some(4));
        assert_eq!(b["verts"][0], json!([e, n, -300.0]));
        let flat = strip_z(&s);
        assert!(flat.get("m").is_none() && !has_z(&flat));
    }
}
