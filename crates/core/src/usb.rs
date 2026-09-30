//! Liaison USB d'un disque branché par un adaptateur ou un boîtier : vitesse négociée avec le
//! port, capacités du port et de l'adaptateur, mode de transfert.
//!
//! La vitesse négociée est la plus basse des trois : port de l'ordinateur, câble, adaptateur.
//! Elle borne le débit mesuré : un bon disque branché en USB 2.0 ne dépasse jamais ~40 Mo/s, et
//! le test de vitesse ne doit pas le juger « faible » pour autant.
//!
//! - Windows : on remonte l'arbre des périphériques du disque jusqu'au périphérique USB, puis on
//!   interroge son concentrateur (hub) sur le port où il est branché
//!   (`IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX` et `_EX_V2`, comme l'outil USBView).
//! - Linux : `/sys` expose la vitesse de chaque périphérique USB (`speed`, en Mb/s).

use serde::Serialize;

/// Génération de la liaison USB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsbSpeed {
    /// USB 1.x (1,5 ou 12 Mb/s).
    Usb1,
    /// USB 2.0 (480 Mb/s).
    Usb2,
    /// USB 3.0, 3.1 Gen 1, 3.2 Gen 1 (5 Gb/s).
    Gen1,
    /// USB 3.1 Gen 2, 3.2 Gen 2 (10 Gb/s).
    Gen2,
    /// USB 3.2 Gen 2x2 (20 Gb/s) et au-delà.
    Gen2x2,
}

impl UsbSpeed {
    /// Débit réel maximal d'un disque sur cette liaison (Mo/s), protocole déduit.
    pub fn practical_mbps(self) -> f64 {
        match self {
            UsbSpeed::Usb1 => 1.0,
            UsbSpeed::Usb2 => 40.0,
            UsbSpeed::Gen1 => 420.0,
            UsbSpeed::Gen2 => 1000.0,
            UsbSpeed::Gen2x2 => 2000.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            UsbSpeed::Usb1 => "USB 1.1 (12 Mb/s)",
            UsbSpeed::Usb2 => "USB 2.0 (480 Mb/s)",
            UsbSpeed::Gen1 => "USB 3.2 Gen 1 (5 Gb/s)",
            UsbSpeed::Gen2 => "USB 3.2 Gen 2 (10 Gb/s)",
            UsbSpeed::Gen2x2 => "USB 3.2 Gen 2x2 (20 Gb/s)",
        }
    }

