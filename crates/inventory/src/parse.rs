//! Fonctions d'analyse pures, communes aux plateformes ou propres aux sorties Windows.
//! Aucune n'accède au matériel : elles sont testées sur toutes les plateformes.

use serde::Serialize;

use crate::model::{
    AutopilotInfo, BatteryInfo, DeviceJoin, LicenseInfo, NetworkKind, SecurityInfo,
};

/// Valeurs de remplissage que les fabricants laissent dans la table SMBIOS au lieu d'une vraie
/// donnée. Comparées sans tenir compte de la casse.
const SMBIOS_PLACEHOLDERS: [&str; 22] = [
    "to be filled by o.e.m.",
    "to be filled by oem",
    "default string",
    "system serial number",
    "system product name",
    "system manufacturer",
    "system version",
    "base board serial number",
    "chassis serial number",
    "type2 - board serial number",
    "not specified",
    "not applicable",
    "not available",
    "unknown",
    "none",
    "n/a",
    "na",
    "oem",
    "o.e.m.",
    "invalid",
    "0123456789",
    "123456789",
];

/// Nettoie une chaîne SMBIOS (WMI ou `/sys/class/dmi/id`) : espaces retirés, valeurs de
/// remplissage et suites d'un seul caractère répété (« 00000000 », « FFFF ») écartées.
pub fn clean_smbios(raw: &str) -> Option<String> {
    let s = raw.trim().trim_matches('\0').trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_lowercase();
    if SMBIOS_PLACEHOLDERS.contains(&lower.as_str()) {
        return None;
    }
    let mut chars = s.chars();
    let first = chars.next()?;
    if s.chars().count() > 1
        && chars.all(|c| c == first)
        && matches!(first, '0' | 'F' | 'f' | ' ' | '.' | '-')
    {
        return None;
    }
    Some(s.to_string())
}

/// Même nettoyage sur une valeur optionnelle (champ WMI absent ou nul).
pub fn clean_opt(raw: Option<&str>) -> Option<String> {
    raw.and_then(clean_smbios)
}

/// Date CIM de WMI (« 20230515000000.000000+000 ») vers AAAA-MM-JJ.
pub fn cim_date(raw: &str) -> Option<String> {
    let s = raw.trim();
    let digits = s.get(..8)?;
    if !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    iso_date(&digits[..4], &digits[4..6], &digits[6..8])
}

/// Date DMI de Linux (« 05/15/2023 », format américain MM/JJ/AAAA) vers AAAA-MM-JJ.
pub fn dmi_date(raw: &str) -> Option<String> {
    let mut parts = raw.trim().split('/');
    let (m, d, y) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() || y.len() != 4 {
        return None;
    }
    iso_date(y, &format!("{m:0>2}"), &format!("{d:0>2}"))
}

fn iso_date(y: &str, m: &str, d: &str) -> Option<String> {
    let (yn, mn, dn): (u32, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1980..=2200).contains(&yn) || !(1..=12).contains(&mn) || !(1..=31).contains(&dn) {
        return None;
    }
    Some(format!("{yn:04}-{mn:02}-{dn:02}"))
}

/// Type de mémoire d'après le code SMBIOS (table 17, champ « Memory Type »), tel que
/// `Win32_PhysicalMemory.SMBIOSMemoryType` le rapporte.
pub fn smbios_memory_type(code: u32) -> Option<&'static str> {
    Some(match code {
        18 => "DDR",
        19 => "DDR2",
        20 => "DDR2 FB-DIMM",
        24 => "DDR3",
        26 => "DDR4",
        27 => "LPDDR",
        28 => "LPDDR2",
        29 => "LPDDR3",
        30 => "LPDDR4",
        34 => "DDR5",
        35 => "LPDDR5",
        _ => return None,
    })
}

/// Température ACPI en dixièmes de kelvin (`MSAcpi_ThermalZoneTemperature`) vers °C.
pub fn decikelvin_to_celsius(dk: u32) -> f64 {
    f64::from(dk) / 10.0 - 273.15
}

