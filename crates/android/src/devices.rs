//! Appareils vus par adb (`adb devices -l`) et messages qui guident l'utilisateur selon l'état.
//!
//! Format d'une ligne : `<série> <état> [clé:valeur]...`, par exemple
//! `R58N00000XX  device usb:1-1 product:beyond1ltevl model:SM_G973W device:beyond1 transport_id:1`.
//! L'état peut contenir des espaces (`no permissions (...); see [http://...]`) : il s'arrête au
//! premier jeton `clé:valeur` dont la clé est connue.

use std::fmt;

use serde::Serialize;

/// État d'un appareil tel qu'adb le rapporte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    /// Prêt : débogage USB autorisé pour cet ordinateur.
    Device,
    /// La clé RSA de l'ordinateur n'a pas encore été acceptée sur le téléphone.
    Unauthorized,
    /// Connexion établie mais le téléphone ne répond pas.
    Offline,
    /// Linux : l'utilisateur n'a pas le droit d'ouvrir le périphérique USB (règles udev).
    NoPermissions,
    /// Autres états d'adb : `recovery`, `sideload`, `bootloader`, `authorizing`, `connecting`...
    Other(String),
}

impl DeviceState {
    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "device" => DeviceState::Device,
            "unauthorized" => DeviceState::Unauthorized,
            "offline" => DeviceState::Offline,
            t if t.starts_with("no permissions") => DeviceState::NoPermissions,
            other => DeviceState::Other(other.to_string()),
        }
    }

    /// Consigne à afficher tant que le téléphone n'est pas prêt. `None` pour `Device`.
    pub fn guidance(&self) -> Option<String> {
        let text = match self {
            DeviceState::Device => return None,
            DeviceState::Unauthorized => {
                "Déverrouille le téléphone et accepte la clé RSA de cet ordinateur dans la \
                 fenêtre « Autoriser le débogage USB ? ». \
                 Sur le téléphone d'un vendeur, ne coche pas « Toujours autoriser ». \
                 Si la fenêtre n'apparaît pas, débranche puis rebranche le câble."
                    .to_string()
            }
            DeviceState::Offline => {
                "Le téléphone ne répond pas. Déverrouille l'écran, débranche puis rebranche le câble. \
                 Si ça persiste : Options pour les développeurs > Révoquer les autorisations de \
                 débogage USB, puis rebranche et accepte de nouveau."
                    .to_string()
            }
            DeviceState::NoPermissions => {
                "Linux refuse l'accès au téléphone : il manque les règles udev d'Android \
                 (paquet « android-sdk-platform-tools-common » ou « android-udev-rules »). \
                 Installe-les, ajoute ton utilisateur au groupe plugdev, puis rebranche le téléphone."
                    .to_string()
            }
            DeviceState::Other(state) => match state.as_str() {
                "authorizing" | "connecting" => {
                    "Connexion en cours : attends quelques secondes puis relance la détection."
                        .to_string()
                }
                other => format!(
                    "Le téléphone est en mode « {other} » : redémarre-le normalement, puis rebranche-le."
                ),
            },
        };
        Some(text)
    }
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceState::Device => f.write_str("device"),
            DeviceState::Unauthorized => f.write_str("unauthorized"),
            DeviceState::Offline => f.write_str("offline"),
            DeviceState::NoPermissions => f.write_str("no permissions"),
            DeviceState::Other(s) => f.write_str(s),
        }
    }
}

/// Un appareil listé par `adb devices -l`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdbDevice {
    /// Numéro de série brut. Sert seulement à adresser l'appareil (`adb -s`) : ne pas l'afficher
    /// ni l'enregistrer dans un rapport, utiliser `serial_masked`.
    pub serial: String,
    /// Numéro de série masqué, pour l'affichage (voir `mask_serial`).
    pub serial_masked: String,
    pub state: DeviceState,
    /// Modèle vu par adb : espaces et tirets remplacés par `_` (`SM_G973W`, `Pixel_7`).
    pub model: Option<String>,
    pub product: Option<String>,
    /// Nom de code de l'appareil (`beyond1`, `panther`).
    pub device: Option<String>,
    /// Identifiant de transport (adb 1.0.40 et plus).
    pub transport_id: Option<u32>,
    /// Consigne pour l'utilisateur quand l'appareil n'est pas prêt.
    pub guidance: Option<String>,
}

/// Clés des propriétés affichées par `adb devices -l`.
const PROPERTY_KEYS: [&str; 5] = ["usb", "product", "model", "device", "transport_id"];

