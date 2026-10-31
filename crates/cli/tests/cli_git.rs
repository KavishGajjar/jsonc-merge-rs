use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::TempDir;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_jsonc-merge")
}
fn cli(dir: &Path, args: &[&str]) -> Output {
    Command::new(binary())
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}
fn inputs(dir: &Path, b: &str, o: &str, t: &str) {
    for (name, text) in [("base.jsonc", b), ("ours.jsonc", o), ("theirs.jsonc", t)] {
        fs::write(dir.join(name), text).unwrap();
    }
}
fn three() -> [&'static str; 3] {
    ["base.jsonc", "ours.jsonc", "theirs.jsonc"]
}

#[test]
fn cli_check_merge_report_and_exit_categories() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    let version = cli(dir, &["--version"]);
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        format!("jsonc-merge {}", env!("CARGO_PKG_VERSION"))
    );
    inputs(
        dir,
        r#"{"a":1,"b":1}"#,
        r#"{"a":2,"b":1}"#,
        r#"{"a":1,"b":2}"#,
    );
    let before = fs::read(dir.join("ours.jsonc")).unwrap();
    let mut args = vec!["merge"];
    args.extend(three());
    let result = cli(dir, &args);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stdout).unwrap(),
        serde_json::json!({"a":2,"b":2})
    );
    assert_eq!(fs::read(dir.join("ours.jsonc")).unwrap(), before);
    assert!(
        cli(dir, &["check", "base.jsonc", "ours.jsonc"])
            .status
            .success()
    );
    inputs(dir, "1", "2", "3");
    let result = cli(dir, &args);
    assert_eq!(result.status.code(), Some(1));
    assert!(result.stdout.is_empty());
    let report: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(report["clean"], false);
    let mut args = vec!["plan"];
    args.extend(three());
    let result = cli(dir, &args);
    assert_eq!(result.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    fs::write(dir.join("base.jsonc"), "{bad").unwrap();
    let result = cli(dir, &args);
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert_eq!(cli(dir, &["check", "missing.jsonc"]).status.code(), Some(2));
}

#[test]
fn policy_cli_and_git_scratch_are_safe() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    inputs(
        dir,
        r#"{"items":[{"id":"a"},{"id":"b"}]}"#,
        r#"{"items":[{"id":"b"},{"id":"a"}]}"#,
        r#"{"items":[{"id":"a","x":2},{"id":"b"}]}"#,
    );
    fs::write(
        dir.join("policy.json"),
        r#"{"version":1,"arrays":[{"path":"/items","key":"id","order":"ordered"}]}"#,
    )
    .unwrap();
    let before = fs::read(dir.join("base.jsonc")).unwrap();
    let result = cli(
        dir,
        &[
            "git-driver",
            "base.jsonc",
            "ours.jsonc",
            "theirs.jsonc",
            "9",
            "--policy",
            "policy.json",
        ],
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let out: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.join("ours.jsonc")).unwrap()).unwrap();
    assert_eq!(out["items"][1]["x"], 2);
    assert_eq!(fs::read(dir.join("base.jsonc")).unwrap(), before);
    inputs(dir, "1", "2", "3");
    let result = cli(
        dir,
        &[
            "git-driver",
            "base.jsonc",
            "ours.jsonc",
            "theirs.jsonc",
            "9",
        ],
    );
    assert_eq!(result.status.code(), Some(1));
    let text = fs::read_to_string(dir.join("ours.jsonc")).unwrap();
    for marker in [
        "<<<<<<<<< OURS\n2\n",
        "||||||||| BASE\n1\n",
        "=========\n3\n",
        ">>>>>>>>> THEIRS\n",
    ] {
        assert!(text.contains(marker), "{text}");
    }
    inputs(dir, "1", "2", "3");
    fs::write(dir.join("theirs.jsonc"), "invalid").unwrap();
    assert_eq!(
        cli(
            dir,
            &["git-driver", "base.jsonc", "ours.jsonc", "theirs.jsonc"]
        )
        .status
        .code(),
        Some(2)
    );
    assert_eq!(fs::read_to_string(dir.join("ours.jsonc")).unwrap(), "2");
}

#[test]
fn diagnostics_identify_the_input_file_and_source_span() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    inputs(dir, "{}", "{bad", "{}");
    let output = cli(dir, &["check", "base.jsonc", "ours.jsonc"]);
    assert_eq!(output.status.code(), Some(2));
    let diagnostic: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["error"]["file"], "ours.jsonc");
    assert_eq!(diagnostic["error"]["code"], "syntax");
    assert!(diagnostic["error"]["span"].is_object());
    let output = cli(dir, &["check", "missing.jsonc"]);
    let diagnostic: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["error"]["file"], "missing.jsonc");
    assert_eq!(diagnostic["error"]["code"], "io");
    fs::write(dir.join("invalid-policy.json"), "{bad").unwrap();
    let output = cli(
        dir,
        &["check", "base.jsonc", "--policy", "invalid-policy.json"],
    );
    let diagnostic: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["error"]["file"], "invalid-policy.json");
    inputs(dir, "[]", "[{}]", "[]");
    fs::write(
        dir.join("policy.json"),
        r#"{"version":1,"arrays":[{"path":"","key":"id","order":"ordered"}]}"#,
    )
    .unwrap();
    let output = cli(
        dir,
        &[
            "merge",
            "base.jsonc",
            "ours.jsonc",
            "theirs.jsonc",
            "--policy",
            "policy.json",
        ],
    );
    let diagnostic: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["error"]["code"], "identity");
    assert_eq!(diagnostic["error"]["input"], "ours");
    assert_eq!(diagnostic["error"]["file"], "ours.jsonc");
}