/// Libellé court d'une zone thermique ACPI : « ACPI\ThermalZone\TZ00_0 » donne « ACPI TZ00 ».
pub fn thermal_zone_label(instance_name: &str) -> String {
    let last = instance_name.rsplit('\\').next().unwrap_or_default();
    let zone = last.strip_suffix("_0").unwrap_or(last);
    if zone.is_empty() {
        "ACPI".to_string()
    } else {
        format!("ACPI {zone}")
    }
}

/// Écarte les lectures absurdes (capteur absent qui renvoie 0, -40 ou 255 °C).
pub fn plausible_celsius(c: f64) -> bool {
    c.is_finite() && c > 5.0 && c < 150.0
}

/// Type de carte réseau d'après `MSFT_NetAdapter.NdisPhysicalMedium` (énumération
/// `NDIS_PHYSICAL_MEDIUM`), avec repli sur le nom du produit.
pub fn network_kind_from_ndis(medium: Option<u32>, name: &str) -> NetworkKind {
    match medium {
        Some(1 | 9) => NetworkKind::Wifi,
        Some(10) => NetworkKind::Bluetooth,
        Some(14) => NetworkKind::Ethernet,
        _ => network_kind_from_name(name).unwrap_or(NetworkKind::Other),
    }
}

/// Devine le type d'après le nom du produit, faute de mieux.
pub fn network_kind_from_name(name: &str) -> Option<NetworkKind> {
    let n = name.to_lowercase();
    if n.contains("bluetooth") {
        Some(NetworkKind::Bluetooth)
    } else if ["wi-fi", "wifi", "wireless", "802.11", "wlan"]
        .iter()
        .any(|k| n.contains(k))
    {
        Some(NetworkKind::Wifi)
    } else if ["ethernet", "gbe", "gigabit", "lan"]
        .iter()
        .any(|k| n.contains(k))
    {
        Some(NetworkKind::Ethernet)
    } else {
        None
    }
}

/// Octets bruts du registre (REG_BINARY de 4 ou 8 octets, petit-boutiste) vers entier.
/// Sert à `HardwareInformation.qwMemorySize` des cartes graphiques, stocké selon le pilote
/// en REG_QWORD ou en REG_BINARY.
pub fn le_bytes_to_u64(bytes: &[u8]) -> Option<u64> {
    match bytes.len() {
        4 => Some(u64::from(u32::from_le_bytes(bytes.try_into().ok()?))),
        8 => Some(u64::from_le_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}

// ---------- Licence Windows ----------

/// Libellé français de `SoftwareLicensingProduct.LicenseStatus`.
pub fn license_status_label(status: u32) -> Option<&'static str> {
    Some(match status {
        0 => "Sans licence",
        1 => "Activé",
        2 => "Période de grâce initiale",
        3 => "Période de grâce (changement matériel)",
        4 => "Période de grâce (licence non authentique)",
        5 => "Notification (non activé)",
        6 => "Période de grâce prolongée",
        _ => return None,
    })
}

/// Canal de licence tiré de la description : « Windows(R) Operating System, OEM_DM channel »
/// donne « OEM_DM ».
pub fn license_channel(description: &str) -> Option<String> {
    let tail = description.rsplit(", ").next()?.trim();
    let channel = tail.strip_suffix(" channel")?.trim();
    (!channel.is_empty()).then(|| channel.to_string())
}

/// Synthèse des produits de licence Windows ayant une clé installée
/// (`(LicenseStatus, Description)`). Activé si l'un d'eux est à l'état 1 ; c'est lui qui est
/// décrit, sinon le premier. Aucun produit : Windows n'a aucune clé installée.
pub fn license_from_products(products: &[(Option<u32>, Option<String>)]) -> LicenseInfo {
    let chosen = products
        .iter()
        .find(|(status, _)| *status == Some(1))
        .or_else(|| products.first());
    let (status, description) = match chosen {
        Some((s, d)) => (*s, d.clone()),
        None => (None, None),
    };
    LicenseInfo {
        activated: status == Some(1),
        status,
        status_label: status.and_then(license_status_label).map(str::to_string),
        channel: description.as_deref().and_then(license_channel),
        description,
    }
}

// ---------- Gestion d'entreprise ----------

/// Analyse la sortie de `dsregcmd /status` : lignes « Clé : Valeur » alignées à droite,
/// réparties en sections. Seule la première occurrence d'une clé compte. Les clés ne sont
/// pas traduites par Windows ; les valeurs booléennes sont `YES` / `NO`.
pub fn parse_dsregcmd(text: &str) -> DeviceJoin {
    let mut join = DeviceJoin::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(" : ") else {
            continue;
        };
        let value = value.trim();
        let yes_no = || match value.to_ascii_uppercase().as_str() {
            "YES" => Some(true),
            "NO" => Some(false),
            _ => None,
        };
        let text_value = || (!value.is_empty()).then(|| value.to_string());
        match key.trim() {
            "AzureAdJoined" if join.azure_ad_joined.is_none() => join.azure_ad_joined = yes_no(),
            "EnterpriseJoined" if join.enterprise_joined.is_none() => {
                join.enterprise_joined = yes_no()
            }
            "DomainJoined" if join.domain_joined.is_none() => join.domain_joined = yes_no(),
            "WorkplaceJoined" if join.workplace_joined.is_none() => {
                join.workplace_joined = yes_no()
            }
            "DomainName" if join.domain_name.is_none() => join.domain_name = text_value(),
            "TenantName" if join.tenant_name.is_none() => join.tenant_name = text_value(),
            "MdmUrl" if join.mdm_url.is_none() => join.mdm_url = text_value(),
            _ => {}
        }
    }
    join
}