/// Analyse la sortie de `adb devices -l` (ou `adb devices`). Les lignes de démarrage du serveur
/// (`* daemon started successfully`) et l'en-tête sont ignorées.
pub fn parse_devices(text: &str) -> Vec<AdbDevice> {
    text.lines().filter_map(parse_device_line).collect()
}

fn parse_device_line(line: &str) -> Option<AdbDevice> {
    let line = line.trim();
    if line.is_empty()
        || line.starts_with('*')
        || line.starts_with("List of devices")
        || line.starts_with("adb server")
    {
        return None;
    }
    let mut tokens = line.split_whitespace();
    let serial = tokens.next()?;
    let mut state_words: Vec<&str> = Vec::new();
    let mut device = AdbDevice {
        serial: serial.to_string(),
        serial_masked: mask_serial(serial),
        state: DeviceState::Device,
        model: None,
        product: None,
        device: None,
        transport_id: None,
        guidance: None,
    };
    let mut in_properties = false;
    for token in tokens {
        let property = token
            .split_once(':')
            .filter(|(key, _)| PROPERTY_KEYS.contains(key));
        match property {
            Some((key, value)) => {
                in_properties = true;
                let value = Some(value.to_string()).filter(|v| !v.is_empty());
                match key {
                    "product" => device.product = value,
                    "model" => device.model = value,
                    "device" => device.device = value,
                    "transport_id" => {
                        device.transport_id = value.and_then(|v| v.parse().ok());
                    }
                    _ => {} // `usb:` : port physique, sans intérêt ici.
                }
            }
            None if !in_properties => state_words.push(token),
            None => {}
        }
    }
    if state_words.is_empty() {
        return None; // Ligne parasite sans état : pas un appareil.
    }
    device.state = DeviceState::parse(&state_words.join(" "));
    device.guidance = device.state.guidance();
    Some(device)
}

/// Consigne quand aucun téléphone n'apparaît. Sous Windows, un téléphone absent de la liste
/// signifie souvent que le pilote USB du fabricant manque.
pub fn no_device_guidance() -> String {
    no_device_guidance_for(cfg!(windows))
}

/// Version testable de `no_device_guidance`.
pub fn no_device_guidance_for(windows: bool) -> String {
    let mut text = String::from(
        "Aucun téléphone détecté. Sur le téléphone : Paramètres > À propos du téléphone, \
         touche 7 fois « Numéro de build » pour activer les Options pour les développeurs, \
         puis active « Débogage USB ». Utilise un câble de données : certains câbles ne font que charger.",
    );
    if windows {
        text.push_str(
            " Sous Windows, si le téléphone n'apparaît toujours pas, il manque probablement le pilote \
             USB du fabricant (Samsung USB Driver, Google USB Driver...) : installe-le puis rebranche.",
        );
    }
    text
}

/// Masque un numéro de série pour l'affichage : garde les 2 premiers et les 2 derniers caractères
/// (1 et 1 pour une série courte), remplace le reste par `*`. Assez pour distinguer deux
/// téléphones branchés, pas assez pour identifier l'appareil.
pub fn mask_serial(serial: &str) -> String {
    let chars: Vec<char> = serial.chars().collect();
    let n = chars.len();
    let keep = match n {
        0..=4 => 0,
        5..=7 => 1,
        _ => 2,
    };
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| if i < keep || i >= n - keep { *c } else { '*' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_serials() {
        assert_eq!(mask_serial("R58N00000XX"), "R5*******XX");
        assert_eq!(mask_serial("ABC123"), "A****3");
        assert_eq!(mask_serial("ABCD"), "****");
        assert_eq!(mask_serial(""), "");
        assert_eq!(mask_serial("192.168.1.50:5555"), "19*************55");
    }

    #[test]
    fn no_device_guidance_mentions_driver_only_on_windows() {
        assert!(no_device_guidance_for(true).contains("pilote"));
        assert!(!no_device_guidance_for(false).contains("pilote"));
        assert!(no_device_guidance_for(false).contains("Débogage USB"));
    }

    #[test]
    fn state_parsing_and_guidance() {
        assert_eq!(DeviceState::parse("device"), DeviceState::Device);
        assert!(DeviceState::Device.guidance().is_none());
        assert!(DeviceState::Unauthorized
            .guidance()
            .unwrap()
            .contains("Autoriser le débogage USB"));
        assert_eq!(
            DeviceState::parse("no permissions (user in plugdev group); see [http://x]"),
            DeviceState::NoPermissions
        );
        let recovery = DeviceState::parse("recovery");
        assert_eq!(recovery, DeviceState::Other("recovery".into()));
        assert!(recovery.guidance().unwrap().contains("« recovery »"));
    }
}
