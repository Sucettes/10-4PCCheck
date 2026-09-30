//! Entrées-sorties sans cache du système : lecture brute d'un disque (scan de surface) et
//! écriture/relecture de fichiers de test (capacité réelle).
//!
//! Sans cache, les lectures mesurent vraiment le disque : relire un fichier qu'on vient d'écrire
//! renverrait sinon la copie en mémoire, et une fausse clé USB passerait le test. En contrepartie,
//! Windows (FILE_FLAG_NO_BUFFERING) et Linux (O_DIRECT) exigent des tampons, des positions et des
//! longueurs alignés sur la taille de secteur : d'où `AlignedBuf` et `ALIGN`.

use std::alloc::{self, Layout};
use std::fs::{File, OpenOptions};
use std::io;
use std::ops::{Deref, DerefMut};
use std::path::Path;
use std::ptr::NonNull;

/// Alignement retenu : 4 Kio couvre les secteurs de 512 octets et de 4 Kio (disques « 4Kn »).
pub const ALIGN: usize = 4096;

/// Tampon d'octets aligné sur `ALIGN`, initialisé à zéro. `Vec<u8>` ne garantit qu'un alignement
/// de 1, d'où l'allocation manuelle.
pub struct AlignedBuf {
    ptr: NonNull<u8>,
    layout: Layout,
}

// SAFETY : le tampon possède sa mémoire en exclusivité (comme un `Box<[u8]>`) ; il peut changer de fil.
unsafe impl Send for AlignedBuf {}

impl AlignedBuf {
    /// `len` est arrondi au multiple de `ALIGN` supérieur. Panique seulement si l'allocation échoue,
    /// comme `Vec`.
    pub fn new(len: usize) -> Self {
        let len = len.max(1).div_ceil(ALIGN) * ALIGN;
        let layout = Layout::from_size_align(len, ALIGN).expect("taille de tampon valide");
        // SAFETY : `layout` a une taille non nulle.
        let raw = unsafe { alloc::alloc_zeroed(layout) };
        let ptr = NonNull::new(raw).unwrap_or_else(|| alloc::handle_alloc_error(layout));
        AlignedBuf { ptr, layout }
    }
}

impl Deref for AlignedBuf {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        // SAFETY : `ptr` pointe vers `layout.size()` octets initialisés, possédés par `self`.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size()) }
    }
}

impl DerefMut for AlignedBuf {
    fn deref_mut(&mut self) -> &mut [u8] {
        // SAFETY : idem, et `&mut self` garantit l'exclusivité.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.as_ptr(), self.layout.size()) }
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        // SAFETY : `ptr` vient de `alloc_zeroed` avec ce même `layout`.
        unsafe { alloc::dealloc(self.ptr.as_ptr(), self.layout) }
    }
}

#[cfg(windows)]
mod sys {
    pub const FILE_SHARE_READ: u32 = 0x1;
    pub const FILE_SHARE_WRITE: u32 = 0x2;
    pub const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
    pub const FILE_FLAG_WRITE_THROUGH: u32 = 0x8000_0000;
    pub const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x0800_0000;
}

/// Ouvre un disque entier en lecture seule, sans cache. Le disque reste utilisable par le système
/// (partage lecture et écriture). Droits administrateur requis.
pub fn open_device_read(path: &str) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.share_mode(sys::FILE_SHARE_READ | sys::FILE_SHARE_WRITE)
            .custom_flags(sys::FILE_FLAG_NO_BUFFERING | sys::FILE_FLAG_SEQUENTIAL_SCAN);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_DIRECT);
    }
    opts.open(path)
}

/// Crée un fichier de test pour l'écriture sans cache (données envoyées au disque, pas à la RAM).
pub fn create_uncached(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(sys::FILE_FLAG_NO_BUFFERING | sys::FILE_FLAG_WRITE_THROUGH);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_DIRECT);
    }
    match opts.open(path) {
        // Certains systèmes de fichiers refusent O_DIRECT : repli sur l'écriture normale, les données
        // seront écartées du cache après `sync_all` (voir `drop_cache`).
        #[cfg(target_os = "linux")]
        Err(e) if e.raw_os_error() == Some(libc::EINVAL) => OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path),
        other => other,
    }
}

/// Crée un fichier NEUF pour l'écriture sans cache : échoue si le chemin existe déjà, un
/// fichier présent n'est donc jamais remplacé ni tronqué.
pub fn create_new_uncached(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(sys::FILE_FLAG_NO_BUFFERING | sys::FILE_FLAG_WRITE_THROUGH);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_DIRECT);
    }
    match opts.open(path) {
        // O_DIRECT refusé par le système de fichiers : écriture normale, `sync_all` fera foi.
        #[cfg(target_os = "linux")]
        Err(e) if e.raw_os_error() == Some(libc::EINVAL) => {
            OpenOptions::new().write(true).create_new(true).open(path)
        }
        other => other,
    }
}

