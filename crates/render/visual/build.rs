//! Conservative numerical identity for local, content-addressed metric reuse.
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

/// Prefer relative Cargo watch directives; only logical labels and contents enter the hash.
fn watch(path: &Path, manifest: &Path) {
    let relative = manifest.ancestors().enumerate().find_map(|(up, base)| {
        let suffix = path.strip_prefix(base).ok()?;
        let mut relative = PathBuf::new();
        for _ in 0..up {
            relative.push("..");
        }
        Some(relative.join(suffix))
    });
    println!("cargo:rerun-if-changed={}", relative.as_deref().unwrap_or(path).display());
}

fn cargo_configs(add: &mut impl FnMut(&[u8])) -> Result<(), Box<dyn std::error::Error>> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("missing manifest directory")?);
    let mut dirs = Vec::new();
    for (depth, ancestor) in manifest.ancestors().enumerate() {
        if depth >= 64 {
            return Err("Cargo configuration ancestry exceeds the supported bound".into());
        }
        dirs.push((format!("ancestor-{depth}"), ancestor.join(".cargo")));
    }
    println!("cargo:rerun-if-env-changed=CARGO_HOME");
    println!("cargo:rerun-if-env-changed=HOME");
    let home = env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| env::var_os("HOME").map(|p| PathBuf::from(p).join(".cargo")));
    add(b"cargo-home-available");
    add(&[u8::from(home.is_some())]);
    if let Some(home) = home {
        let home = if home.is_absolute() { home } else { manifest.join(home) };
        dirs.push(("cargo-home".into(), home));
    }
    let mut watched = std::collections::BTreeSet::new();
    for (label, dir) in dirs {
        for name in ["config", "config.toml"] {
            add(format!("{label}/{name}").as_bytes());
            let path = dir.join(name);
            match fs::read(&path) {
                Ok(bytes) => {
                    add(b"present");
                    add(&bytes);
                    if watched.insert(path.clone()) {
                        watch(&path, &manifest);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    add(b"absent");
                    // Cargo discovers newly created configuration files and directories.
                    // Do not watch directories: Cargo home's registry and global cache are
                    // updated by unrelated builds and would repeatedly invalidate this crate.
                }
                Err(_) => return Err("could not read Cargo configuration for metric identity".into()),
            }
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut hash = Sha256::new();
    let mut add = |bytes: &[u8]| {
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    };
    for path in ["Cargo.toml", "../../../Cargo.toml", "build.rs", "src/compare.rs", "src/cache.rs", "src/cache/record.rs"] {
        println!("cargo:rerun-if-changed={path}");
        add(path.as_bytes());
        add(&fs::read(path)?);
    }
    cargo_configs(&mut add)?;
    let compiler = env::var_os("RUSTC").ok_or("RUSTC is not set")?;
    let version = Command::new(compiler).arg("-vV").output()?;
    if !version.status.success() {
        return Err("could not determine compiler version".into());
    }
    add(&version.stdout);
    for key in [
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "TARGET",
        "HOST",
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
        "RUSTFLAGS",
    ] {
        println!("cargo:rerun-if-env-changed={key}");
        add(key.as_bytes());
        add(env::var_os(key).unwrap_or_default().to_string_lossy().as_bytes());
    }
    // Profile settings are normally in the workspace manifest. Also watch overrides
    // explicitly, including currently absent variables, so adding one invalidates reuse.
    let fields = [
        "OPT_LEVEL",
        "DEBUG",
        "SPLIT_DEBUGINFO",
        "STRIP",
        "DEBUG_ASSERTIONS",
        "OVERFLOW_CHECKS",
        "LTO",
        "PANIC",
        "INCREMENTAL",
        "CODEGEN_UNITS",
        "RPATH",
    ];
    let mut overrides = std::collections::BTreeSet::new();
    for profile in ["DEV", "RELEASE", "TEST", "BENCH"] {
        for field in fields {
            overrides.insert(format!("CARGO_PROFILE_{profile}_{field}"));
            overrides.insert(format!("CARGO_PROFILE_{profile}_BUILD_OVERRIDE_{field}"));
        }
    }
    // Retain custom-profile overrides supplied by the caller too. Cargo tracks profile
    // changes; known-profile watches additionally detect environment-only additions.
    overrides.extend(env::vars_os().filter_map(|(key, _)| key.into_string().ok().filter(|key| key.starts_with("CARGO_PROFILE_"))));
    for key in overrides {
        println!("cargo:rerun-if-env-changed={key}");
        add(key.as_bytes());
        add(env::var_os(&key).unwrap_or_default().to_string_lossy().as_bytes());
    }
    // Cargo reruns build scripts when the compilation target/features change.
    let mut config: Vec<_> = env::vars().filter(|(k, _)| k.starts_with("CARGO_CFG_") || k.starts_with("CARGO_FEATURE_")).collect();
    config.sort();
    for (key, value) in config {
        add(key.as_bytes());
        add(value.as_bytes());
    }
    // Only the digest is embedded. Compiler paths and host-specific flags are never emitted.
    println!("cargo:rustc-env=KITION_VISUAL_NUMERICAL_BUILD={:x}", hash.finalize());
    Ok(())
}
