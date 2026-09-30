//! Windows : lancement de PhotoRec avec une console invisible, disque physique d'un volume,
//! liste des lecteurs.

use std::ffi::OsStr;
use std::io;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ELEVATION_REQUIRED, HANDLE, INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0,
};
use windows_sys::Win32::Storage::FileSystem::{
    BusTypeUsb, CreateFileW, GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives,
    GetVolumeInformationW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW, FILE_SHARE_READ,
    FILE_SHARE_WRITE, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, OPEN_EXISTING,
};
use windows_sys::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, DISK_EXTENT, IOCTL_STORAGE_GET_DEVICE_NUMBER,
    IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_DEVICE_NUMBER,
    STORAGE_PROPERTY_QUERY, VOLUME_DISK_EXTENTS,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessW, GetExitCodeProcess, TerminateProcess, WaitForSingleObject, CREATE_NO_WINDOW,
    PROCESS_INFORMATION, STARTF_USESHOWWINDOW, STARTUPINFOW,
};
use windows_sys::Win32::System::WindowsProgramming::{DRIVE_FIXED, DRIVE_REMOTE, DRIVE_REMOVABLE};
use windows_sys::Win32::System::IO::DeviceIoControl;

use super::cmdline::windows_command_line;
use crate::disk::{DestinationLocation, DiskId};
use crate::error::RecoveryError;
use crate::volumes::Volume;

/// `SW_HIDE` (winuser.h), défini ici pour ne pas tirer tout le module d'interface de windows-sys.
const SW_HIDE: u16 = 0;
/// `FILE_READ_ONLY_VOLUME` (winnt.h) dans les drapeaux de `GetVolumeInformationW`.
const FILE_READ_ONLY_VOLUME: u32 = 0x0008_0000;
/// Code de sortie donné à PhotoRec quand on l'arrête.
const TERMINATED_EXIT_CODE: u32 = 1;

// ---------- Processus ----------

/// PhotoRec lancé par `CreateProcessW`.
///
/// Pourquoi pas `std::process::Command` : PhotoRec démarre toujours son interface ncurses
/// (PDCurses sous Windows), même en mode `/cmd` (phmain.c appelle `start_ncurses` sans
/// condition ; intrfn.c : `initscr()` sous MinGW). PDCurses exige de vraies poignées de console
/// sur l'entrée et la sortie standard et refuse la redirection. Or `Command` passe toujours
/// `STARTF_USESTDHANDLES` avec les poignées du parent, nulles dans une application graphique
/// (Tauri). Ici, sans `STARTF_USESTDHANDLES`, Windows branche l'entrée et la sortie de l'enfant
/// sur sa propre console, créée sans fenêtre par `CREATE_NO_WINDOW` (même principe que
/// `pccheck_core::process::hide_console`). `SW_HIDE` en plus, par sécurité.
/// À valider avec le vrai photorec_win.exe ; repli si besoin : `CREATE_NEW_CONSOLE` + `SW_HIDE`.
pub(crate) struct ChildProcess {
    process: HANDLE,
}

// SAFETY : une poignée de processus Windows est utilisable depuis n'importe quel fil ; elle
// n'est fermée qu'une fois, dans `Drop`.
unsafe impl Send for ChildProcess {}