/// Ouvre un fichier de test pour la relecture sans cache.
pub fn open_uncached(path: &Path) -> io::Result<File> {
    let mut opts = OpenOptions::new();
    opts.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        opts.custom_flags(sys::FILE_FLAG_NO_BUFFERING | sys::FILE_FLAG_SEQUENTIAL_SCAN);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_DIRECT);
    }
    match opts.open(path) {
        #[cfg(target_os = "linux")]
        Err(e) if e.raw_os_error() == Some(libc::EINVAL) => {
            let f = File::open(path)?;
            drop_cache(&f);
            Ok(f)
        }
        other => other,
    }
}

/// Demande au noyau d'oublier les pages en cache d'un fichier (repli quand O_DIRECT est refusé).
#[cfg(target_os = "linux")]
pub fn drop_cache(file: &File) {
    use std::os::unix::io::AsRawFd;
    // SAFETY : descripteur valide pendant l'appel ; posix_fadvise n'a pas d'autre précondition.
    // Échec ignoré : ce n'est qu'un conseil au noyau.
    unsafe {
        libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
    }
}

/// Lecture à une position, sans déplacer de curseur partagé.
pub fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        file.seek_read(buf, offset)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        file.read_at(buf, offset)
    }
}

/// Lit exactement `buf.len()` octets à `offset` (plusieurs appels si le système en rend moins).
pub fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        match read_at(file, buf, offset) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                buf = &mut buf[n..];
                offset += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Chemin système pour lire un disque, depuis le nom donné par smartctl.
/// Windows : `/dev/sda` → `\\.\PhysicalDrive0`, `/dev/sdab` → 27, `/dev/pd3` → 3.
/// Linux : `/dev/sda` tel quel, `/dev/nvme0` → `/dev/nvme0n1` (espace de noms 1).
/// `None` pour un chemin qu'on ne sait pas lire directement (CSMI, contrôleurs RAID).
pub fn raw_device_path(smartctl_name: &str) -> Option<String> {
    #[cfg(windows)]
    {
        windows_raw_path(smartctl_name)
    }
    #[cfg(not(windows))]
    {
        linux_raw_path(smartctl_name)
    }
}

pub fn windows_raw_path(name: &str) -> Option<String> {
    let index = if let Some(n) = name.strip_prefix("/dev/pd") {
        n.parse::<u32>().ok()?
    } else {
        sd_letters_index(name.strip_prefix("/dev/sd")?)?
    };
    Some(format!(r"\\.\PhysicalDrive{index}"))
}

pub fn linux_raw_path(name: &str) -> Option<String> {
    if let Some(rest) = name.strip_prefix("/dev/nvme") {
        // /dev/nvme0 (contrôleur) → /dev/nvme0n1 ; /dev/nvme0n1 déjà un espace de noms. Rien
        // d'autre : le nom vient de l'interface et sera ouvert en root.
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        return match rest.split_once('n') {
            None if digits(rest) => Some(format!("{name}n1")),
            Some((ctrl, ns)) if digits(ctrl) && digits(ns) => Some(name.to_string()),
            _ => None,
        };
    }
    let letters = name.strip_prefix("/dev/sd")?;
    sd_letters_index(letters)?;
    Some(name.to_string())
}

/// `a` → 0, `z` → 25, `aa` → 26 : même convention que Linux et smartctl sous Windows.
fn sd_letters_index(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 2 || !letters.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    let mut index = 0u32;
    for b in letters.bytes() {
        index = index * 26 + u32::from(b - b'a') + 1;
    }
    Some(index - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_buffer_is_aligned_and_rounded() {
        let buf = AlignedBuf::new(5000);
        assert_eq!(buf.as_ptr() as usize % ALIGN, 0);
        assert_eq!(buf.len(), 8192);
        assert!(buf.iter().all(|&b| b == 0));
    }

    #[test]
    fn smartctl_names_map_to_raw_devices() {
        assert_eq!(
            windows_raw_path("/dev/sda").as_deref(),
            Some(r"\\.\PhysicalDrive0")
        );
        assert_eq!(
            windows_raw_path("/dev/sdd").as_deref(),
            Some(r"\\.\PhysicalDrive3")
        );
        assert_eq!(
            windows_raw_path("/dev/sdaa").as_deref(),
            Some(r"\\.\PhysicalDrive26")
        );
        assert_eq!(
            windows_raw_path("/dev/pd5").as_deref(),
            Some(r"\\.\PhysicalDrive5")
        );
        assert_eq!(windows_raw_path("/dev/csmi0,4"), None);
        assert_eq!(
            linux_raw_path("/dev/nvme0").as_deref(),
            Some("/dev/nvme0n1")
        );
        assert_eq!(
            linux_raw_path("/dev/nvme1n1").as_deref(),
            Some("/dev/nvme1n1")
        );
        assert_eq!(linux_raw_path("/dev/sdb").as_deref(), Some("/dev/sdb"));
        assert_eq!(linux_raw_path("/dev/sd1"), None);
        assert_eq!(linux_raw_path("/dev/nvme../../etc/shadow"), None);
        assert_eq!(linux_raw_path("/dev/nvme0n"), None);
    }
}
