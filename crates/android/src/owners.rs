//! Gestion d'entreprise : propriétaire d'appareil (Device Owner) et propriétaires de profil.
//!
//! Source principale : `adb shell dpm list-owners` (Android 12 et plus), qui affiche
//! `no owners` ou
//! ```text
//! 1 owner:
//! User  0: admin=com.example.mdm/.AdminReceiver,DeviceOwner
//! ```
//! Repli pour les versions plus anciennes : `dumpsys device_policy`, dont les sections
//! `Device Owner:` et `Profile Owner (User N):` contiennent une ligne `package=...`.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct DeviceOwners {
    /// Paquet de l'application de gestion propriétaire de l'appareil (gestion d'entreprise
    /// complète : le téléphone appartient à une organisation).
    pub device_owner: Option<String>,
    pub profile_owners: Vec<ProfileOwner>,
}

impl DeviceOwners {
    pub fn is_empty(&self) -> bool {
        self.device_owner.is_none() && self.profile_owners.is_empty()
    }
}

/// Propriétaire d'un profil (souvent un profil professionnel séparé).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileOwner {
    pub user_id: u32,
    pub package: String,
    /// Profil géré (profil de travail) rattaché à un autre utilisateur.
    pub managed_profile: bool,
}

/// Analyse `dpm list-owners`. `None` si la commande n'est pas reconnue (Android 11 et moins)
/// ou si la sortie est inattendue.
pub fn parse_dpm_list_owners(text: &str) -> Option<DeviceOwners> {
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next()?;
    if first.starts_with("no owners") {
        return Some(DeviceOwners::default());
    }
    // En-tête « N owner: » ou « N owners: ».
    let (count, word) = first.trim_end_matches(':').split_once(' ')?;
    if count.parse::<u32>().is_err() || !word.starts_with("owner") {
        return None;
    }
    let mut owners = DeviceOwners::default();
    for line in lines {
        let Some(rest) = line.strip_prefix("User") else {
            continue;
        };
        let Some((user, detail)) = rest.split_once(':') else {
            continue;
        };
        let Ok(user_id) = user.trim().parse::<u32>() else {
            continue;
        };
        let Some(admin) = detail.trim().strip_prefix("admin=") else {
            continue;
        };
        let mut parts = admin.split(',');
        let package = package_of(parts.next().unwrap_or_default());
        let flags: Vec<&str> = parts.collect();
        if flags.contains(&"DeviceOwner") {
            owners.device_owner = Some(package.clone());
        }
        let managed = flags.iter().any(|f| f.starts_with("ManagedProfileOwner"));
        if managed || flags.contains(&"ProfileOwner") {
            owners.profile_owners.push(ProfileOwner {
                user_id,
                package,
                managed_profile: managed,
            });
        }
    }
    Some(owners)
}

/// Analyse `dumpsys device_policy` (repli pour Android 11 et moins). `None` si la sortie ne
/// ressemble pas à un état du gestionnaire de règles.
pub fn parse_dumpsys_device_policy(text: &str) -> Option<DeviceOwners> {
    if !text.contains("Device Policy Manager") {
        return None;
    }
    let mut owners = DeviceOwners::default();
    // Section en cours : `None` hors section, `Some(None)` = Device Owner, `Some(Some(u))` = profil.
    let mut section: Option<Option<u32>> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Device Owner") {
            section = Some(None);
        } else if let Some(rest) = trimmed.strip_prefix("Profile Owner (User ") {
            section = rest
                .split(')')
                .next()
                .and_then(|u| u.trim().parse::<u32>().ok())
                .map(Some);
        } else if trimmed.is_empty() {
            section = None;
        } else if let (Some(owner), Some(pkg)) = (section, trimmed.strip_prefix("package=")) {
            match owner {
                None => owners.device_owner = Some(pkg.to_string()),
                Some(user_id) => owners.profile_owners.push(ProfileOwner {
                    user_id,
                    package: pkg.to_string(),
                    // Sans autre indice, un profil hors utilisateur principal est un profil géré.
                    managed_profile: user_id != 0,
                }),
            }
            section = None;
        }
    }
    Some(owners)
}

/// `com.example.mdm/.AdminReceiver` ou `ComponentInfo{com.example.mdm/...}` → `com.example.mdm`.
fn package_of(component: &str) -> String {
    let c = component.trim();
    let c = c.strip_prefix("ComponentInfo{").unwrap_or(c);
    c.split('/').next().unwrap_or(c).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_extraction() {
        assert_eq!(
            package_of("com.example.mdm/.AdminReceiver"),
            "com.example.mdm"
        );
        assert_eq!(
            package_of("ComponentInfo{com.example.mdm/com.example.mdm.R}"),
            "com.example.mdm"
        );
    }

    #[test]
    fn unexpected_output_is_unknown() {
        assert_eq!(parse_dpm_list_owners(""), None);
        assert_eq!(parse_dpm_list_owners("Error: unknown command\n"), None);
        assert_eq!(parse_dumpsys_device_policy("Permission Denial"), None);
    }
}
