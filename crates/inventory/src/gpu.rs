//! Carte graphique sous charge : capteurs NVIDIA (`nvidia-smi`, installé avec le pilote) et
//! analyse d'un test de charge fait dans la vue web (WebGL), dont l'interface envoie les mesures.
//!
//! AMD et Intel n'ont pas d'équivalent fourni avec le pilote : le test reste possible (débit,
//! erreurs de rendu), sans température.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pccheck_core::process;
use serde::{Deserialize, Serialize};

use crate::stress::{analyse_throttling, ThrottleAnalysis};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GpuSensor {
    pub name: String,
    pub temperature_c: Option<f64>,
    pub sm_clock_mhz: Option<f64>,
    pub power_w: Option<f64>,
    pub utilization_pct: Option<f64>,
    /// Degrés restants avant que la carte se bride elle-même (NVIDIA « tlimit »).
    pub degrees_to_throttle: Option<f64>,
}

const FIELDS: &str = "name,temperature.gpu,clocks.sm,power.draw,utilization.gpu";
/// Degrés avant bridage : champ récent, refusé par les anciens pilotes (toute la requête échoue).
const TLIMIT: &str = ",temperature.gpu.tlimit";

fn nvidia_smi() -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let programs =
            std::env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into());
        vec![
            // Pilotes DCH (depuis 2019), puis anciens pilotes.
            PathBuf::from(root).join("System32").join("nvidia-smi.exe"),
            PathBuf::from(programs).join(r"NVIDIA Corporation\NVSMI\nvidia-smi.exe"),
        ]
    } else {
        vec![
            "/usr/bin/nvidia-smi".into(),
            "/usr/local/bin/nvidia-smi".into(),
        ]
    };
    candidates.into_iter().find(|p| p.is_file())
}

/// Capteurs des cartes NVIDIA ; liste vide sans carte NVIDIA ou sans pilote.
pub fn gpu_sensors() -> Vec<GpuSensor> {
    let Some(exe) = nvidia_smi() else {
        return Vec::new();
    };
    read_sensors(&exe)
}

fn read_sensors(exe: &Path) -> Vec<GpuSensor> {
    let query = |fields: &str| {
        let arg = format!("--query-gpu={fields}");
        process::run(
            exe,
            &[&arg, "--format=csv,noheader,nounits"],
            Duration::from_secs(5),
        )
        .ok()
        // Pilote absent ou carte retirée : nvidia-smi écrit son message d'erreur sur la sortie
        // standard, qu'il ne faut pas prendre pour une carte.
        .filter(|o| o.code == Some(0))
        .map(|o| parse_nvidia_csv(&o.stdout))
    };
    query(&format!("{FIELDS}{TLIMIT}"))
        .or_else(|| query(FIELDS))
        .unwrap_or_default()
}

/// Analyse la sortie CSV de `nvidia-smi` (une ligne par carte). « [N/A] » ou « [Not Supported] »
/// donnent `None`.
pub fn parse_nvidia_csv(text: &str) -> Vec<GpuSensor> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let cols: Vec<&str> = l.split(',').map(str::trim).collect();
            // Au moins les 5 champs de base : une ligne de texte n'est pas une carte.
            if cols.len() < 5 {
                return None;
            }
            let num = |i: usize| cols.get(i).and_then(|v| v.parse::<f64>().ok());
            Some(GpuSensor {
                name: cols.first().filter(|n| !n.is_empty())?.to_string(),
                temperature_c: num(1),
                sm_clock_mhz: num(2),
                power_w: num(3),
                utilization_pct: num(4),
                degrees_to_throttle: num(5),
            })
        })
        .collect()
}

/// Mesure d'une seconde du test WebGL, envoyée par l'interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuSample {
    pub t_s: f64,
    /// Rendus complets du shader de charge pendant cette seconde.
    pub passes_per_s: f64,
    pub temperature_c: Option<f64>,
}

/// Données brutes du test, envoyées par l'interface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuTestInput {
    /// Moteur de rendu déclaré par WebGL (souvent le nom de la carte).
    pub renderer: Option<String>,
    pub samples: Vec<GpuSample>,
    /// Images de contrôle différentes de la référence (rendu instable).
    pub render_errors: u32,
    pub checks: u32,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GpuTestResult {
    pub input: GpuTestInput,
    pub duration_s: f64,
    pub max_temperature_c: Option<f64>,
    pub throttling: ThrottleAnalysis,
}

/// Même analyse du bridage que pour le processeur (référence prise après la montée en fréquence).
pub fn analyse_gpu_test(input: GpuTestInput) -> GpuTestResult {
    let rates: Vec<f64> = input.samples.iter().map(|s| s.passes_per_s).collect();
    let max_temperature_c = input
        .samples
        .iter()
        .filter_map(|s| s.temperature_c)
        .fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.max(t))));
    GpuTestResult {
        duration_s: input.samples.last().map_or(0.0, |s| s.t_s),
        max_temperature_c,
        throttling: analyse_throttling(&rates),
        input,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stress::ThrottleLevel;

    #[test]
    fn nvidia_csv_is_parsed_with_missing_values() {
        let s = parse_nvidia_csv("NVIDIA GeForce RTX 3060, 43, 210, 15.70, 0, 22\nNVIDIA T400, 38, [N/A], [Not Supported], 3, [N/A]\n");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].temperature_c, Some(43.0));
        assert_eq!(s[0].power_w, Some(15.7));
        assert_eq!(s[0].degrees_to_throttle, Some(22.0));
        assert_eq!(s[1].sm_clock_mhz, None);
        assert_eq!(s[1].power_w, None);
        let error = "NVIDIA-SMI has failed because it couldn't communicate with the NVIDIA driver.";
        assert!(parse_nvidia_csv(error).is_empty());
    }

    #[test]
    fn gpu_test_is_analysed_like_cpu() {
        let samples = (0..120)
            .map(|i| GpuSample {
                t_s: f64::from(i + 1),
                passes_per_s: if i < 80 { 100.0 } else { 60.0 },
                temperature_c: Some(60.0 + f64::from(i) * 0.2),
            })
            .collect();
        let r = analyse_gpu_test(GpuTestInput {
            renderer: None,
            samples,
            render_errors: 0,
            checks: 12,
            cancelled: false,
        });
        assert_eq!(r.throttling.level, ThrottleLevel::Sustained);
        assert_eq!(r.duration_s, 120.0);
        assert!((r.max_temperature_c.unwrap() - 83.8).abs() < 1e-9);
    }
}
