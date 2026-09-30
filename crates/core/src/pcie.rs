//! Liaison PCIe d'un SSD NVMe : génération (PCIe 3.0, 4.0...) et nombre de voies (x2, x4),
//! actuelles et maximales.
//!
//! Un SSD PCIe 4.0 x4 dans un emplacement PCIe 3.0, ou câblé en x2, est bridé : le test de
//! vitesse ne doit pas le juger « faible » pour autant. Attention : certaines machines baissent
//! la vitesse du lien au repos (économie d'énergie) ; la valeur actuelle est celle du moment de
//! la lecture.
//!
//! - Windows : propriétés `DEVPKEY_PciDevice_{Current,Max}Link{Speed,Width}` du contrôleur NVMe
//!   (premier ancêtre « PCI\ » du disque dans l'arbre des périphériques).
//! - Linux : `current_link_speed`, `current_link_width`, `max_link_speed`, `max_link_width` du
//!   périphérique PCI dans `/sys/class/nvme/nvmeN/device/`.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PcieLink {
    /// Génération : 1 (2,5 GT/s) à 5 (32 GT/s) et au-delà.
    pub current_gen: u8,
    pub current_lanes: u8,
    pub max_gen: Option<u8>,
    pub max_lanes: Option<u8>,
}

/// Débit utile d'une voie par génération, en Mo/s (codage 8b/10b jusqu'à PCIe 2.0, 128b/130b
/// ensuite).
fn lane_mbps(generation: u8) -> f64 {
    match generation {
        0 | 1 => 250.0,
        2 => 500.0,
        3 => 985.0,
        4 => 1969.0,
        5 => 3938.0,
        _ => 7877.0,
    }
}

impl PcieLink {
    /// Débit réel maximal attendu d'un SSD sur ce lien (Mo/s) : environ 90 % du débit utile
    /// (protocole NVMe et contrôleur).
    pub fn cap_mbps(&self) -> f64 {
        lane_mbps(self.current_gen) * f64::from(self.current_lanes.max(1)) * 0.9
    }

    pub fn label(generation: u8, lanes: u8) -> String {
        let version = match generation {
            1 => "1.0",
            2 => "2.0",
            3 => "3.0",
            4 => "4.0",
            5 => "5.0",
            6 => "6.0",
            _ => "?",
        };
        format!("PCIe {version} x{lanes}")
    }

    /// Lien actuel plus lent que ce que le disque sait faire.
    pub fn is_degraded(&self) -> bool {
        self.max_gen.is_some_and(|g| g > self.current_gen)
            || self.max_lanes.is_some_and(|l| l > self.current_lanes)
    }

    /// Phrase complète pour l'écran et le rapport.
    pub fn describe(&self) -> String {
        let now = Self::label(self.current_gen, self.current_lanes);
        let mut s = format!(
            "Branché en {now} : environ {} Mo/s au maximum.",
            (self.cap_mbps() / 10.0).round() as u64 * 10
        );
        if self.is_degraded() {
            let max = Self::label(
                self.max_gen.unwrap_or(self.current_gen),
                self.max_lanes.unwrap_or(self.current_lanes),
            );
            s.push_str(&format!(
                " Le disque sait faire {max} : emplacement ou carte mère plus lents, ou lien \
                 en économie d'énergie au moment de la lecture."
            ));
        }
        s
    }
}

/// Liaison PCIe du disque nommé par smartctl (`/dev/sda` sous Windows, `/dev/nvme0` sous Linux),
/// `None` s'il n'est pas sur un lien PCIe lisible (SATA, USB, contrôleur RAID).
pub fn pcie_link(smartctl_name: &str) -> Option<PcieLink> {
    #[cfg(windows)]
    {
        win::pcie_link(crate::devtree::physical_drive_index(smartctl_name)?)
    }
    #[cfg(target_os = "linux")]
    {
        linux::pcie_link(smartctl_name)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = smartctl_name;
        None
    }
}

