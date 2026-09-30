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
        let index = crate::rawio::windows_raw_path(smartctl_name)?
            .strip_prefix(r"\\.\PhysicalDrive")?
            .parse::<u32>()
            .ok()?;
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
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::ptr::{null, null_mut};

    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_Registry_PropertyW, CM_Get_Device_IDW, CM_Get_Device_Interface_ListW,
        CM_Get_Device_Interface_List_SizeW, CM_Get_Parent, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
        CM_DRP_ADDRESS, CM_DRP_SERVICE, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    };
    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Ioctl::{
        IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;

    use super::{UsbLink, UsbSpeed};

    const GUID_DEVINTERFACE_DISK: GUID = GUID::from_u128(0x53f56307_b6bf_11d0_94f2_00a0c91efb8b);
    const GUID_DEVINTERFACE_USB_HUB: GUID = GUID::from_u128(0xf18a0e88_c30c_11d0_8815_00a0c906bed8);
    /// CTL_CODE(FILE_DEVICE_USB, 274 / 279, METHOD_BUFFERED, FILE_ANY_ACCESS).
    const IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX: u32 = 0x0022_0448;
    const IOCTL_USB_GET_NODE_CONNECTION_INFORMATION_EX_V2: u32 = 0x0022_045C;
    const CR_SUCCESS: u32 = 0;

    struct Handle(HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY : poignée ouverte par CreateFileW, fermée une seule fois.
            unsafe { CloseHandle(self.0) };
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn open(path: &[u16], access: u32) -> Option<Handle> {
        // SAFETY : chemin terminé par zéro ; aucune structure de sécurité.
        let h = unsafe {
            CreateFileW(
                path.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        (h != INVALID_HANDLE_VALUE).then_some(Handle(h))
    }

    /// Nœud de périphérique (devinst) du disque physique `index`.
    fn disk_devinst(index: u32) -> Option<u32> {
        // SAFETY : appels SetupAPI avec des structures locales dont `cbSize` est renseigné ;
        // la liste est détruite en fin de fonction.
        unsafe {
            let set = SetupDiGetClassDevsW(
                &GUID_DEVINTERFACE_DISK,
                null(),
                null_mut(),
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            );
            if set == -1 {
                return None;
            }
            let mut found = None;
            let mut i = 0;
            loop {
                let mut iface: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
                iface.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
                if SetupDiEnumDeviceInterfaces(set, null(), &GUID_DEVINTERFACE_DISK, i, &mut iface)
                    == 0
                {
                    break;
                }
                i += 1;
                let mut needed = 0u32;
                SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &iface,
                    null_mut(),
                    0,
                    &mut needed,
                    null_mut(),
                );
                if needed == 0 {
                    continue;
                }
                // Tampon aligné sur 8 octets pour la structure de détail.
                let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
                let detail = buf.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
                (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                let mut info: SP_DEVINFO_DATA = std::mem::zeroed();
                info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &iface,
                    detail,
                    needed,
                    null_mut(),
                    &mut info,
                ) == 0
                {
                    continue;
                }
                let path_ptr = std::ptr::addr_of!((*detail).DevicePath).cast::<u16>();
                let len = (0..).take_while(|&k| *path_ptr.add(k) != 0).count();
                let mut path: Vec<u16> = std::slice::from_raw_parts(path_ptr, len).to_vec();
                path.push(0);
                // Accès 0 : requêtes d'information seulement, sans droits administrateur.
                let Some(h) = open(&path, 0) else { continue };
                let mut number: STORAGE_DEVICE_NUMBER = std::mem::zeroed();
                let mut returned = 0u32;
                let ok = DeviceIoControl(
                    h.0,
                    IOCTL_STORAGE_GET_DEVICE_NUMBER,
                    null(),
                    0,
                    (&mut number as *mut STORAGE_DEVICE_NUMBER).cast(),
                    std::mem::size_of::<STORAGE_DEVICE_NUMBER>() as u32,
                    &mut returned,
                    null_mut(),
                );
                if ok != 0 && number.DeviceNumber == index {
                    found = Some(info.DevInst);
                    break;
                }
            }
            SetupDiDestroyDeviceInfoList(set);
            found
        }
    }

    fn device_id(devinst: u32) -> Option<String> {
        let mut buf = vec![0u16; 512];
        // SAFETY : tampon de la taille annoncée.
        let r = unsafe { CM_Get_Device_IDW(devinst, buf.as_mut_ptr(), buf.len() as u32, 0) };
        if r != CR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(
            OsString::from_wide(&buf[..len])
                .to_string_lossy()
                .into_owned(),
        )
    }

    fn parent(devinst: u32) -> Option<u32> {
        let mut p = 0u32;
        // SAFETY : sortie dans une variable locale.
        (unsafe { CM_Get_Parent(&mut p, devinst, 0) } == CR_SUCCESS).then_some(p)
    }

    fn registry_string(devinst: u32, property: u32) -> Option<String> {
        let mut buf = vec![0u16; 256];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY : tampon et longueur cohérents.
        let r = unsafe {
            CM_Get_DevNode_Registry_PropertyW(
                devinst,
                property,
                null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
                0,
            )
        };
        if r != CR_SUCCESS {
            return None;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(
            OsString::from_wide(&buf[..n])
                .to_string_lossy()
                .into_owned(),
        )
    }

    fn registry_u32(devinst: u32, property: u32) -> Option<u32> {
        let mut v = 0u32;
        let mut len = 4u32;
        // SAFETY : sortie de 4 octets dans une variable locale.
        let r = unsafe {
            CM_Get_DevNode_Registry_PropertyW(
                devinst,
                property,
                null_mut(),
                (&mut v as *mut u32).cast(),
                &mut len,
                0,
            )
        };
        (r == CR_SUCCESS).then_some(v)
    }

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
        let mut node = disk_devinst(index)?;
        let usb = loop {
            node = parent(node)?;
            let id = device_id(node)?;
            if id.to_ascii_uppercase().starts_with("USB\\") {
                break node;
            }
        };
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
