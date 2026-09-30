//! Analyse de chaque sortie de commande à partir des fixtures reconstruites.

use pccheck_android::{
    identity_from_props, parse_devices, parse_df, parse_dpm_list_owners, parse_dumpsys_account,
    parse_dumpsys_battery, parse_dumpsys_device_policy, parse_getprop, parse_version,
    security_from_props, BatteryHealth, ChargeStatus, Date, DeviceState, VerifiedBootState,
};

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path} : {e}"))
}

// ---------- adb ----------

#[test]
fn adb_version_ignores_install_path() {
    let v = parse_version(&fixture("adb_version.txt")).unwrap();
    assert_eq!(v.protocol, "1.0.41");
    assert_eq!(v.release.as_deref(), Some("35.0.2-12147458"));
    assert!(parse_version("error: unknown\n").is_none());
}

#[test]
fn devices_mixed_states() {
    let d = parse_devices(&fixture("devices_mixed.txt"));
    assert_eq!(d.len(), 5);

    assert_eq!(d[0].serial, "R58N00000XX");
    assert_eq!(d[0].serial_masked, "R5*******XX");
    assert_eq!(d[0].state, DeviceState::Device);
    assert_eq!(d[0].model.as_deref(), Some("SM_G973W"));
    assert_eq!(d[0].product.as_deref(), Some("beyond1ltevl"));
    assert_eq!(d[0].device.as_deref(), Some("beyond1"));
    assert_eq!(d[0].transport_id, Some(1));
    assert!(d[0].guidance.is_none());

    assert_eq!(d[1].state, DeviceState::Unauthorized);
    assert_eq!(d[1].model, None);
    assert!(d[1].guidance.as_deref().unwrap().contains("clé RSA"));

    assert_eq!(d[2].serial, "192.168.1.50:5555");
    assert_eq!(d[2].state, DeviceState::Offline);
    assert_eq!(d[2].model.as_deref(), Some("Pixel_7"));

    assert_eq!(d[3].serial, "emulator-5554");
    assert_eq!(d[3].state, DeviceState::Device);

    assert_eq!(d[4].state, DeviceState::Other("recovery".into()));
    assert!(d[4].guidance.as_deref().unwrap().contains("redémarre"));
}

#[test]
fn devices_skip_daemon_start_lines() {
    let d = parse_devices(&fixture("devices_daemon_start.txt"));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].serial, "2A000000XX000");
    assert_eq!(d[0].model.as_deref(), Some("Pixel_7"));
}

#[test]
fn devices_linux_without_udev_rules() {
    let d = parse_devices(&fixture("devices_linux_no_permissions.txt"));
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].state, DeviceState::NoPermissions);
    assert_eq!(d[0].transport_id, Some(1));
    assert!(d[0].guidance.as_deref().unwrap().contains("udev"));
}

#[test]
fn devices_empty_and_short_format() {
    assert!(parse_devices(&fixture("devices_empty.txt")).is_empty());
    assert!(parse_devices("").is_empty());
    let d = parse_devices(&fixture("devices_short_crlf.txt"));
    assert_eq!(d.len(), 2);
    assert_eq!(d[0].state, DeviceState::Device);
    assert_eq!(d[1].state, DeviceState::Unauthorized);
    assert_eq!(d[1].serial, "0A000000000000");
}

// ---------- getprop ----------

#[test]
fn getprop_samsung() {
    let p = parse_getprop(&fixture("getprop_samsung.txt"));
    let id = identity_from_props(&p);
    assert_eq!(id.manufacturer.as_deref(), Some("samsung"));
    assert_eq!(id.brand.as_deref(), Some("samsung"));
    assert_eq!(id.model.as_deref(), Some("SM-G973W"));
    assert_eq!(id.product_name.as_deref(), Some("beyond1ltevl"));
    assert_eq!(id.device_code.as_deref(), Some("beyond1"));
    assert_eq!(id.android_version.as_deref(), Some("12"));
    assert_eq!(id.sdk, Some(31));
    assert_eq!(id.first_api_level, Some(28));
    assert_eq!(id.sales_code.as_deref(), Some("XAC"));
    assert!(id
        .build_fingerprint
        .as_deref()
        .unwrap()
        .ends_with("user/release-keys"));

    let s = security_from_props(&p);
    assert_eq!(s.security_patch, Date::new(2023, 1, 1));
    assert_eq!(s.verified_boot, Some(VerifiedBootState::Green));
    assert_eq!(s.bootloader_locked, Some(true));
    assert_eq!(s.knox_warranty_void, Some(false));
    assert_eq!(s.root.debuggable, Some(false));
    assert_eq!(s.root.test_keys, Some(false));
    assert_eq!(s.root.build_type.as_deref(), Some("user"));
    assert_eq!(s.root.su_found, None, "vient de `which su`, pas de getprop");
    assert!(s.root.reasons().is_empty());
}