/// Fournisseur MDM d'Intune dans `HKLM\SOFTWARE\Microsoft\Enrollments\*\ProviderID`.
pub fn is_intune_provider(provider_id: &str) -> bool {
    provider_id.trim().eq_ignore_ascii_case("MS DM Server")
}

/// Autopilot : la machine est attribuée si le registre porte un domaine ou un identifiant
/// de locataire non vide.
pub fn autopilot_from_values(
    tenant_domain: Option<&str>,
    tenant_id: Option<&str>,
) -> AutopilotInfo {
    let non_empty = |v: Option<&str>| v.map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    let tenant_domain = non_empty(tenant_domain);
    let assigned = tenant_domain.is_some() || non_empty(tenant_id).is_some();
    AutopilotInfo {
        assigned,
        tenant_domain,
    }
}

/// Synthèse « gérée par une entreprise » : jointe à Azure AD ou à un domaine, inscrite à un
/// MDM, ou attribuée dans Autopilot. Un compte professionnel ajouté par l'utilisateur
/// (`workplace_joined`) ne compte pas : il ne donne pas la main sur la machine.
pub fn is_enterprise_managed(s: &SecurityInfo) -> bool {
    let join = s.device_join.as_ref();
    let joined = join.is_some_and(|j| {
        j.azure_ad_joined == Some(true)
            || j.domain_joined == Some(true)
            || j.enterprise_joined == Some(true)
            || j.mdm_url.as_deref().is_some_and(|u| !u.trim().is_empty())
    });
    joined || s.intune_enrolled == Some(true) || s.autopilot.as_ref().is_some_and(|a| a.assigned)
}

/// Version de la norme TPM tirée de `Win32_Tpm.SpecVersion` (« 2.0, 0, 1.38 » donne « 2.0 »).
pub fn tpm_spec_version(spec: &str) -> Option<String> {
    let v = spec.split(',').next()?.trim();
    (!v.is_empty()).then(|| v.to_string())
}

// ---------- Batterie ----------