    /// Vitesse en Mb/s telle que Linux l'écrit dans `/sys/.../speed`.
    pub fn from_mbits(mbits: u32) -> UsbSpeed {
        match mbits {
            0..=12 => UsbSpeed::Usb1,
            13..=480 => UsbSpeed::Usb2,
            481..=5000 => UsbSpeed::Gen1,
            5001..=10000 => UsbSpeed::Gen2,
            _ => UsbSpeed::Gen2x2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UsbLink {
    /// Vitesse négociée actuellement.
    pub speed: UsbSpeed,
    /// Vitesse maximale que l'adaptateur sait faire, si connue (Windows).
    pub device_capable: Option<UsbSpeed>,
    /// Vitesse maximale du port de l'ordinateur, si connue.
    pub port_capable: Option<UsbSpeed>,
    /// `Some(true)` : mode UAS (moderne) ; `Some(false)` : ancien mode « stockage de masse »
    /// (BOT), plus lent sur les petits fichiers.
    pub uas: Option<bool>,
}

impl UsbLink {
    /// Débit maximal attendu sur cette liaison, en Mo/s.
    pub fn cap_mbps(&self) -> f64 {
        self.speed.practical_mbps()
    }

    /// Conseil quand la liaison est plus lente que ce que permettent le port ou l'adaptateur.
    pub fn advice(&self) -> Option<String> {
        let device = self.device_capable.unwrap_or(self.speed);
        let port = self.port_capable.unwrap_or(self.speed);
        if device > self.speed && port > self.speed {
            Some(format!(
                "L'adaptateur et le port savent aller plus vite ({}) : câble ou rallonge à changer.",
                device.min(port).label()
            ))
        } else if device > self.speed {
            Some(
                "Le port de l'ordinateur est plus lent que l'adaptateur : branche le disque sur un \
                 port USB 3 (souvent bleu ou marqué SS) pour mesurer sa vraie vitesse."
                    .into(),
            )
        } else if port > self.speed {
            Some(
                "L'adaptateur (boîtier ou câble USB) est plus lent que le port : c'est lui qui \
                 limite la vitesse, pas le disque."
                    .into(),
            )
        } else {
            None
        }
    }

    /// Phrase complète pour l'écran et le rapport.
    pub fn describe(&self) -> String {
        let mut s = format!(
            "Branché en {} : environ {} Mo/s au maximum, quel que soit le disque.",
            self.speed.label(),
            self.cap_mbps() as u64
        );
        if self.uas == Some(false) && self.speed >= UsbSpeed::Gen1 {
            s.push_str(" Adaptateur en ancien mode de transfert (pas UAS) : plus lent sur les petits fichiers.");
        }
        if let Some(a) = self.advice() {
            s.push(' ');
            s.push_str(&a);
        }
        s
    }
}

/// Liaison USB du disque nommé par smartctl (`/dev/sdb`), `None` s'il n'est pas branché en USB
/// ou si l'information est illisible.
pub fn usb_link(smartctl_name: &str) -> Option<UsbLink> {
    #[cfg(windows)]
    {
        let index = crate::devtree::physical_drive_index(smartctl_name)?;
        win::usb_link(index)
    }
    #[cfg(target_os = "linux")]
    {
        let name = smartctl_name.strip_prefix("/dev/")?;
        linux::usb_link(name)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = smartctl_name;
        None
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::Path;

    use super::{UsbLink, UsbSpeed};

    fn read_speed(dir: &Path) -> Option<UsbSpeed> {
        let text = std::fs::read_to_string(dir.join("speed")).ok()?;
        // « 480 », « 5000 », « 1.5 » : partie entière suffisante.
        let mbits = text.trim().split('.').next()?.parse().ok()?;
        Some(UsbSpeed::from_mbits(mbits))
    }

    /// `/sys/class/block/sdb` pointe vers `.../usb2/2-1/2-1:1.0/host3/.../block/sdb` : le premier
    /// ancêtre avec un fichier `speed` et `idVendor` est le périphérique USB ; son interface
    /// (`2-1:1.0`) porte le pilote (`uas` ou `usb-storage`) ; son parent est le concentrateur.
    pub(super) fn usb_link(block: &str) -> Option<UsbLink> {
        let real = std::fs::canonicalize(Path::new("/sys/class/block").join(block)).ok()?;
        let mut interface = None;
        let mut dir = real.as_path();
        while let Some(parent) = dir.parent() {
            if parent.join("idVendor").is_file() && parent.join("speed").is_file() {
                let speed = read_speed(parent)?;
                let port_capable = parent.parent().and_then(read_speed);
                let uas = interface.and_then(|i: &Path| {
                    let driver = std::fs::read_link(i.join("driver")).ok()?;
                    Some(driver.file_name()?.to_str()? == "uas")
                });
                return Some(UsbLink {
                    speed,
                    device_capable: None,
                    port_capable,
                    uas,
                });
            }
            interface = Some(dir);
            dir = parent;
        }
        None
    }
}

#[cfg(windows)]
mod win {
    use std::ptr::null_mut;

    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_Device_Interface_ListW, CM_Get_Device_Interface_List_SizeW, CM_DRP_ADDRESS,
        CM_DRP_SERVICE,
    };
    use windows_sys::Win32::Foundation::GENERIC_WRITE;
    use windows_sys::Win32::System::IO::DeviceIoControl;

    use super::{UsbLink, UsbSpeed};
    use crate::devtree::{
        device_id, disk_ancestor, open, parent, registry_string, registry_u32, wide, Handle,
        CR_SUCCESS,
    };

    const GUID_DEVINTERFACE_USB_HUB: GUID = GUID::from_u128(0xf18a0e88_c30c_11d0_8815_00a0c906bed8);
    /// CTL_CODE(FILE_DEVICE_USB, 274 / 279, METHOD_BUFFERED, FILE_ANY_ACCESS).
    const IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX: u32 = 0x0022_0448;
    const IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX_V2: u32 = 0x0022_045C;

    /// Chemin d'interface du concentrateur `hub` (pour lui envoyer des requêtes).
    fn hub_path(hub: u32) -> Option<Vec<u16>> {
        let id = wide(&device_id(hub)?);
        let mut size = 0u32;
        // SAFETY : identifiant terminé par zéro ; tampon dimensionné par l'appel précédent.
        unsafe {
            if CM_Get_Device_Interface_List_SizeW(
                &mut size,
                &GUID_DEVINTERFACE_USB_HUB,
                id.as_ptr(),
                0,
            ) != CR_SUCCESS
                || size <= 1
            {
                return None;
            }
            let mut buf = vec![0u16; size as usize];
            if CM_Get_Device_Interface_ListW(
                &GUID_DEVINTERFACE_USB_HUB,
                id.as_ptr(),
                buf.as_mut_ptr(),
                size,
                0,
            ) != CR_SUCCESS
            {
                return None;
            }
            // Liste de chaînes terminées par zéro : la première suffit.
            let n = buf.iter().position(|&c| c == 0)?;
            let mut path = buf[..n].to_vec();
            path.push(0);
            Some(path)
        }
    }

    fn ioctl(h: &Handle, code: u32, buf: &mut [u8]) -> bool {
        let mut returned = 0u32;
        // SAFETY : même tampon en entrée et en sortie, de la taille annoncée.
        unsafe {
            DeviceIoControl(
                h.0,
                code,
                buf.as_ptr().cast(),
                buf.len() as u32,
                buf.as_mut_ptr().cast(),
                buf.len() as u32,
                &mut returned,
                null_mut(),
            ) != 0
        }
    }

    pub(super) fn usb_link(index: u32) -> Option<UsbLink> {
        // Remonte du disque jusqu'au premier ancêtre « USB\... » : le périphérique USB (boîtier
        // ou adaptateur). Son service dit le mode : UASPStor (UAS) ou USBSTOR (ancien).
        let usb = disk_ancestor(index, "USB\\")?;
        let uas = registry_string(usb, CM_DRP_SERVICE).map(|s| s.eq_ignore_ascii_case("UASPStor"));
        let port = registry_u32(usb, CM_DRP_ADDRESS)?;
        let hub = open(&hub_path(parent(usb)?)?, GENERIC_WRITE)?;

        // Vitesse négociée : octet `Speed` de USB_NODE_CONNECTION_INFORMATION_EX (structure
        // compacte : index de port sur 4 octets, descripteur de 18, configuration, puis Speed).
        let mut ex = [0u8; 512];
        ex[..4].copy_from_slice(&port.to_le_bytes());
        if !ioctl(&hub, IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX, &mut ex) {
            return None;
        }
        let mut speed = match ex[23] {
            0 | 1 => UsbSpeed::Usb1,
            2 => UsbSpeed::Usb2,
            _ => UsbSpeed::Gen1,
        };

        // Détail USB 3 : protocoles du port et drapeaux « fonctionne en / sait faire ».
        let mut v2 = [0u8; 16];
        v2[..4].copy_from_slice(&port.to_le_bytes());
        v2[4..8].copy_from_slice(&16u32.to_le_bytes());
        v2[8..12].copy_from_slice(&0b111u32.to_le_bytes()); // USB 1.1, 2.0 et 3.0 demandés
        let (mut device_capable, mut port_capable) = (None, None);
        if ioctl(
            &hub,
            IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX_V2,
            &mut v2,
        ) {
            let protocols = u32::from_le_bytes(v2[8..12].try_into().unwrap_or_default());
            let flags = u32::from_le_bytes(v2[12..16].try_into().unwrap_or_default());
            let (ss_now, ss_able, ssp_now, ssp_able) = (
                flags & 1 != 0,
                flags & 2 != 0,
                flags & 4 != 0,
                flags & 8 != 0,
            );
            if ssp_now {
                speed = UsbSpeed::Gen2;
            } else if ss_now && speed < UsbSpeed::Gen1 {
                speed = UsbSpeed::Gen1;
            }
            device_capable = Some(if ssp_able {
                UsbSpeed::Gen2
            } else if ss_able {
                UsbSpeed::Gen1
            } else {
                speed.min(UsbSpeed::Usb2)
            });
            port_capable = Some(if protocols & 0b100 != 0 {
                // Port USB 3 : Gen 2 si le périphérique y fonctionne déjà en SuperSpeed+.
                if ssp_now {
                    UsbSpeed::Gen2
                } else {
                    UsbSpeed::Gen1
                }
            } else if protocols & 0b010 != 0 {
                UsbSpeed::Usb2
            } else {
                UsbSpeed::Usb1
            });
        }
        Some(UsbLink {
            speed,
            device_capable,
            port_capable,
            uas,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_speeds_are_mapped() {
        assert_eq!(UsbSpeed::from_mbits(480), UsbSpeed::Usb2);
        assert_eq!(UsbSpeed::from_mbits(5000), UsbSpeed::Gen1);
        assert_eq!(UsbSpeed::from_mbits(10000), UsbSpeed::Gen2);
        assert_eq!(UsbSpeed::from_mbits(20000), UsbSpeed::Gen2x2);
        assert_eq!(UsbSpeed::from_mbits(12), UsbSpeed::Usb1);
    }

    #[test]
    fn usb3_adapter_on_usb2_port_is_explained() {
        let link = UsbLink {
            speed: UsbSpeed::Usb2,
            device_capable: Some(UsbSpeed::Gen1),
            port_capable: Some(UsbSpeed::Usb2),
            uas: Some(true),
        };
        assert_eq!(link.cap_mbps(), 40.0);
        assert!(link.describe().contains("port USB 3"));

        let bad_cable = UsbLink {
            port_capable: Some(UsbSpeed::Gen1),
            ..link.clone()
        };
        assert!(bad_cable.describe().contains("câble"));

        let usb2_adapter = UsbLink {
            device_capable: Some(UsbSpeed::Usb2),
            port_capable: Some(UsbSpeed::Gen1),
            ..link
        };
        assert!(usb2_adapter.describe().contains("adaptateur"));
    }

    #[test]
    fn full_speed_link_gives_no_advice() {
        let link = UsbLink {
            speed: UsbSpeed::Gen1,
            device_capable: Some(UsbSpeed::Gen1),
            port_capable: Some(UsbSpeed::Gen1),
            uas: Some(false),
        };
        assert_eq!(link.advice(), None);
        assert!(link.describe().contains("pas UAS"));
    }

    /// Affiche la liaison USB de chaque disque de la machine :
    /// `cargo test -p pccheck-core --lib usb::tests::real -- --ignored --nocapture`.
    #[test]
    #[ignore = "lit les disques de la machine"]
    fn real_disks_links() {
        for letter in 'a'..='h' {
            let name = format!("/dev/sd{letter}");
            println!("{name} : {:?}", usb_link(&name));
        }
    }
}
