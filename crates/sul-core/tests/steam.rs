use std::fs;
use std::path::PathBuf;
use sul_core::steam::*;
use sul_core::Error;

const LIBS: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"553850"		"123456"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
	}
}
"#;

const ACF: &str = r#"
"AppState"
{
	"appid"		"553850"
	"Universe"		"1"
	"name"		"HELLDIVERS™ 2"
	"StateFlags"		"4"
	"installdir"		"Helldivers 2"
	"buildid"		"12345678"
	"AutoUpdateBehavior"		"1"
}
"#;

fn acf_with_installdir(dir: &str) -> String {
    format!("\"AppState\"\n{{\n\"appid\" \"1\"\n\"installdir\" \"{dir}\"\n}}\n")
}

#[test]
fn parses_library_folders_and_unescapes_paths() {
    let libs = library_folders(LIBS).unwrap();
    assert_eq!(libs, vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"D:\SteamLibrary")]);
}

#[test]
fn parses_app_manifest_fields() {
    let m = parse_app_manifest(ACF).unwrap();
    assert_eq!(m.app_id, 553850);
    assert_eq!(m.name, "HELLDIVERS™ 2");
    assert_eq!(m.install_dir, "Helldivers 2");
    assert_eq!(m.build_id.as_deref(), Some("12345678"));
    assert_eq!(m.auto_update_behavior, Some(1));
}

#[test]
fn rejects_path_like_or_missing_manifest_fields() {
    assert!(parse_app_manifest(&acf_with_installdir(r"..\\..\\Windows")).is_err());
    assert!(parse_app_manifest(&acf_with_installdir("a/b")).is_err());
    assert!(matches!(parse_app_manifest(&acf_with_installdir("a\\\\b")), Err(Error::UnsafePath(_))));
    assert!(parse_app_manifest("\"AppState\"\n{\n\"name\" \"x\"\n}\n").is_err());
    assert!(parse_app_manifest("\"Other\"\n{\n}\n").is_err());
}

#[test]
fn malformed_vdf_is_an_error_not_a_panic() {
    assert!(parse_vdf("\"a\" \"unterminated").is_err());
    assert!(parse_vdf("\"a\"\n{\n\"b\" \"c\"\n").is_err());
    assert!(parse_vdf("}").is_err());
    assert!(parse_vdf("\"key\"").is_err());
    let deep = "\"a\" {".repeat(200) + &"}".repeat(200);
    assert!(parse_vdf(&deep).is_err());
}

#[test]
fn comments_and_unquoted_tokens_are_tolerated() {
    let v = parse_vdf("// comment\nroot { k v }\n").unwrap();
    assert_eq!(v.get("root").and_then(|r| r.get("k")).and_then(Vdf::as_str), Some("v"));
}

#[test]
fn scan_reports_bad_manifests_and_only_reads() {
    let d = tempfile::tempdir().unwrap();
    let sa = d.path().join("steamapps");
    fs::create_dir_all(sa.join("common")).unwrap();
    fs::write(sa.join("appmanifest_553850.acf"), ACF).unwrap();
    fs::write(sa.join("appmanifest_2.acf"), "not vdf at all {{{").unwrap();
    fs::write(sa.join("libraryfolders.vdf"), LIBS).unwrap();
    let before: Vec<_> = fs::read_dir(&sa).unwrap().map(|e| e.unwrap().file_name()).collect();

    let (games, warnings) = scan_steamapps(&sa).unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(warnings.len(), 1);
    assert_eq!(games[0].install_path, sa.join("common").join("Helldivers 2"));

    let mut after: Vec<_> = fs::read_dir(&sa).unwrap().map(|e| e.unwrap().file_name()).collect();
    let mut before = before;
    before.sort();
    after.sort();
    assert_eq!(before, after, "scan must not create or remove files");
    assert_eq!(fs::read_to_string(sa.join("appmanifest_553850.acf")).unwrap(), ACF);
}

#[test]
fn launch_url_uses_steam_protocol_only() {
    assert_eq!(launch_url(553850), "steam://rungameid/553850");
}

#[test]
fn managed_builds_must_stay_outside_steam_libraries() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().to_path_buf();
    fs::create_dir_all(root.join("steamapps").join("common").join("game")).unwrap();
    fs::create_dir_all(root.join("SUL-builds").join("legacy")).unwrap();
    let libs = vec![root.clone()];
    assert!(is_inside_steam_library(&root.join("steamapps").join("common").join("game"), &libs));
    assert!(!is_inside_steam_library(&root.join("SUL-builds").join("legacy"), &libs));
    assert!(!is_inside_steam_library(&PathBuf::from("/definitely/elsewhere"), &libs));
}
