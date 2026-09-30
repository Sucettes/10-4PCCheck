//! Inventaire de la vraie machine. On ne vérifie que la forme (champs présents, valeurs
//! plausibles) : aucune valeur n'est affichée ni écrite sur disque.

use pccheck_inventory::collect;
use serde_json::Value;

/// Plus grand entier représentable exactement par un nombre JavaScript.
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

fn assert_js_safe(v: &Value, path: &str) {
    match v {
        Value::Number(n) => {
            if let Some(u) = n.as_u64() {
                assert!(u <= MAX_SAFE_INTEGER, "{path} dépasse 2^53");
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                assert_js_safe(item, &format!("{path}[{i}]"));
            }
        }
        Value::Object(map) => {
            for (k, item) in map {
                assert_js_safe(item, &format!("{path}.{k}"));
            }
        }
        _ => {}
    }
}

#[test]
fn collect_returns_a_plausible_inventory() {
    let inv = collect();
    if !cfg!(any(windows, target_os = "linux")) {
        assert!(!inv.errors.is_empty());
        return;
    }
    let os = inv.os.as_ref().expect("système lu");
    assert!(os.name.is_some());

    let cpu = inv.cpu.as_ref().expect("processeur lu");
    let threads = cpu.threads.expect("nombre de fils");
    assert!((1..=4096).contains(&threads));
    if let Some(cores) = cpu.cores {
        assert!(cores >= 1 && cores <= threads);
    }

    let total = inv
        .memory
        .as_ref()
        .and_then(|m| m.total_bytes)
        .expect("mémoire totale");
    assert!(total >= 256 << 20);

    let security = inv.security.as_ref().expect("section sécurité");
    assert!(security.secure_boot.is_some() || !inv.errors.is_empty());
    if let Some(b) = &inv.battery {
        assert!(b.count >= 1);
        if let Some(h) = b.health_pct {
            assert!(h > 0.0 && h < 200.0);
        }
    }
    for s in &inv.temperatures {
        assert!(s.celsius > 5.0 && s.celsius < 150.0);
    }
    for e in &inv.errors {
        assert!(e.contains(" : "), "message d'erreur sans partie : {e}");
    }

    let json = serde_json::to_value(&inv).expect("sérialisable");
    assert_js_safe(&json, "inventaire");
    assert!(json.get("errors").is_some());
}
