//! Comptes connectés (`adb shell dumpsys account`).
//!
//! Format : un bloc par utilisateur Android, par exemple
//! ```text
//! User UserInfo{0:Propriétaire:c13}:
//!   Accounts: 2
//!     Account {name=compte1@example.com, type=com.google}
//! ```
//! suivi d'un historique et de la liste des types de comptes installés (`ServiceInfo: ...`),
//! ignorés. **Seul le type est lu** : l'adresse du compte n'est jamais conservée.

use std::collections::BTreeMap;

use serde::Serialize;

/// Nombre de comptes d'un type donné, tous utilisateurs confondus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountCount {
    /// Type Android du compte (`com.google`, `com.samsung.account`).
    pub account_type: String,
    /// Nom lisible si le type est connu.
    pub label: Option<&'static str>,
    pub count: u32,
    /// Ce compte verrouille le téléphone après une réinitialisation s'il n'est pas retiré avant
    /// (FRP chez Google, verrou de réactivation chez Samsung...).
    pub activation_lock: bool,
}

/// Types connus : (type, nom lisible, verrou après réinitialisation).
const KNOWN_TYPES: [(&str, &str, bool); 8] = [
    ("com.google", "Google", true),
    ("com.samsung.account", "Samsung", true),
    ("com.osp.app.signin", "Samsung", true),
    ("com.xiaomi", "Xiaomi", true),
    ("com.huawei.hwid", "Huawei", true),
    (
        "com.microsoft.workaccount",
        "Microsoft (compte professionnel)",
        false,
    ),
    ("com.whatsapp", "WhatsApp", false),
    ("com.google.android.gm.exchange", "Exchange", false),
];

/// Analyse `dumpsys account`. `None` si la sortie ne ressemble pas à une liste de comptes
/// (refus d'accès, service absent) : l'absence de comptes n'est alors pas prouvée.
pub fn parse_dumpsys_account(text: &str) -> Option<Vec<AccountCount>> {
    let mut readable = false;
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("Accounts:") {
            readable = true;
        } else if let Some(rest) = line.strip_prefix("Account {") {
            readable = true;
            // Dernier « type= » : le nom, placé avant, pourrait en contenir un.
            let Some(pos) = rest.rfind("type=") else {
                continue;
            };
            let account_type = rest[pos + "type=".len()..]
                .trim_end_matches('}')
                .trim()
                .to_string();
            if !account_type.is_empty() {
                *counts.entry(account_type).or_insert(0) += 1;
            }
        }
    }
    if !readable {
        return None;
    }
    Some(
        counts
            .into_iter()
            .map(|(account_type, count)| {
                let known = KNOWN_TYPES.iter().find(|(t, _, _)| *t == account_type);
                AccountCount {
                    label: known.map(|(_, l, _)| *l),
                    activation_lock: known.is_some_and(|(_, _, lock)| *lock),
                    account_type,
                    count,
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_denial_is_unreadable() {
        let text = "Permission Denial: can't dump AccountsManager from from pid=4242, uid=2000 \
                    without permission android.permission.DUMP\n";
        assert_eq!(parse_dumpsys_account(text), None);
    }

    #[test]
    fn name_containing_type_is_not_confused() {
        let text = "  Accounts: 1\n    Account {name=type=faux, type=com.google}\n";
        let accounts = parse_dumpsys_account(text).unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].account_type, "com.google");
    }
}
