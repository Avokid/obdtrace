//! OBD-II PID table, response parsing and DTC decoding.

use serde::Serialize;

pub struct PidDef {
    pub pid: u8,
    pub name: &'static str,
    pub unit: &'static str,
    pub len: usize,
    /// 0 = polled every cycle, 1 = every 5th cycle
    pub tier: u8,
    /// Decimal places kept after decoding.
    pub decimals: i32,
    /// Returns NaN when the ECU marks the value as not applicable.
    pub f: fn(&[u8]) -> f64,
}

fn ab(d: &[u8]) -> f64 {
    d[0] as f64 * 256.0 + d[1] as f64
}

pub const PIDS: &[PidDef] = &[
    PidDef {
        pid: 0x04,
        name: "engine_load",
        unit: "%",
        len: 1,
        tier: 0,
        decimals: 2,
        f: |d| d[0] as f64 * 100.0 / 255.0,
    },
    PidDef {
        pid: 0x05,
        name: "coolant_temp",
        unit: "°C",
        len: 1,
        tier: 1,
        decimals: 2,
        f: |d| d[0] as f64 - 40.0,
    },
    PidDef {
        pid: 0x06,
        name: "short_fuel_trim_b1",
        unit: "%",
        len: 1,
        tier: 0,
        decimals: 2,
        f: |d| d[0] as f64 * 100.0 / 128.0 - 100.0,
    },
    PidDef {
        pid: 0x07,
        name: "long_fuel_trim_b1",
        unit: "%",
        len: 1,
        tier: 0,
        decimals: 2,
        f: |d| d[0] as f64 * 100.0 / 128.0 - 100.0,
    },
    PidDef {
        pid: 0x08,
        name: "short_fuel_trim_b2",
        unit: "%",
        len: 1,
        tier: 1,
        decimals: 2,
        f: |d| d[0] as f64 * 100.0 / 128.0 - 100.0,
    },
    PidDef {
        pid: 0x09,
        name: "long_fuel_trim_b2",
        unit: "%",
        len: 1,
        tier: 1,
        decimals: 2,
        f: |d| d[0] as f64 * 100.0 / 128.0 - 100.0,
    },
    PidDef {
        pid: 0x0B,
        name: "intake_manifold_pressure",
        unit: "kPa",
        len: 1,
        tier: 0,
        decimals: 2,
        f: |d| d[0] as f64,
    },
    PidDef {
        pid: 0x0C,
        name: "rpm",
        unit: "rpm",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| ab(d) / 4.0,
    },
    PidDef {
        pid: 0x0D,
        name: "speed",
        unit: "km/h",
        len: 1,
        tier: 0,
        decimals: 2,
        f: |d| d[0] as f64,
    },
    PidDef {
        pid: 0x0E,
        name: "timing_advance",
        unit: "°",
        len: 1,
        tier: 0,
        decimals: 2,
        f: |d| d[0] as f64 / 2.0 - 64.0,
    },
    PidDef {
        pid: 0x0F,
        name: "intake_air_temp",
        unit: "°C",
        len: 1,
        tier: 1,
        decimals: 2,
        f: |d| d[0] as f64 - 40.0,
    },
    PidDef {
        pid: 0x10,
        name: "maf",
        unit: "g/s",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| ab(d) / 100.0,
    },
    PidDef {
        pid: 0x11,
        name: "throttle",
        unit: "%",
        len: 1,
        tier: 0,
        decimals: 2,
        f: |d| d[0] as f64 * 100.0 / 255.0,
    },
    PidDef {
        pid: 0x14,
        name: "o2_b1s1_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        decimals: 3,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x15,
        name: "o2_b1s2_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        decimals: 3,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x14,
        name: "o2_b1s1_stft",
        unit: "%",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| {
            if d[1] == 0xFF {
                f64::NAN
            } else {
                d[1] as f64 * 100.0 / 128.0 - 100.0
            }
        },
    },
    PidDef {
        pid: 0x15,
        name: "o2_b1s2_stft",
        unit: "%",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| {
            if d[1] == 0xFF {
                f64::NAN
            } else {
                d[1] as f64 * 100.0 / 128.0 - 100.0
            }
        },
    },
    PidDef {
        pid: 0x16,
        name: "o2_b1s3_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        decimals: 3,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x16,
        name: "o2_b1s3_stft",
        unit: "%",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| {
            if d[1] == 0xFF {
                f64::NAN
            } else {
                d[1] as f64 * 100.0 / 128.0 - 100.0
            }
        },
    },
    PidDef {
        pid: 0x17,
        name: "o2_b1s4_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        decimals: 3,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x17,
        name: "o2_b1s4_stft",
        unit: "%",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| {
            if d[1] == 0xFF {
                f64::NAN
            } else {
                d[1] as f64 * 100.0 / 128.0 - 100.0
            }
        },
    },
    PidDef {
        pid: 0x18,
        name: "o2_b2s1_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        decimals: 3,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x18,
        name: "o2_b2s1_stft",
        unit: "%",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| {
            if d[1] == 0xFF {
                f64::NAN
            } else {
                d[1] as f64 * 100.0 / 128.0 - 100.0
            }
        },
    },
    PidDef {
        pid: 0x19,
        name: "o2_b2s2_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        decimals: 3,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x19,
        name: "o2_b2s2_stft",
        unit: "%",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| {
            if d[1] == 0xFF {
                f64::NAN
            } else {
                d[1] as f64 * 100.0 / 128.0 - 100.0
            }
        },
    },
    PidDef {
        pid: 0x1A,
        name: "o2_b2s3_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        decimals: 3,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x1A,
        name: "o2_b2s3_stft",
        unit: "%",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| {
            if d[1] == 0xFF {
                f64::NAN
            } else {
                d[1] as f64 * 100.0 / 128.0 - 100.0
            }
        },
    },
    PidDef {
        pid: 0x1B,
        name: "o2_b2s4_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        decimals: 3,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x1B,
        name: "o2_b2s4_stft",
        unit: "%",
        len: 2,
        tier: 0,
        decimals: 2,
        f: |d| {
            if d[1] == 0xFF {
                f64::NAN
            } else {
                d[1] as f64 * 100.0 / 128.0 - 100.0
            }
        },
    },
    PidDef {
        pid: 0x24,
        name: "o2_b1s1_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x24,
        name: "o2_b1s1_wr_voltage",
        unit: "V",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) * 8.0 / 65536.0,
    },
    PidDef {
        pid: 0x25,
        name: "o2_b1s2_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x25,
        name: "o2_b1s2_wr_voltage",
        unit: "V",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) * 8.0 / 65536.0,
    },
    PidDef {
        pid: 0x26,
        name: "o2_b1s3_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x26,
        name: "o2_b1s3_wr_voltage",
        unit: "V",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) * 8.0 / 65536.0,
    },
    PidDef {
        pid: 0x27,
        name: "o2_b1s4_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x27,
        name: "o2_b1s4_wr_voltage",
        unit: "V",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) * 8.0 / 65536.0,
    },
    PidDef {
        pid: 0x28,
        name: "o2_b2s1_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x28,
        name: "o2_b2s1_wr_voltage",
        unit: "V",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) * 8.0 / 65536.0,
    },
    PidDef {
        pid: 0x29,
        name: "o2_b2s2_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x29,
        name: "o2_b2s2_wr_voltage",
        unit: "V",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) * 8.0 / 65536.0,
    },
    PidDef {
        pid: 0x2A,
        name: "o2_b2s3_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x2A,
        name: "o2_b2s3_wr_voltage",
        unit: "V",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) * 8.0 / 65536.0,
    },
    PidDef {
        pid: 0x2B,
        name: "o2_b2s4_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x2B,
        name: "o2_b2s4_wr_voltage",
        unit: "V",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) * 8.0 / 65536.0,
    },
    PidDef {
        pid: 0x34,
        name: "o2_b1s1_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x34,
        name: "o2_b1s1_current",
        unit: "mA",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) / 256.0 - 128.0,
    },
    PidDef {
        pid: 0x35,
        name: "o2_b1s2_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x35,
        name: "o2_b1s2_current",
        unit: "mA",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) / 256.0 - 128.0,
    },
    PidDef {
        pid: 0x36,
        name: "o2_b1s3_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x36,
        name: "o2_b1s3_current",
        unit: "mA",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) / 256.0 - 128.0,
    },
    PidDef {
        pid: 0x37,
        name: "o2_b1s4_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x37,
        name: "o2_b1s4_current",
        unit: "mA",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) / 256.0 - 128.0,
    },
    PidDef {
        pid: 0x38,
        name: "o2_b2s1_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x38,
        name: "o2_b2s1_current",
        unit: "mA",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) / 256.0 - 128.0,
    },
    PidDef {
        pid: 0x39,
        name: "o2_b2s2_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x39,
        name: "o2_b2s2_current",
        unit: "mA",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) / 256.0 - 128.0,
    },
    PidDef {
        pid: 0x3A,
        name: "o2_b2s3_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x3A,
        name: "o2_b2s3_current",
        unit: "mA",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) / 256.0 - 128.0,
    },
    PidDef {
        pid: 0x3B,
        name: "o2_b2s4_lambda",
        unit: "λ",
        len: 4,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x3B,
        name: "o2_b2s4_current",
        unit: "mA",
        len: 4,
        tier: 0,
        decimals: 3,
        f: |d| ab(&d[2..]) / 256.0 - 128.0,
    },
    PidDef {
        pid: 0x44,
        name: "commanded_lambda",
        unit: "λ",
        len: 2,
        tier: 0,
        decimals: 4,
        f: |d| ab(d) * 2.0 / 65536.0,
    },
    PidDef {
        pid: 0x1F,
        name: "run_time",
        unit: "s",
        len: 2,
        tier: 1,
        decimals: 2,
        f: |d| ab(d),
    },
    PidDef {
        pid: 0x2F,
        name: "fuel_level",
        unit: "%",
        len: 1,
        tier: 1,
        decimals: 2,
        f: |d| d[0] as f64 * 100.0 / 255.0,
    },
    PidDef {
        pid: 0x31,
        name: "distance_since_clear",
        unit: "km",
        len: 2,
        tier: 1,
        decimals: 2,
        f: |d| ab(d),
    },
    PidDef {
        pid: 0x33,
        name: "barometric_pressure",
        unit: "kPa",
        len: 1,
        tier: 1,
        decimals: 2,
        f: |d| d[0] as f64,
    },
    PidDef {
        pid: 0x42,
        name: "control_module_voltage",
        unit: "V",
        len: 2,
        tier: 1,
        decimals: 2,
        f: |d| ab(d) / 1000.0,
    },
    PidDef {
        pid: 0x46,
        name: "ambient_temp",
        unit: "°C",
        len: 1,
        tier: 1,
        decimals: 2,
        f: |d| d[0] as f64 - 40.0,
    },
    PidDef {
        pid: 0x5C,
        name: "oil_temp",
        unit: "°C",
        len: 1,
        tier: 1,
        decimals: 2,
        f: |d| d[0] as f64 - 40.0,
    },
    PidDef {
        pid: 0x5E,
        name: "fuel_rate",
        unit: "L/h",
        len: 2,
        tier: 1,
        decimals: 2,
        f: |d| ab(d) / 20.0,
    },
];

