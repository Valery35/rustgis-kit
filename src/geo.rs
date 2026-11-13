// rustgis-kit - общая основа расширений RustGIS линейки Информ++.
// Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
// SPDX-License-Identifier: Apache-2.0

//! Градусы WGS 84 в метры зоны UTM и обратно: поперечная проекция
//! Меркатора рядами Крюгера шестого порядка (Karney, «Transverse Mercator
//! with an accuracy of a few nanometers», J. Geodesy 85, 2011). Туда
//! и обратно - нанометры, а не миллиметры, как у рядов Снайдера: ячейка
//! расчёта на ровной границе от перевода не сдвигается.
//!
//! RustGIS отдаёт расширению объекты только в градусах WGS 84, а расчётам
//! нужны метры. Зона - по середине данных ([`Utm::at`]).

/// Большая полуось и сжатие WGS 84.
const A: f64 = 6_378_137.0;
const F: f64 = 1.0 / 298.257_223_563;
/// Масштаб на осевом меридиане и ложные сдвиги UTM.
const K0: f64 = 0.9996;
const FALSE_EAST: f64 = 500_000.0;
const FALSE_NORTH_SOUTH: f64 = 10_000_000.0;

/// Зона UTM: номер и полушарие.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utm {
    pub zone: u8,
    pub north: bool,
}

/// Коэффициенты рядов: A, α1..α6, β1..β6, эксцентриситет.
struct Series {
    big_a: f64,
    alpha: [f64; 6],
    beta: [f64; 6],
    e: f64,
}

fn series() -> Series {
    let n = F / (2.0 - F);
    let (n2, n3, n4, n5, n6) = (n * n, n.powi(3), n.powi(4), n.powi(5), n.powi(6));
    Series {
        big_a: A / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0 + n6 / 256.0),
        alpha: [
            n / 2.0 - 2.0 * n2 / 3.0 + 5.0 * n3 / 16.0 + 41.0 * n4 / 180.0 - 127.0 * n5 / 288.0
                + 7891.0 * n6 / 37800.0,
            13.0 * n2 / 48.0 - 3.0 * n3 / 5.0 + 557.0 * n4 / 1440.0 + 281.0 * n5 / 630.0
                - 1983433.0 * n6 / 1935360.0,
            61.0 * n3 / 240.0 - 103.0 * n4 / 140.0
                + 15061.0 * n5 / 26880.0
                + 167603.0 * n6 / 181440.0,
            49561.0 * n4 / 161280.0 - 179.0 * n5 / 168.0 + 6601661.0 * n6 / 7257600.0,
            34729.0 * n5 / 80640.0 - 3418889.0 * n6 / 1995840.0,
            212378941.0 * n6 / 319334400.0,
        ],
        beta: [
            n / 2.0 - 2.0 * n2 / 3.0 + 37.0 * n3 / 96.0 - n4 / 360.0 - 81.0 * n5 / 512.0
                + 96199.0 * n6 / 604800.0,
            n2 / 48.0 + n3 / 15.0 - 437.0 * n4 / 1440.0 + 46.0 * n5 / 105.0
                - 1118711.0 * n6 / 3870720.0,
            17.0 * n3 / 480.0 - 37.0 * n4 / 840.0 - 209.0 * n5 / 4480.0 + 5569.0 * n6 / 90720.0,
            4397.0 * n4 / 161280.0 - 11.0 * n5 / 504.0 - 830251.0 * n6 / 7257600.0,
            4583.0 * n5 / 161280.0 - 108847.0 * n6 / 3991680.0,
            20648693.0 * n6 / 638668800.0,
        ],
        e: (F * (2.0 - F)).sqrt(),
    }
}

impl Utm {
    /// Зона по точке: долгота и широта в градусах.
    pub fn at(lon: f64, lat: f64) -> Self {
        let z = ((lon + 180.0) / 6.0).floor().clamp(0.0, 59.0) as u8 + 1;
        Self {
            zone: z,
            north: lat >= 0.0,
        }
    }

    /// Код EPSG зоны: 326xx на севере, 327xx на юге.
    pub fn epsg(&self) -> u32 {
        (if self.north { 32600 } else { 32700 }) + u32::from(self.zone)
    }

    /// Подпись зоны: `40N`.
    pub fn label(&self) -> String {
        format!("{}{}", self.zone, if self.north { "N" } else { "S" })
    }

    fn lon0(&self) -> f64 {
        (f64::from(self.zone) * 6.0 - 183.0).to_radians()
    }

    fn false_north(&self) -> f64 {
        if self.north { 0.0 } else { FALSE_NORTH_SOUTH }
    }