#[test]
fn getprop_pixel() {
    let p = parse_getprop(&fixture("getprop_pixel.txt"));
    let id = identity_from_props(&p);
    assert_eq!(id.manufacturer.as_deref(), Some("Google"));
    assert_eq!(id.model.as_deref(), Some("Pixel 7"));
    assert_eq!(id.android_version.as_deref(), Some("16"));
    assert_eq!(id.sdk, Some(36));
    assert_eq!(id.sales_code, None, "pas de code vendeur hors Samsung");
    let s = security_from_props(&p);
    assert_eq!(s.security_patch, Date::new(2026, 8, 5));
    assert_eq!(s.knox_warranty_void, None);
    assert_eq!(s.bootloader_locked, Some(true));
}

#[test]
fn getprop_unlocked_rooted_pixel() {
    let text = fixture("getprop_pixel.txt")
        .replace("[ro.boot.flash.locked]: [1]", "[ro.boot.flash.locked]: [0]")
        .replace(
            "[ro.boot.vbmeta.device_state]: [locked]",
            "[ro.boot.vbmeta.device_state]: [unlocked]",
        )
        .replace(
            "[ro.boot.verifiedbootstate]: [green]",
            "[ro.boot.verifiedbootstate]: [orange]",
        )
        .replace("[ro.debuggable]: [0]", "[ro.debuggable]: [1]")
        .replace("[ro.build.type]: [user]", "[ro.build.type]: [userdebug]")
        .replace(
            "[ro.build.tags]: [release-keys]",
            "[ro.build.tags]: [test-keys]",
        );
    let s = security_from_props(&parse_getprop(&text));
    assert_eq!(s.verified_boot, Some(VerifiedBootState::Orange));
    assert_eq!(s.bootloader_locked, Some(false));
    assert_eq!(s.root.reasons().len(), 3);
}

// ---------- batterie ----------

#[test]
fn battery_old_format() {
    let b = parse_dumpsys_battery(&fixture("battery_old.txt")).unwrap();
    assert_eq!(b.level_pct, Some(57));
    assert_eq!(b.status, Some(ChargeStatus::Charging));
    assert_eq!(b.health, Some(BatteryHealth::Good));
    assert_eq!(b.health_label.as_deref(), Some("Bonne"));
    assert_eq!(b.temperature_decicelsius, Some(301));
    assert_eq!(b.temperature_c, Some(30.1));
    assert_eq!(b.voltage_mv, Some(3872), "« voltage:3872 » sans espace");
    assert_eq!(b.technology.as_deref(), Some("Li-ion"));
    assert_eq!(b.present, Some(true));
    assert_eq!(b.charge_counter_uah, None);
    assert!(!b.updates_stopped);
}

#[test]
fn battery_new_format_with_sysfs() {
    let b = parse_dumpsys_battery(&fixture("battery_new.txt"))
        .unwrap()
        .with_sysfs(Some("523\n"), Some("3688000\n"), Some("4270000\n"));
    assert_eq!(b.level_pct, Some(85));
    assert_eq!(b.voltage_mv, Some(4231));
    assert_eq!(b.temperature_decicelsius, Some(290));
    assert_eq!(b.charge_counter_uah, Some(3_218_000));
    assert_eq!(b.cycle_count, Some(523));
    assert_eq!(b.charge_full_uah, Some(3_688_000));
    assert_eq!(b.charge_full_design_uah, Some(4_270_000));
    assert_eq!(b.capacity_pct, Some(86));
}