pub const DEFAULT_PIDS: &[&str] = &[
    "rpm",
    "speed",
    "engine_load",
    "throttle",
    "coolant_temp",
    "intake_air_temp",
    "maf",
    "short_fuel_trim_b1",
    "long_fuel_trim_b1",
    "intake_manifold_pressure",
    "timing_advance",
    "o2_b1s1_voltage",
    "o2_b1s2_voltage",
    "o2_b2s1_voltage",
    "o2_b2s2_voltage",
    "o2_b1s1_lambda",
    "o2_b2s1_lambda",
    "commanded_lambda",
    "control_module_voltage",
];

/// First definition with this sensor name. A name can map to several PIDs
/// (e.g. wideband lambda via 0x24.. or 0x34..); use [`resolve`] to read it.
pub fn pid_by_name(name: &str) -> Option<&'static PidDef> {
    PIDS.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

/// Definition for a sensor name, preferring a PID the ECU advertises.
pub fn resolve(
    name: &str,
    supported: Option<&std::collections::BTreeSet<u8>>,
) -> Option<&'static PidDef> {
    let mut defs = PIDS.iter().filter(|p| p.name.eq_ignore_ascii_case(name));
    match supported {
        Some(s) => defs
            .find(|p| s.contains(&p.pid))
            .or_else(|| pid_by_name(name)),
        None => defs.next(),
    }
}