/// Mesure d'une batterie, avant regroupement.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BatteryReading {
    pub design_mwh: Option<u64>,
    pub full_mwh: Option<u64>,
    /// Santé calculée dans l'unité d'origine : utile sous Linux quand la batterie parle en
    /// µAh sans donner sa tension (conversion en mWh impossible, rapport encore juste).
    pub health_pct: Option<f64>,
    pub cycle_count: Option<u32>,
}

/// Santé = pleine charge / origine, en %, arrondie à 0,1. `None` si l'origine est nulle.
pub fn health_pct(full: u64, design: u64) -> Option<f64> {
    if design == 0 {
        return None;
    }
    let pct = full as f64 / design as f64 * 100.0;
    Some((pct * 10.0).round() / 10.0)
}

/// Regroupe plusieurs batteries : capacités additionnées (si toutes connues), cycles au
/// maximum. Aucune lecture : `None` (pas de batterie).
pub fn merge_batteries(readings: &[BatteryReading]) -> Option<BatteryInfo> {
    if readings.is_empty() {
        return None;
    }
    let sum = |f: fn(&BatteryReading) -> Option<u64>| -> Option<u64> {
        readings
            .iter()
            .map(f)
            .try_fold(0u64, |acc, v| acc.checked_add(v?))
    };
    let design = sum(|r| r.design_mwh);
    let full = sum(|r| r.full_mwh);
    let health = match (full, design, readings) {
        (Some(f), Some(d), _) => health_pct(f, d),
        (_, _, [only]) => only.health_pct,
        _ => None,
    };
    Some(BatteryInfo {
        count: u32::try_from(readings.len()).unwrap_or(u32::MAX),
        design_capacity_mwh: design,
        full_charge_capacity_mwh: full,
        cycle_count: readings.iter().filter_map(|r| r.cycle_count).max(),
        health_pct: health,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smbios_placeholders_are_dropped() {
        assert_eq!(clean_smbios("  To Be Filled By O.E.M. "), None);
        assert_eq!(clean_smbios("Default string"), None);
        assert_eq!(clean_smbios("00000000"), None);
        assert_eq!(clean_smbios("FFFFFFFF"), None);
        assert_eq!(clean_smbios(""), None);
        assert_eq!(clean_smbios(" LENOVO "), Some("LENOVO".into()));
        assert_eq!(
            clean_smbios("0"),
            Some("0".into()),
            "un seul caractère reste une valeur"
        );
    }

    #[test]
    fn dates_are_normalised() {
        assert_eq!(
            cim_date("20230515000000.000000+000").as_deref(),
            Some("2023-05-15")
        );
        assert_eq!(cim_date("abc"), None);
        assert_eq!(dmi_date("05/15/2023").as_deref(), Some("2023-05-15"));
        assert_eq!(dmi_date("5/1/2023").as_deref(), Some("2023-05-01"));
        assert_eq!(dmi_date("15/05/2023"), None, "mois 15 invalide");
        assert_eq!(dmi_date("2023-05-15"), None);
    }

    #[test]
    fn license_prefers_activated_product() {
        let products = vec![
            (
                Some(0),
                Some("Windows(R) Operating System, VOLUME_KMSCLIENT channel".into()),
            ),
            (
                Some(1),
                Some("Windows(R) Operating System, OEM_DM channel".into()),
            ),
        ];
        let l = license_from_products(&products);
        assert!(l.activated);
        assert_eq!(l.channel.as_deref(), Some("OEM_DM"));
        assert_eq!(l.status_label.as_deref(), Some("Activé"));

        let none = license_from_products(&[]);
        assert!(!none.activated);
        assert_eq!(none.status, None);

        let grace = license_from_products(&[(Some(5), None)]);
        assert!(!grace.activated);
        assert_eq!(grace.channel, None);
    }

    #[test]
    fn autopilot_needs_a_non_empty_value() {
        assert!(!autopilot_from_values(None, None).assigned);
        assert!(!autopilot_from_values(Some("  "), Some("")).assigned);
        let a = autopilot_from_values(Some("exemple.onmicrosoft.com"), None);
        assert!(a.assigned);
        assert_eq!(a.tenant_domain.as_deref(), Some("exemple.onmicrosoft.com"));
        assert!(autopilot_from_values(None, Some("00000000-1111-2222-3333-444444444444")).assigned);
    }

    #[test]
    fn enterprise_synthesis() {
        let mut s = SecurityInfo::default();
        assert!(!is_enterprise_managed(&s));
        s.device_join = Some(DeviceJoin {
            workplace_joined: Some(true),
            ..DeviceJoin::default()
        });
        assert!(!is_enterprise_managed(&s), "compte professionnel seul");
        s.intune_enrolled = Some(true);
        assert!(is_enterprise_managed(&s));
        s.intune_enrolled = Some(false);
        s.autopilot = Some(autopilot_from_values(Some("x.example"), None));
        assert!(is_enterprise_managed(&s));
    }

    #[test]
    fn network_kinds() {
        assert_eq!(network_kind_from_ndis(Some(9), ""), NetworkKind::Wifi);
        assert_eq!(network_kind_from_ndis(Some(14), ""), NetworkKind::Ethernet);
        assert_eq!(network_kind_from_ndis(Some(10), ""), NetworkKind::Bluetooth);
        assert_eq!(
            network_kind_from_ndis(Some(0), "Intel(R) Wi-Fi 6 AX201 160MHz"),
            NetworkKind::Wifi
        );
        assert_eq!(
            network_kind_from_ndis(None, "Carte inconnue"),
            NetworkKind::Other
        );
    }

    #[test]
    fn battery_merge() {
        assert_eq!(merge_batteries(&[]), None);
        let one = BatteryReading {
            design_mwh: Some(57_000),
            full_mwh: Some(45_600),
            health_pct: health_pct(45_600, 57_000),
            cycle_count: Some(312),
        };
        let b = merge_batteries(std::slice::from_ref(&one)).unwrap();
        assert_eq!(b.health_pct, Some(80.0));
        assert_eq!(b.count, 1);

        let two = BatteryReading {
            design_mwh: Some(23_000),
            full_mwh: Some(20_000),
            health_pct: None,
            cycle_count: Some(40),
        };
        let b = merge_batteries(&[one.clone(), two]).unwrap();
        assert_eq!(b.design_capacity_mwh, Some(80_000));
        assert_eq!(b.full_charge_capacity_mwh, Some(65_600));
        assert_eq!(b.health_pct, Some(82.0));
        assert_eq!(b.cycle_count, Some(312));

        // Capacités en µAh sans tension : pas de mWh, mais la santé reste connue.
        let charge_only = BatteryReading {
            health_pct: Some(71.3),
            ..BatteryReading::default()
        };
        let b = merge_batteries(&[charge_only]).unwrap();
        assert_eq!(b.design_capacity_mwh, None);
        assert_eq!(b.health_pct, Some(71.3));
    }

    #[test]
    fn misc_conversions() {
        assert!((decikelvin_to_celsius(3232) - 50.05).abs() < 1e-9);
        assert!(!plausible_celsius(0.0));
        assert_eq!(thermal_zone_label(r"ACPI\ThermalZone\TZ00_0"), "ACPI TZ00");
        assert_eq!(thermal_zone_label(""), "ACPI");
        assert_eq!(tpm_spec_version("2.0, 0, 1.38").as_deref(), Some("2.0"));
        assert_eq!(
            le_bytes_to_u64(&[0, 0, 0, 0x80, 0, 0, 0, 0]),
            Some(0x8000_0000)
        );
        assert_eq!(le_bytes_to_u64(&[0, 0, 0, 0x40]), Some(0x4000_0000));
        assert_eq!(le_bytes_to_u64(&[1, 2]), None);
        assert_eq!(smbios_memory_type(34), Some("DDR5"));
        assert_eq!(smbios_memory_type(0), None);
    }
}