impl ChildProcess {
    pub(crate) fn spawn(
        program: &Path,
        args: &[String],
        cwd: &Path,
    ) -> Result<Self, RecoveryError> {
        let spawn_err = |reason: String| RecoveryError::Spawn {
            path: program.to_path_buf(),
            reason,
        };
        let app = wide(program.as_os_str());
        // CreateProcessW peut modifier ce tampon : il doit être mutable et nous appartenir.
        let mut line = wide(OsStr::new(&windows_command_line(
            &program.to_string_lossy(),
            args,
        )));
        let dir = wide(cwd.as_os_str());
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            dwFlags: STARTF_USESHOWWINDOW,
            wShowWindow: SW_HIDE,
            ..Default::default()
        };
        let mut info = PROCESS_INFORMATION::default();
        // SAFETY : chaînes terminées par un zéro qui vivent pendant l'appel ; structures
        // initialisées ; pas d'héritage de poignées (FALSE).
        let ok = unsafe {
            CreateProcessW(
                app.as_ptr(),
                line.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_NO_WINDOW,
                null(),
                dir.as_ptr(),
                &startup,
                &mut info,
            )
        };
        if ok == 0 {
            let err = io::Error::last_os_error();
            let reason = if err.raw_os_error() == Some(ERROR_ELEVATION_REQUIRED as i32) {
                "PhotoRec demande les droits administrateur : relance PCCheck en administrateur"
                    .to_string()
            } else {
                err.to_string()
            };
            return Err(spawn_err(reason));
        }
        // SAFETY : poignée de fil reçue de CreateProcessW, inutile ici.
        unsafe { CloseHandle(info.hThread) };
        Ok(ChildProcess {
            process: info.hProcess,
        })
    }

    /// Code de sortie si le processus est terminé, sans attendre.
    pub(crate) fn try_wait(&mut self) -> io::Result<Option<i32>> {
        // SAFETY : poignée valide jusqu'au Drop.
        match unsafe { WaitForSingleObject(self.process, 0) } {
            WAIT_OBJECT_0 => {
                let mut code = 0u32;
                // SAFETY : idem ; `code` est un u32 valide.
                if unsafe { GetExitCodeProcess(self.process, &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                // Les codes Windows sont des u32 ; réinterprétés en i32 comme `ExitStatus::code`.
                Ok(Some(code as i32))
            }
            WAIT_FAILED => Err(io::Error::last_os_error()),
            _ => Ok(None),
        }
    }

    /// Arrêt demandé par l'utilisateur. PhotoRec n'a pas d'arrêt propre accessible depuis une
    /// console sans fenêtre : on termine le processus. Les fichiers déjà écrits restent ;
    /// le dernier peut être tronqué.
    pub(crate) fn request_stop(&mut self) {
        self.kill();
    }

    pub(crate) fn kill(&mut self) {
        // SAFETY : poignée valide. Échec ignoré : le processus peut être déjà terminé.
        unsafe { TerminateProcess(self.process, TERMINATED_EXIT_CODE) };
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        // SAFETY : poignée reçue de CreateProcessW, fermée une seule fois.
        unsafe { CloseHandle(self.process) };
    }
}

// ---------- Disque d'un chemin ----------

/// Disque(s) physique(s) portant un chemin existant.
pub(crate) fn locate_path(path: &Path) -> DestinationLocation {
    let text = path.to_string_lossy();
    // `\\serveur\partage` ou `\\?\UNC\serveur\partage` : réseau.
    let is_unc = text.starts_with(r"\\?\UNC\")
        || (text.starts_with(r"\\") && !text.starts_with(r"\\?\") && !text.starts_with(r"\\.\"));
    if is_unc {
        return DestinationLocation::Remote;
    }
    let Some(root) = volume_path_name(path) else {
        return unknown("racine du volume introuvable", &io::Error::last_os_error());
    };
    if drive_type(&root) == DRIVE_REMOTE {
        return DestinationLocation::Remote;
    }
    let device = volume_device(&root);
    match volume_disks(&device) {
        Ok(disks) if !disks.is_empty() => DestinationLocation::Disks { disks },
        Ok(_) => DestinationLocation::Unknown {
            reason: format!("aucun disque pour {device}"),
        },
        Err(e) => unknown(&format!("disque de {device} illisible"), &e),
    }
}

fn unknown(what: &str, e: &io::Error) -> DestinationLocation {
    DestinationLocation::Unknown {
        reason: format!("{what} : {e}"),
    }
}

/// Racine du volume qui contient `path` (`C:\`, ou `C:\montage\` pour un volume monté dans un
/// dossier).
fn volume_path_name(path: &Path) -> Option<Vec<u16>> {
    let p = wide(path.as_os_str());
    let mut buf = vec![0u16; 1024];
    // SAFETY : `p` terminé par zéro ; `buf` de la taille annoncée.
    let ok = unsafe { GetVolumePathNameW(p.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) };
    (ok != 0).then(|| trim_nul(buf))
}

fn drive_type(root: &[u16]) -> u32 {
    let r = with_nul(root);
    // SAFETY : chaîne terminée par zéro.
    unsafe { GetDriveTypeW(r.as_ptr()) }
}

/// Chemin de périphérique du volume, sans barre finale (sinon on ouvrirait sa racine) :
/// `\\?\Volume{GUID}` si possible, sinon `\\.\X:` pour une racine `X:\`.
fn volume_device(root: &[u16]) -> String {
    let r = with_nul(root);
    let mut buf = vec![0u16; 64];
    // SAFETY : `r` terminé par zéro ; `buf` de la taille annoncée (50 caractères suffisent).
    let ok = unsafe {
        GetVolumeNameForVolumeMountPointW(r.as_ptr(), buf.as_mut_ptr(), buf.len() as u32)
    };
    let name = if ok != 0 {
        String::from_utf16_lossy(&trim_nul(buf))
    } else {
        let root = String::from_utf16_lossy(root);
        format!(r"\\.\{}", root.trim_end_matches('\\'))
    };
    name.trim_end_matches('\\').to_string()
}

/// Disques d'un volume : `IOCTL_STORAGE_GET_DEVICE_NUMBER` (cas courant, un seul disque),
/// sinon `IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS` (volume dynamique ou Espaces de stockage
/// sur plusieurs disques).
fn volume_disks(device: &str) -> io::Result<Vec<DiskId>> {
    let handle = Handle::open_query(device)?;
    if let Ok(n) = device_number(&handle) {
        return Ok(vec![DiskId::PhysicalDrive(n)]);
    }
    disk_extents(&handle)
}

fn device_number(handle: &Handle) -> io::Result<u32> {
    let mut out = STORAGE_DEVICE_NUMBER::default();
    handle.ioctl(
        IOCTL_STORAGE_GET_DEVICE_NUMBER,
        &[],
        (&mut out as *mut STORAGE_DEVICE_NUMBER).cast(),
        size_of::<STORAGE_DEVICE_NUMBER>(),
    )?;
    Ok(out.DeviceNumber)
}

fn disk_extents(handle: &Handle) -> io::Result<Vec<DiskId>> {
    // Place pour 32 extents : largement assez pour un volume réparti.
    const MAX: usize = 32;
    let size = size_of::<VOLUME_DISK_EXTENTS>() + (MAX - 1) * size_of::<DISK_EXTENT>();
    // Tampon en u64 pour l'alignement des champs i64 de DISK_EXTENT.
    let mut buf = vec![0u64; size.div_ceil(8)];
    handle.ioctl(
        IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
        &[],
        buf.as_mut_ptr().cast(),
        buf.len() * 8,
    )?;
    let base = buf.as_ptr().cast::<u8>();
    // SAFETY : le tampon commence par un VOLUME_DISK_EXTENTS rempli par le pilote.
    let count = unsafe { (*base.cast::<VOLUME_DISK_EXTENTS>()).NumberOfDiskExtents } as usize;
    let first = std::mem::offset_of!(VOLUME_DISK_EXTENTS, Extents);
    let mut disks: Vec<DiskId> = Vec::new();
    for i in 0..count.min(MAX) {
        // SAFETY : `i < MAX`, donc l'extent est dans le tampon ; lecture non alignée par prudence.
        let extent: DISK_EXTENT = unsafe {
            std::ptr::read_unaligned(
                base.add(first + i * size_of::<DISK_EXTENT>())
                    .cast::<DISK_EXTENT>(),
            )
        };
        let id = DiskId::PhysicalDrive(extent.DiskNumber);
        if !disks.contains(&id) {
            disks.push(id);
        }
    }
    Ok(disks)
}

/// USB ou support amovible, d'après le descripteur de stockage.
fn is_usb_or_removable(handle: &Handle) -> bool {
    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    // SAFETY : on ne lit que les octets de la structure passée en entrée.
    let input = unsafe {
        std::slice::from_raw_parts(
            (&query as *const STORAGE_PROPERTY_QUERY).cast::<u8>(),
            size_of::<STORAGE_PROPERTY_QUERY>(),
        )
    };
    let mut buf = vec![0u64; 128];
    if handle
        .ioctl(
            IOCTL_STORAGE_QUERY_PROPERTY,
            input,
            buf.as_mut_ptr().cast(),
            buf.len() * 8,
        )
        .is_err()
    {
        return false;
    }
    // Lecture champ par champ : `RemovableMedia` est un `bool` Rust, et relire tout le
    // descripteur d'un coup serait un comportement indéfini si le pilote y mettait autre
    // chose que 0 ou 1 (c'est un BOOLEAN C, un octet quelconque).
    let base = buf.as_ptr().cast::<u8>();
    // SAFETY : les deux décalages sont dans le tampon de 1 Kio rempli par le pilote.
    let (removable, bus) = unsafe {
        (
            *base.add(std::mem::offset_of!(
                STORAGE_DEVICE_DESCRIPTOR,
                RemovableMedia
            )) != 0,
            std::ptr::read_unaligned(
                base.add(std::mem::offset_of!(STORAGE_DEVICE_DESCRIPTOR, BusType))
                    .cast::<i32>(),
            ),
        )
    };
    bus == BusTypeUsb || removable
}

/// Poignée de périphérique ouverte sans droit de lecture ni d'écriture : suffisant pour les
/// requêtes d'information, et aucune écriture possible par construction.
struct Handle(HANDLE);

impl Handle {
    fn open_query(device: &str) -> io::Result<Self> {
        let name = wide(OsStr::new(device));
        // SAFETY : nom terminé par zéro ; accès 0 = requêtes seulement.
        let h = unsafe {
            CreateFileW(
                name.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                null(),
                OPEN_EXISTING,
                0,
                null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Handle(h))
    }

    fn ioctl(
        &self,
        code: u32,
        input: &[u8],
        out: *mut core::ffi::c_void,
        out_len: usize,
    ) -> io::Result<()> {
        let mut returned = 0u32;
        // SAFETY : `out` pointe vers `out_len` octets inscriptibles fournis par l'appelant ;
        // `input` est une tranche valide ; appel synchrone (pas d'OVERLAPPED).
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                code,
                if input.is_empty() {
                    null()
                } else {
                    input.as_ptr().cast()
                },
                input.len() as u32,
                out,
                out_len as u32,
                &mut returned,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte par CreateFileW, fermée une seule fois.
        unsafe { CloseHandle(self.0) };
    }
}

// ---------- Volumes ----------

/// Lecteurs locaux (fixes ou amovibles) avec un support inscriptible. Les lecteurs réseau sont
/// omis : `GetDiskFreeSpaceExW` peut bloquer longtemps sur un partage déconnecté.
pub(crate) fn list_volumes() -> Vec<Volume> {
    // SAFETY : aucun argument.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| volume_info(char::from(b'A' + i)))
        .collect()
}

fn volume_info(letter: char) -> Option<Volume> {
    let root_str = format!(r"{letter}:\");
    let root: Vec<u16> = root_str.encode_utf16().collect();
    let dtype = drive_type(&root);
    if dtype != DRIVE_FIXED && dtype != DRIVE_REMOVABLE {
        return None;
    }
    let r = with_nul(&root);
    let mut label = vec![0u16; 261];
    let mut fs = vec![0u16; 261];
    let mut flags = 0u32;
    // SAFETY : tampons de la taille annoncée ; échec = pas de support (lecteur de carte vide).
    let ok = unsafe {
        GetVolumeInformationW(
            r.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            null_mut(),
            null_mut(),
            &mut flags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    if ok == 0 || flags & FILE_READ_ONLY_VOLUME != 0 {
        return None;
    }
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY : pointeurs vers des u64 valides ; le dernier est facultatif.
    if unsafe { GetDiskFreeSpaceExW(r.as_ptr(), &mut free, &mut total, null_mut()) } == 0 {
        return None;
    }
    let device = format!(r"\\.\{letter}:");
    let handle = Handle::open_query(&device).ok();
    let disk = handle
        .as_ref()
        .and_then(|h| device_number(h).ok())
        .map(DiskId::PhysicalDrive);
    let removable = dtype == DRIVE_REMOVABLE || handle.as_ref().is_some_and(is_usb_or_removable);
    Some(Volume {
        path: PathBuf::from(root_str),
        label: String::from_utf16_lossy(&trim_nul(label)),
        filesystem: String::from_utf16_lossy(&trim_nul(fs)),
        total_bytes: total,
        free_bytes: free,
        disk,
        removable,
    })
}

// ---------- Chaînes larges ----------

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

fn with_nul(s: &[u16]) -> Vec<u16> {
    s.iter().copied().chain(Some(0)).collect()
}

/// Coupe au premier zéro.
fn trim_nul(mut buf: Vec<u16>) -> Vec<u16> {
    if let Some(end) = buf.iter().position(|&c| c == 0) {
        buf.truncate(end);
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn system32(exe: &str) -> PathBuf {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join(exe)
    }

    fn wait(child: &mut ChildProcess, max: Duration) -> Option<i32> {
        let start = Instant::now();
        while start.elapsed() < max {
            if let Some(code) = child.try_wait().unwrap() {
                return Some(code);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    #[test]
    fn child_exit_code_and_working_directory() {
        let dir = std::env::temp_dir().join(format!("pccheck-recovery-win-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let args: Vec<String> = ["/c", "echo ok> marque.txt & exit 3"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut child = ChildProcess::spawn(&system32("cmd.exe"), &args, &dir).unwrap();
        assert_eq!(wait(&mut child, Duration::from_secs(10)), Some(3));
        assert!(
            dir.join("marque.txt").is_file(),
            "dossier courant non appliqué"
        );
    }

    #[test]
    fn child_has_a_real_console_without_window() {
        // Sonde : entrée et sortie standard non redirigées (1 et 2) et console mesurable (4).
        // C'est ce que PDCurses exige. Mesuré : `Command` + CREATE_NO_WINDOW donne 3
        // (redirigées, pas de console), ce `ChildProcess` donne 4.
        let probe = "$w = try { [Console]::WindowWidth } catch { -1 }; \
                     exit ([int][Console]::IsInputRedirected + 2*[int][Console]::IsOutputRedirected \
                     + 4*[int]($w -gt 0))";
        let args: Vec<String> = ["-NoProfile", "-NonInteractive", "-Command", probe]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let ps = system32("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let mut child = ChildProcess::spawn(&ps, &args, &std::env::temp_dir()).unwrap();
        assert_eq!(wait(&mut child, Duration::from_secs(30)), Some(4));
    }

    #[test]
    fn long_child_is_killed() {
        let args: Vec<String> = ["/c", "ping -n 30 127.0.0.1 > nul"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut child =
            ChildProcess::spawn(&system32("cmd.exe"), &args, &std::env::temp_dir()).unwrap();
        assert_eq!(child.try_wait().unwrap(), None);
        child.request_stop();
        assert_eq!(
            wait(&mut child, Duration::from_secs(5)),
            Some(TERMINATED_EXIT_CODE as i32)
        );
    }

    #[test]
    fn missing_program_is_a_spawn_error() {
        let r = ChildProcess::spawn(
            Path::new(r"C:\inexistant\photorec_win.exe"),
            &[],
            &std::env::temp_dir(),
        );
        assert!(matches!(r, Err(RecoveryError::Spawn { .. })));
    }

    #[test]
    fn temp_dir_is_on_a_physical_drive() {
        match locate_path(&std::env::temp_dir()) {
            DestinationLocation::Disks { disks } => assert!(!disks.is_empty()),
            other => panic!("disque du dossier temporaire introuvable : {other:?}"),
        }
    }

    #[test]
    fn unc_paths_are_remote() {
        assert_eq!(
            locate_path(Path::new(r"\\nas\partage\recup")),
            DestinationLocation::Remote
        );
        assert_eq!(
            locate_path(Path::new(r"\\?\UNC\nas\partage")),
            DestinationLocation::Remote
        );
    }

    #[test]
    fn system_drive_is_listed() {
        let volumes = list_volumes();
        let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let c = volumes
            .iter()
            .find(|v| v.path.to_string_lossy().starts_with(&system))
            .expect("lecteur système absent de la liste");
        assert!(c.total_bytes > 0 && c.free_bytes <= c.total_bytes);
        assert!(c.disk.is_some());
    }
}
