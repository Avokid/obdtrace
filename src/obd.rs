//! OBD-II PID table, response parsing and DTC decoding.

use serde::Serialize;

pub struct PidDef {
    pub pid: u8,
    pub name: &'static str,
    pub unit: &'static str,
    pub len: usize,
    /// 0 = polled every cycle, 1 = every 5th cycle
    pub tier: u8,
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
        f: |d| d[0] as f64 * 100.0 / 255.0,
    },
    PidDef {
        pid: 0x05,
        name: "coolant_temp",
        unit: "°C",
        len: 1,
        tier: 1,
        f: |d| d[0] as f64 - 40.0,
    },
    PidDef {
        pid: 0x06,
        name: "short_fuel_trim_b1",
        unit: "%",
        len: 1,
        tier: 0,
        f: |d| d[0] as f64 * 100.0 / 128.0 - 100.0,
    },
    PidDef {
        pid: 0x07,
        name: "long_fuel_trim_b1",
        unit: "%",
        len: 1,
        tier: 0,
        f: |d| d[0] as f64 * 100.0 / 128.0 - 100.0,
    },
    PidDef {
        pid: 0x08,
        name: "short_fuel_trim_b2",
        unit: "%",
        len: 1,
        tier: 1,
        f: |d| d[0] as f64 * 100.0 / 128.0 - 100.0,
    },
    PidDef {
        pid: 0x09,
        name: "long_fuel_trim_b2",
        unit: "%",
        len: 1,
        tier: 1,
        f: |d| d[0] as f64 * 100.0 / 128.0 - 100.0,
    },
    PidDef {
        pid: 0x0B,
        name: "intake_manifold_pressure",
        unit: "kPa",
        len: 1,
        tier: 0,
        f: |d| d[0] as f64,
    },
    PidDef {
        pid: 0x0C,
        name: "rpm",
        unit: "rpm",
        len: 2,
        tier: 0,
        f: |d| ab(d) / 4.0,
    },
    PidDef {
        pid: 0x0D,
        name: "speed",
        unit: "km/h",
        len: 1,
        tier: 0,
        f: |d| d[0] as f64,
    },
    PidDef {
        pid: 0x0E,
        name: "timing_advance",
        unit: "°",
        len: 1,
        tier: 0,
        f: |d| d[0] as f64 / 2.0 - 64.0,
    },
    PidDef {
        pid: 0x0F,
        name: "intake_air_temp",
        unit: "°C",
        len: 1,
        tier: 1,
        f: |d| d[0] as f64 - 40.0,
    },
    PidDef {
        pid: 0x10,
        name: "maf",
        unit: "g/s",
        len: 2,
        tier: 0,
        f: |d| ab(d) / 100.0,
    },
    PidDef {
        pid: 0x11,
        name: "throttle",
        unit: "%",
        len: 1,
        tier: 0,
        f: |d| d[0] as f64 * 100.0 / 255.0,
    },
    PidDef {
        pid: 0x14,
        name: "o2_b1s1_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x15,
        name: "o2_b1s2_voltage",
        unit: "V",
        len: 2,
        tier: 0,
        f: |d| d[0] as f64 / 200.0,
    },
    PidDef {
        pid: 0x1F,
        name: "run_time",
        unit: "s",
        len: 2,
        tier: 1,
        f: |d| ab(d),
    },
    PidDef {
        pid: 0x2F,
        name: "fuel_level",
        unit: "%",
        len: 1,
        tier: 1,
        f: |d| d[0] as f64 * 100.0 / 255.0,
    },
    PidDef {
        pid: 0x31,
        name: "distance_since_clear",
        unit: "km",
        len: 2,
        tier: 1,
        f: |d| ab(d),
    },
    PidDef {
        pid: 0x33,
        name: "barometric_pressure",
        unit: "kPa",
        len: 1,
        tier: 1,
        f: |d| d[0] as f64,
    },
    PidDef {
        pid: 0x42,
        name: "control_module_voltage",
        unit: "V",
        len: 2,
        tier: 1,
        f: |d| ab(d) / 1000.0,
    },
    PidDef {
        pid: 0x46,
        name: "ambient_temp",
        unit: "°C",
        len: 1,
        tier: 1,
        f: |d| d[0] as f64 - 40.0,
    },
    PidDef {
        pid: 0x5C,
        name: "oil_temp",
        unit: "°C",
        len: 1,
        tier: 1,
        f: |d| d[0] as f64 - 40.0,
    },
    PidDef {
        pid: 0x5E,
        name: "fuel_rate",
        unit: "L/h",
        len: 2,
        tier: 1,
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
    "control_module_voltage",
];

pub fn pid_by_name(name: &str) -> Option<&'static PidDef> {
    PIDS.iter().find(|p| p.name.eq_ignore_ascii_case(name))
}

pub fn pid_by_code(code: u8) -> Option<&'static PidDef> {
    PIDS.iter().find(|p| p.pid == code)
}

pub fn decode_pid(def: &PidDef, data: &[u8]) -> Option<f64> {
    if data.len() < def.len {
        return None;
    }
    let v = (def.f)(data);
    v.is_finite().then_some(round(v))
}

fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
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