#[test]
fn battery_samsung_frozen_values() {
    let b = parse_dumpsys_battery(&fixture("battery_samsung_stopped.txt")).unwrap();
    assert!(b.updates_stopped);
    assert_eq!(b.level_pct, Some(100), "première occurrence de « level »");
    assert_eq!(b.status, Some(ChargeStatus::Full));
    assert_eq!(b.health, Some(BatteryHealth::Overheat));
    assert_eq!(b.voltage_mv, Some(4380), "µV ramenés en mV");
    assert_eq!(b.temperature_decicelsius, Some(471));
    assert_eq!(b.charge_counter_uah, None, "compteur à 0 = non renseigné");
}

// ---------- comptes ----------

#[test]
fn accounts_are_counted_by_type_without_names() {
    let text = fixture("account_with.txt");
    let a = parse_dumpsys_account(&text).unwrap();
    let find = |t: &str| a.iter().find(|c| c.account_type == t).unwrap();
    assert_eq!(a.len(), 4);
    assert_eq!(find("com.google").count, 2);
    assert!(find("com.google").activation_lock);
    assert_eq!(find("com.google").label, Some("Google"));
    assert_eq!(find("com.samsung.account").count, 1);
    assert!(find("com.samsung.account").activation_lock);
    assert_eq!(find("com.whatsapp").count, 1);
    assert!(!find("com.whatsapp").activation_lock);
    assert_eq!(
        find("com.microsoft.workaccount").count,
        1,
        "compte du profil de travail compté, ServiceInfo ignoré"
    );
    let json = serde_json::to_string(&a).unwrap();
    assert!(!json.contains("example.com"), "aucune adresse : {json}");
    assert!(!json.contains("Propriétaire"), "aucun nom d'utilisateur");
}

#[test]
fn no_accounts() {
    assert_eq!(
        parse_dumpsys_account(&fixture("account_none.txt")),
        Some(vec![])
    );
}

// ---------- propriétaires ----------

#[test]
fn dpm_without_owner() {
    let o = parse_dpm_list_owners(&fixture("dpm_none.txt")).unwrap();
    assert!(o.is_empty());
}

#[test]
fn dpm_device_owner() {
    let o = parse_dpm_list_owners(&fixture("dpm_device_owner.txt")).unwrap();
    assert_eq!(o.device_owner.as_deref(), Some("com.example.mdm"));
    assert!(o.profile_owners.is_empty());
}

#[test]
fn dpm_managed_profile() {
    let o = parse_dpm_list_owners(&fixture("dpm_profile_owner.txt")).unwrap();
    assert_eq!(o.device_owner, None);
    assert_eq!(o.profile_owners.len(), 1);
    assert_eq!(o.profile_owners[0].user_id, 10);
    assert_eq!(o.profile_owners[0].package, "com.example.work");
    assert!(o.profile_owners[0].managed_profile);
}

#[test]
fn dpm_old_android_falls_back_to_dumpsys() {
    assert_eq!(parse_dpm_list_owners(&fixture("dpm_unknown_old.txt")), None);
    let o = parse_dumpsys_device_policy(&fixture("device_policy_old.txt")).unwrap();
    assert_eq!(o.device_owner.as_deref(), Some("com.example.mdm"));
    assert_eq!(o.profile_owners.len(), 1);
    assert_eq!(o.profile_owners[0].user_id, 10);
    assert_eq!(o.profile_owners[0].package, "com.example.work");
    let empty = parse_dumpsys_device_policy(
        "Current Device Policy Manager state:\n  Immutable state:\n    mHasFeature=true\n",
    )
    .unwrap();
    assert!(empty.is_empty());
}

// ---------- stockage ----------

#[test]
fn df_toybox() {
    let s = parse_df(&fixture("df_toybox.txt")).unwrap();
    assert_eq!(s.total_bytes, 110_691_636 * 1024);
    assert_eq!(s.used_bytes, 40_123_456 * 1024);
    assert_eq!(s.free_bytes, 70_437_796 * 1024);
}

#[test]
fn df_toolbox_human_sizes() {
    let s = parse_df(&fixture("df_toolbox.txt")).unwrap();
    assert_eq!(s.total_bytes, 13_421_772_800);
    assert_eq!(s.used_bytes, 5_583_457_485);
    assert_eq!(s.free_bytes, 7_838_315_315);
}

#[test]
fn df_wrapped_filesystem_name() {
    let s = parse_df(&fixture("df_wrapped.txt")).unwrap();
    assert_eq!(s.total_bytes, 52_428_800 * 1024);
    assert_eq!(s.free_bytes, 51_380_224 * 1024);
}