/// « 16.0 GT/s PCIe » ou « 8 GT/s » (Linux) → génération.
pub fn generation_from_gts(text: &str) -> Option<u8> {
    let gts: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(match gts {
        g if g < 3.0 => 1,
        g if g < 6.0 => 2,
        g if g < 12.0 => 3,
        g if g < 24.0 => 4,
        g if g < 48.0 => 5,
        _ => 6,
    })
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::Path;

    use super::{generation_from_gts, PcieLink};

    pub(super) fn pcie_link(smartctl_name: &str) -> Option<PcieLink> {
        // /dev/nvme0 ou /dev/nvme0n1 → contrôleur nvme0.
        let name = smartctl_name.strip_prefix("/dev/")?;
        let controller: String = name
            .strip_prefix("nvme")?
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let dir = Path::new("/sys/class/nvme")
            .join(format!("nvme{controller}"))
            .join("device");
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok();
        let lanes = |f: &str| read(f)?.trim().parse::<u8>().ok();
        Some(PcieLink {
            current_gen: generation_from_gts(&read("current_link_speed")?)?,
            current_lanes: lanes("current_link_width")?,
            max_gen: read("max_link_speed").and_then(|t| generation_from_gts(&t)),
            max_lanes: lanes("max_link_width"),
        })
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::Properties::DEVPROPKEY;

    use super::PcieLink;
    use crate::devtree::{disk_ancestor, property_u32};

    const PCI: GUID = GUID::from_u128(0x3ab22e31_8264_4b4e_9af5_a8d2d8e33e62);
    const fn key(pid: u32) -> DEVPROPKEY {
        DEVPROPKEY { fmtid: PCI, pid }
    }
    // DEVPKEY_PciDevice_CurrentLinkSpeed (9), CurrentLinkWidth (10), MaxLinkSpeed (11),
    // MaxLinkWidth (12) : vitesse codée comme dans la norme PCIe (1 = 2,5 GT/s... 4 = 16 GT/s),
    // largeur en nombre de voies.
    const CURRENT_SPEED: DEVPROPKEY = key(9);
    const CURRENT_WIDTH: DEVPROPKEY = key(10);
    const MAX_SPEED: DEVPROPKEY = key(11);
    const MAX_WIDTH: DEVPROPKEY = key(12);

    pub(super) fn pcie_link(index: u32) -> Option<PcieLink> {
        let controller = disk_ancestor(index, "PCI\\")?;
        let small = |v: u32| u8::try_from(v).ok().filter(|&n| n > 0);
        Some(PcieLink {
            current_gen: small(property_u32(controller, &CURRENT_SPEED)?)?,
            current_lanes: small(property_u32(controller, &CURRENT_WIDTH)?)?,
            max_gen: property_u32(controller, &MAX_SPEED).and_then(small),
            max_lanes: property_u32(controller, &MAX_WIDTH).and_then(small),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_speeds_give_generations() {
        assert_eq!(generation_from_gts("2.5 GT/s PCIe"), Some(1));
        assert_eq!(generation_from_gts("8.0 GT/s PCIe"), Some(3));
        assert_eq!(generation_from_gts("16.0 GT/s PCIe"), Some(4));
        assert_eq!(generation_from_gts("32.0 GT/s PCIe"), Some(5));
        assert_eq!(generation_from_gts("Unknown"), None);
    }

    #[test]
    fn caps_and_degraded_links() {
        let gen4 = PcieLink {
            current_gen: 4,
            current_lanes: 4,
            max_gen: Some(4),
            max_lanes: Some(4),
        };
        assert!((gen4.cap_mbps() - 7088.4).abs() < 1.0);
        assert!(!gen4.is_degraded());
        assert!(gen4.describe().starts_with("Branché en PCIe 4.0 x4"));

        let in_gen3_slot = PcieLink {
            current_gen: 3,
            ..gen4
        };
        assert!(in_gen3_slot.is_degraded());
        assert!(in_gen3_slot.describe().contains("sait faire PCIe 4.0 x4"));
    }

    /// Affiche la liaison PCIe de chaque disque de la machine :
    /// `cargo test -p pccheck-core --lib pcie::tests::real -- --ignored --nocapture`.
    #[test]
    #[ignore = "lit les disques de la machine"]
    fn real_disks_links() {
        for name in [
            "/dev/sda",
            "/dev/sdb",
            "/dev/sdc",
            "/dev/sdd",
            "/dev/nvme0",
            "/dev/nvme1",
        ] {
            println!("{name} : {:?}", pcie_link(name));
        }
    }
}