/// Distinct sensor names in table order.
pub fn sensor_names() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for p in PIDS {
        if !out.contains(&p.name) {
            out.push(p.name);
        }
    }
    out
}

pub fn pid_by_code(code: u8) -> Option<&'static PidDef> {
    PIDS.iter().find(|p| p.pid == code)
}

#[cfg(test)]
pub fn defs_for_code(code: u8) -> impl Iterator<Item = &'static PidDef> {
    PIDS.iter().filter(move |p| p.pid == code)
}

pub fn decode_pid(def: &PidDef, data: &[u8]) -> Option<f64> {
    if data.len() < def.len {
        return None;
    }
    let v = (def.f)(data);
    let scale = 10f64.powi(def.decimals);
    v.is_finite().then_some((v * scale).round() / scale)
}

/// Oxygen sensor slots (index 0..8 of PIDs 0x14/0x24/0x34 + index) present
/// according to PID 0x13 (2 banks x 4) or 0x1D (4 banks x 2). Sensor names
/// always follow the 0x13 layout; the location is what the ECU reports.
pub fn o2_locations(layout_pid: u8, bitmap: u8) -> Vec<(String, String)> {
    (0..8)
        .filter(|i| bitmap >> i & 1 != 0)
        .map(|i| {
            let (bank, sensor) = if layout_pid == 0x1D {
                (i / 2 + 1, i % 2 + 1)
            } else {
                (i / 4 + 1, i % 4 + 1)
            };
            (
                format!("o2_b{}s{}", i / 4 + 1, i % 4 + 1),
                format!("bank {bank} sensor {sensor}"),
            )
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Dtc {
    pub code: String,
}

pub fn decode_dtc(a: u8, b: u8) -> String {
    let letter = ['P', 'C', 'B', 'U'][(a >> 6) as usize];
    format!("{}{:X}{:X}{:02X}", letter, (a >> 4) & 0x3, a & 0xF, b)
}

/// Decode a Mode 03/07/0A payload (bytes after the mode byte). CAN responses
/// carry a leading count byte (odd length); legacy protocols pad with 0000.
pub fn decode_dtc_payload(payload: &[u8], count_prefixed: bool) -> Vec<String> {
    let body = if count_prefixed {
        payload.get(1..).unwrap_or_default()
    } else {
        payload
    };
    body.chunks_exact(2)
        .filter(|c| c[0] != 0 || c[1] != 0)
        .map(|c| decode_dtc(c[0], c[1]))
        .collect()
}

#[derive(Debug, Serialize)]
pub struct Readiness {
    pub mil_on: bool,
    pub dtc_count: u8,
    pub compression_ignition: bool,
    pub monitors: Vec<MonitorStatus>,
}

#[derive(Debug, Serialize)]
pub struct MonitorStatus {
    pub name: &'static str,
    pub status: &'static str,
}

pub fn decode_readiness(d: &[u8]) -> Option<Readiness> {
    if d.len() < 4 {
        return None;
    }
    let compression = d[1] & 0x08 != 0;
    let mut monitors = Vec::new();
    let mut push = |name, supported: bool, incomplete: bool| {
        monitors.push(MonitorStatus {
            name,
            status: if !supported {
                "not_supported"
            } else if incomplete {
                "incomplete"
            } else {
                "complete"
            },
        });
    };
    push("misfire", d[1] & 1 != 0, d[1] & 0x10 != 0);
    push("fuel_system", d[1] & 2 != 0, d[1] & 0x20 != 0);
    push("components", d[1] & 4 != 0, d[1] & 0x40 != 0);
    if !compression {
        let names = [
            "catalyst",
            "heated_catalyst",
            "evap",
            "secondary_air",
            "ac_refrigerant",
            "o2_sensor",
            "o2_heater",
            "egr",
        ];
        for (i, n) in names.iter().enumerate() {
            push(n, d[2] >> i & 1 != 0, d[3] >> i & 1 != 0);
        }
    } else {
        let names = [
            "egr_vvt",
            "particulate_filter",
            "exhaust_gas_sensor",
            "boost_pressure",
            "nox_scr",
            "nmhc_catalyst",
        ];
        for (i, n) in names.iter().enumerate() {
            push(n, d[2] >> i & 1 != 0, d[3] >> i & 1 != 0);
        }
    }
    Some(Readiness {
        mil_on: d[0] & 0x80 != 0,
        dtc_count: d[0] & 0x7F,
        compression_ignition: compression,
        monitors,
    })
}

/// Parse a 4-byte supported-PID bitmap at base PID (0x00, 0x20, ...).
/// Returns the supported PIDs and whether the next range is advertised.
pub fn decode_supported(base: u8, d: &[u8]) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let mut cont = false;
    if d.len() < 4 {
        return (out, cont);
    }
    for i in 0..32u32 {
        let bit = d[(i / 8) as usize] >> (7 - (i % 8)) & 1 != 0;
        if bit {
            let pid = base as u32 + i + 1;
            if i == 31 {
                cont = true;
            } else {
                out.push(pid as u8);
            }
        }
    }
    (out, cont)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtc_decoding() {
        assert_eq!(decode_dtc(0x04, 0x20), "P0420");
        assert_eq!(decode_dtc(0x43, 0x01), "C0301");
        assert_eq!(decode_dtc(0xC1, 0x00), "U0100");
    }

    #[test]
    fn dtc_payload_can_and_legacy() {
        assert_eq!(decode_dtc_payload(&[0x01, 0x04, 0x20], true), vec!["P0420"]);
        assert_eq!(
            decode_dtc_payload(&[0x04, 0x20, 0x00, 0x00, 0x00, 0x00], false),
            vec!["P0420"]
        );
        assert!(decode_dtc_payload(&[0x00], false).is_empty());
        assert_eq!(
            decode_dtc_payload(&[0x01, 0x04, 0x20, 0x00, 0x00, 0x00], true),
            vec!["P0420"]
        );
        assert_eq!(
            decode_dtc_payload(&[0x02, 0x01, 0x71, 0x03, 0x01], true),
            vec!["P0171", "P0301"]
        );
        assert_eq!(
            decode_dtc_payload(&[0x01, 0x01, 0x00, 0x00, 0x00, 0x00], false),
            vec!["P0101"]
        );
    }

    #[test]
    fn pid_formulas() {
        assert_eq!(
            decode_pid(pid_by_name("rpm").unwrap(), &[0x1A, 0xF8]),
            Some(1726.0)
        );
        assert_eq!(
            decode_pid(pid_by_name("coolant_temp").unwrap(), &[0x7B]),
            Some(83.0)
        );
        assert_eq!(decode_pid(pid_by_name("rpm").unwrap(), &[0x1A]), None);
    }

    #[test]
    fn oxygen_sensor_pids_decode_every_value() {
        let narrow: Vec<_> = defs_for_code(0x14).collect();
        assert_eq!(narrow.len(), 2);
        let v = |name: &str, code: u8, d: &[u8]| {
            decode_pid(defs_for_code(code).find(|p| p.name == name).unwrap(), d)
        };
        assert_eq!(v("o2_b1s1_voltage", 0x14, &[0x5A, 0x80]), Some(0.45));
        assert_eq!(v("o2_b1s1_stft", 0x14, &[0x5A, 0x80]), Some(0.0));
        assert_eq!(v("o2_b1s1_stft", 0x14, &[0x5A, 0xFF]), None);
        assert_eq!(
            v("o2_b2s1_lambda", 0x28, &[0x80, 0x00, 0x40, 0x00]),
            Some(1.0)
        );
        assert_eq!(
            v("o2_b2s1_wr_voltage", 0x28, &[0x80, 0x00, 0x40, 0x00]),
            Some(2.0)
        );
        assert_eq!(
            v("o2_b1s1_lambda", 0x34, &[0x7F, 0x00, 0x80, 0x80]),
            Some(0.9922)
        );
        assert_eq!(
            v("o2_b1s1_current", 0x34, &[0x7F, 0x00, 0x80, 0x80]),
            Some(0.5)
        );
        assert_eq!(v("commanded_lambda", 0x44, &[0x80, 0x00]), Some(1.0));
    }

    #[test]
    fn lambda_name_resolves_to_the_advertised_pid() {
        let wide_current: std::collections::BTreeSet<u8> = [0x34].into();
        assert_eq!(resolve("o2_b1s1_lambda", None).unwrap().pid, 0x24);
        assert_eq!(
            resolve("o2_b1s1_lambda", Some(&wide_current)).unwrap().pid,
            0x34
        );
        assert_eq!(
            sensor_names()
                .iter()
                .filter(|n| **n == "o2_b1s1_lambda")
                .count(),
            1
        );
    }

    #[test]
    fn oxygen_sensor_locations_follow_the_reported_layout() {
        assert_eq!(
            o2_locations(0x13, 0b0001_0011),
            vec![
                ("o2_b1s1".into(), "bank 1 sensor 1".into()),
                ("o2_b1s2".into(), "bank 1 sensor 2".into()),
                ("o2_b2s1".into(), "bank 2 sensor 1".into()),
            ]
        );
        assert_eq!(o2_locations(0x1D, 0b0000_0100)[0].1, "bank 2 sensor 1");
    }

    #[test]
    fn supported_bitmap() {
        let (p, c) = decode_supported(0, &[0x98, 0x3B, 0x80, 0x13]);
        assert!(p.contains(&0x0C) && p.contains(&0x0D) && p.contains(&0x01));
        assert!(c);
    }

    #[test]
    fn readiness_flags() {
        let r = decode_readiness(&[0x81, 0x07, 0xE5, 0x01]).unwrap();
        assert!(r.mil_on);
        assert_eq!(r.dtc_count, 1);
        let cat = r.monitors.iter().find(|m| m.name == "catalyst").unwrap();
        assert_eq!(cat.status, "incomplete");
    }

    #[test]
    fn diesel_readiness_monitors_are_decoded() {
        let r = decode_readiness(&[0x00, 0x08, 0x10, 0x10]).unwrap();
        assert!(r.compression_ignition);
        let nox = r.monitors.iter().find(|m| m.name == "nox_scr").unwrap();
        assert_eq!(nox.status, "incomplete");
        assert!(r.monitors.iter().any(|m| m.name == "particulate_filter"));
    }
}
