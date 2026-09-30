//! Windows : arbre des périphériques (Configuration Manager et SetupAPI), partagé par la
//! liaison USB (`usb`) et la liaison PCIe (`pcie`) d'un disque. Requêtes d'information seulement,
//! sans droits administrateur.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::ptr::{null, null_mut};

use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_PropertyW, CM_Get_DevNode_Registry_PropertyW, CM_Get_Device_IDW, CM_Get_Parent,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
};
use windows_sys::Win32::Devices::Properties::DEVPROPKEY;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Ioctl::{IOCTL_STORAGE_GET_DEVICE_NUMBER, STORAGE_DEVICE_NUMBER};
use windows_sys::Win32::System::IO::DeviceIoControl;

const GUID_DEVINTERFACE_DISK: GUID = GUID::from_u128(0x53f56307_b6bf_11d0_94f2_00a0c91efb8b);
pub(crate) const CR_SUCCESS: u32 = 0;

/// Numéro de disque physique (`\\.\PhysicalDriveN`) d'un disque nommé par smartctl.
pub(crate) fn physical_drive_index(smartctl_name: &str) -> Option<u32> {
    crate::rawio::windows_raw_path(smartctl_name)?
        .strip_prefix(r"\\.\PhysicalDrive")?
        .parse()
        .ok()
}

/// Premier ancêtre du disque `index` dont l'identifiant commence par `prefix` (`USB\`, `PCI\`).
pub(crate) fn disk_ancestor(index: u32, prefix: &str) -> Option<u32> {
    let mut node = disk_devinst(index)?;
    loop {
        node = parent(node)?;
        if device_id(node)?.to_ascii_uppercase().starts_with(prefix) {
            return Some(node);
        }
    }
}

/// Propriété entière (UINT32) d'un nœud, par sa clé (`DEVPKEY_...`).
pub(crate) fn property_u32(devinst: u32, key: &DEVPROPKEY) -> Option<u32> {
    let mut kind = 0u32;
    let mut v = 0u32;
    let mut len = 4u32;
    // SAFETY : sortie de 4 octets dans une variable locale, taille annoncée.
    let r = unsafe {
        CM_Get_DevNode_PropertyW(
            devinst,
            key,
            &mut kind,
            (&mut v as *mut u32).cast(),
            &mut len,
            0,
        )
    };
    (r == CR_SUCCESS && len == 4).then_some(v)
}

pub(crate) struct Handle(pub(crate) HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte par CreateFileW, fermée une seule fois.
        unsafe { CloseHandle(self.0) };
    }
}

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub(crate) fn open(path: &[u16], access: u32) -> Option<Handle> {
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
pub(crate) fn disk_devinst(index: u32) -> Option<u32> {
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
            if SetupDiEnumDeviceInterfaces(set, null(), &GUID_DEVINTERFACE_DISK, i, &mut iface) == 0
            {
                break;
            }
            i += 1;
            let mut needed = 0u32;
            SetupDiGetDeviceInterfaceDetailW(set, &iface, null_mut(), 0, &mut needed, null_mut());
            if needed == 0 {
                continue;
            }
            // Tampon aligné sur 8 octets pour la structure de détail.
            let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
            let detail = buf.as_mut_ptr().cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
            (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            let mut info: SP_DEVINFO_DATA = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(set, &iface, detail, needed, null_mut(), &mut info)
                == 0
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

pub(crate) fn device_id(devinst: u32) -> Option<String> {
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

pub(crate) fn parent(devinst: u32) -> Option<u32> {
    let mut p = 0u32;
    // SAFETY : sortie dans une variable locale.
    (unsafe { CM_Get_Parent(&mut p, devinst, 0) } == CR_SUCCESS).then_some(p)
}

pub(crate) fn registry_string(devinst: u32, property: u32) -> Option<String> {
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

pub(crate) fn registry_u32(devinst: u32, property: u32) -> Option<u32> {
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