#[cfg(unix)]
#[test]
fn diagnostics_do_not_panic_on_non_utf8_filenames() {
    use std::os::unix::ffi::OsStringExt;
    let temp = TempDir::new().unwrap();
    let name = std::ffi::OsString::from_vec(b"invalid-\xff.jsonc".to_vec());
    // macOS rejects non-UTF-8 filenames at the filesystem boundary. A missing
    // path still exercises the diagnostic without requiring such a file to exist.
    let output = Command::new(binary())
        .current_dir(temp.path())
        .arg("check")
        .arg(&name)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let diagnostic: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["error"]["code"], "io");
    assert_eq!(diagnostic["error"]["file"], name.to_string_lossy().as_ref());
}

#[cfg(target_os = "linux")]
#[test]
fn existing_non_utf8_filenames_report_source_errors() {
    use std::os::unix::ffi::OsStringExt;
    let temp = TempDir::new().unwrap();
    let name = std::ffi::OsString::from_vec(b"invalid-\xff.jsonc".to_vec());
    fs::write(temp.path().join(&name), "{bad").unwrap();
    let output = Command::new(binary())
        .current_dir(temp.path())
        .arg("check")
        .arg(&name)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let diagnostic: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["error"]["code"], "syntax");
    assert_eq!(diagnostic["error"]["file"], name.to_string_lossy().as_ref());
}

#[cfg(unix)]
#[test]
fn git_driver_refuses_symlinks_without_touching_targets() {
    let temp = TempDir::new().unwrap();
    let dir = temp.path();
    inputs(dir, "1", "1", "2");
    fs::rename(dir.join("ours.jsonc"), dir.join("target.jsonc")).unwrap();
    std::os::unix::fs::symlink("target.jsonc", dir.join("ours.jsonc")).unwrap();
    assert_eq!(
        cli(
            dir,
            &["git-driver", "base.jsonc", "ours.jsonc", "theirs.jsonc"]
        )
        .status
        .code(),
        Some(2)
    );
    assert_eq!(fs::read_to_string(dir.join("target.jsonc")).unwrap(), "1");
}

fn git(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .args(args)
        .output()
        .unwrap()
}
fn success(dir: &Path, args: &[&str]) {
    let result = git(dir, args);
    assert!(
        result.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
fn setup(dir: &Path) {
    success(dir, &["init", "-b", "main"]);
    success(dir, &["config", "user.email", "test@example.invalid"]);
    success(dir, &["config", "user.name", "Local fixture"]);
    success(dir, &["config", "commit.gpgsign", "false"]);
    let quoted = format!(
        "'{}' git-driver %O %A %B %L",
        binary().replace('\'', "'\\''")
    );
    success(dir, &["config", "merge.jsonc.driver", &quoted]);
    fs::write(dir.join(".gitattributes"), "*.jsonc merge=jsonc\n").unwrap();
    fs::write(dir.join("config.jsonc"), r#"{"a":1,"b":1}"#).unwrap();
    success(dir, &["add", "."]);
    success(dir, &["commit", "-m", "base"]);
}

#[test]
fn real_git_merge_uses_driver_for_clean_and_conflicted_results() {
    for conflict in [false, true] {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();
        setup(dir);
        success(dir, &["checkout", "-b", "theirs"]);
        fs::write(
            dir.join("config.jsonc"),
            if conflict {
                r#"{"a":3,"b":1}"#
            } else {
                r#"{"a":1,"b":2}"#
            },
        )
        .unwrap();
        success(dir, &["commit", "-am", "theirs"]);
        success(dir, &["checkout", "main"]);
        fs::write(dir.join("config.jsonc"), r#"{"a":2,"b":1}"#).unwrap();
        success(dir, &["commit", "-am", "ours"]);
        let result = git(dir, &["merge", "--no-edit", "theirs"]);
        let text = fs::read_to_string(dir.join("config.jsonc")).unwrap();
        if conflict {
            assert!(!result.status.success());
            assert!(text.contains("||||||| BASE"));
            assert!(
                String::from_utf8(git(dir, &["status", "--porcelain"]).stdout)
                    .unwrap()
                    .contains("UU config.jsonc")
            );
        } else {
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&text).unwrap(),
                serde_json::json!({"a":2,"b":2})
            );
        }
    }
}