    /// Градусы в метры: восток, север.
    pub fn forward(&self, lon: f64, lat: f64) -> [f64; 2] {
        let s = series();
        let phi = lat.to_radians();
        let lam = lon.to_radians() - self.lon0();
        // тангенс широты на конформной сфере
        let sp = phi.sin();
        let t = (sp.atanh() - s.e * (s.e * sp).atanh()).sinh();
        let xi1 = t.atan2(lam.cos());
        let eta1 = (lam.sin() / (1.0 + t * t).sqrt()).atanh();
        let (mut xi, mut eta) = (xi1, eta1);
        for (j, a) in s.alpha.iter().enumerate() {
            let k = 2.0 * (j as f64 + 1.0);
            xi += a * (k * xi1).sin() * (k * eta1).cosh();
            eta += a * (k * xi1).cos() * (k * eta1).sinh();
        }
        [
            FALSE_EAST + K0 * s.big_a * eta,
            self.false_north() + K0 * s.big_a * xi,
        ]
    }

    /// Метры в градусы: долгота, широта.
    pub fn inverse(&self, x: f64, y: f64) -> [f64; 2] {
        let s = series();
        let xi = (y - self.false_north()) / (K0 * s.big_a);
        let eta = (x - FALSE_EAST) / (K0 * s.big_a);
        let (mut xi1, mut eta1) = (xi, eta);
        for (j, b) in s.beta.iter().enumerate() {
            let k = 2.0 * (j as f64 + 1.0);
            xi1 -= b * (k * xi).sin() * (k * eta).cosh();
            eta1 -= b * (k * xi).cos() * (k * eta).sinh();
        }
        let lam = eta1.sinh().atan2(xi1.cos());
        // тангенс конформной широты и его обращение Ньютоном
        let tau1 = xi1.sin() / (eta1.sinh().powi(2) + xi1.cos().powi(2)).sqrt();
        let e2 = s.e * s.e;
        let mut tau = tau1;
        for _ in 0..8 {
            let sigma = (s.e * (s.e * tau / (1.0 + tau * tau).sqrt()).atanh()).sinh();
            let tau_i = tau * (1.0 + sigma * sigma).sqrt() - sigma * (1.0 + tau * tau).sqrt();
            let d = (tau1 - tau_i) / (1.0 + tau_i * tau_i).sqrt() * (1.0 + (1.0 - e2) * tau * tau)
                / ((1.0 - e2) * (1.0 + tau * tau).sqrt());
            tau += d;
            if d.abs() < 1e-15 {
                break;
            }
        }
        [(self.lon0() + lam).to_degrees(), tau.atan().to_degrees()]
    }
}

/// Код EPSG из текста: `EPSG:28410`, `epsg:28410` или `28410`.
pub fn epsg(text: &str) -> Option<String> {
    let t = text.trim();
    let digits = t
        .strip_prefix("EPSG:")
        .or_else(|| t.strip_prefix("epsg:"))
        .unwrap_or(t);
    (!digits.is_empty() && digits.len() <= 9 && digits.chars().all(|c| c.is_ascii_digit()))
        .then(|| format!("EPSG:{digits}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Опорная точка EPSG:32632 (epsg.io, до сантиметра): 10° в. д.,
    /// 50° с. ш. - 571 666.45, 5 539 109.82. Туда и обратно - нанометры.
    #[test]
    fn utm_like_proj() {
        let z = Utm::at(10.0, 50.0);
        assert_eq!(
            (z.zone, z.north, z.epsg(), z.label().as_str()),
            (32, true, 32632, "32N")
        );
        let [x, y] = z.forward(10.0, 50.0);
        assert!((x - 571_666.45).abs() < 0.006, "{x}");
        assert!((y - 5_539_109.82).abs() < 0.006, "{y}");
        let [x, y] = Utm::at(3.0, 0.0).forward(3.0, 0.0);
        assert!((x - 500_000.0).abs() < 1e-6 && y.abs() < 1e-6);
        for (lon, lat) in [
            (57.3, 56.1),
            (59.9, 67.5),
            (-70.6, -33.4),
            (151.2, -33.9),
            (56.0, 54.7),
        ] {
            let z = Utm::at(lon, lat);
            let [x, y] = z.forward(lon, lat);
            let [lo, la] = z.inverse(x, y);
            assert!(
                (lo - lon).abs() < 1e-11 && (la - lat).abs() < 1e-11,
                "{lon} {lat}: {lo} {la}"
            );
            let [x2, y2] = z.forward(lo, la);
            assert!(
                (x2 - x).abs() < 1e-6 && (y2 - y).abs() < 1e-6,
                "{lon} {lat}"
            );
        }
        assert_eq!(Utm::at(-70.6, -33.4).epsg(), 32719);
    }

    #[test]
    fn epsg_codes() {
        assert_eq!(epsg("EPSG:3857").as_deref(), Some("EPSG:3857"));
        assert_eq!(epsg(" epsg:4326 ").as_deref(), Some("EPSG:4326"));
        assert_eq!(epsg("28410").as_deref(), Some("EPSG:28410"));
        assert!(epsg("Pulkovo").is_none() && epsg("").is_none() && epsg("1234567890").is_none());
    }
}
