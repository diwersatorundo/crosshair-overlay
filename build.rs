use std::env;
#[cfg(not(windows))]
use std::fmt::Write as _;
#[cfg(not(windows))]
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(not(windows))]
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let version_value = version_value(&version);

    #[cfg(windows)]
    compile_with_winresource(&manifest_dir, version_value);

    #[cfg(not(windows))]
    compile_with_zig(&manifest_dir, &out_dir, &version, version_value);
}

#[cfg(windows)]
fn compile_with_winresource(manifest_dir: &Path, version_value: u64) {
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(
        manifest_dir
            .join("assets/icon.ico")
            .to_str()
            .expect("icon path must be valid UTF-8"),
    );
    resource.set("CompanyName", "Crosshair Overlay");
    resource.set("FileDescription", "Crosshair Overlay");
    resource.set("FileVersion", env!("CARGO_PKG_VERSION"));
    resource.set("InternalName", "crosshair-overlay");
    resource.set("LegalCopyright", "MIT License");
    resource.set("OriginalFilename", "crosshair-overlay.exe");
    resource.set("ProductName", "Crosshair Overlay");
    resource.set("ProductVersion", env!("CARGO_PKG_VERSION"));
    resource.set_version_info(winresource::VersionInfo::FILEVERSION, version_value);
    resource.set_version_info(winresource::VersionInfo::PRODUCTVERSION, version_value);
    resource.set_version_info(winresource::VersionInfo::FILEOS, 0x40004);
    resource.set_version_info(winresource::VersionInfo::FILETYPE, 1);
    resource
        .compile()
        .expect("failed to compile Windows resources");
}

#[cfg(not(windows))]
fn compile_with_zig(manifest_dir: &Path, out_dir: &Path, version: &str, version_value: u64) {
    let icon = manifest_dir
        .join("assets/icon.ico")
        .to_str()
        .expect("icon path must be valid UTF-8")
        .replace('\\', "\\\\");
    let file_version = version_value_text(version_value);
    let html_version = version;

    let mut source = String::new();
    writeln!(source, "#include <windows.h>").unwrap();
    writeln!(source, "1 ICON \"{icon}\"").unwrap();
    writeln!(source, "VS_VERSION_INFO VERSIONINFO").unwrap();
    writeln!(source, " FILEVERSION {}", file_version.replace('.', ",")).unwrap();
    writeln!(source, " PRODUCTVERSION {}", file_version.replace('.', ",")).unwrap();
    writeln!(source, " FILEFLAGSMASK 0x3fL").unwrap();
    writeln!(source, " FILEFLAGS 0x0L").unwrap();
    writeln!(source, " FILEOS 0x40004L").unwrap();
    writeln!(source, " FILETYPE 0x1L").unwrap();
    writeln!(source, " FILESUBTYPE 0x0L").unwrap();
    writeln!(source, "BEGIN").unwrap();
    writeln!(source, "    BLOCK \"StringFileInfo\"").unwrap();
    writeln!(source, "    BEGIN").unwrap();
    writeln!(source, "        BLOCK \"040904B0\"").unwrap();
    writeln!(source, "        BEGIN").unwrap();
    writeln!(
        source,
        "            VALUE \"CompanyName\", \"Crosshair Overlay\\0\""
    )
    .unwrap();
    writeln!(
        source,
        "            VALUE \"FileDescription\", \"Crosshair Overlay\\0\""
    )
    .unwrap();
    writeln!(
        source,
        "            VALUE \"FileVersion\", \"{html_version}\\0\""
    )
    .unwrap();
    writeln!(
        source,
        "            VALUE \"InternalName\", \"crosshair-overlay\\0\""
    )
    .unwrap();
    writeln!(
        source,
        "            VALUE \"LegalCopyright\", \"MIT License\\0\""
    )
    .unwrap();
    writeln!(
        source,
        "            VALUE \"OriginalFilename\", \"crosshair-overlay.exe\\0\""
    )
    .unwrap();
    writeln!(
        source,
        "            VALUE \"ProductName\", \"Crosshair Overlay\\0\""
    )
    .unwrap();
    writeln!(
        source,
        "            VALUE \"ProductVersion\", \"{html_version}\\0\""
    )
    .unwrap();
    writeln!(source, "        END").unwrap();
    writeln!(source, "    END").unwrap();
    writeln!(source, "    BLOCK \"VarFileInfo\"").unwrap();
    writeln!(source, "    BEGIN").unwrap();
    writeln!(source, "        VALUE \"Translation\", 0x409, 1200").unwrap();
    writeln!(source, "    END").unwrap();
    writeln!(source, "END").unwrap();

    let rc_name = "crosshair-overlay.rc";
    let res_name = "crosshair-overlay.res";
    let rc_path = out_dir.join(rc_name);
    let res_path = out_dir.join(res_name);
    fs::write(&rc_path, source).expect("failed to write resource script");

    let zig = env::var("ZIG").unwrap_or_else(|_| "zig".to_string());
    let status = Command::new(zig)
        .current_dir(out_dir)
        .arg("rc")
        .arg("/fo")
        .arg(res_name)
        .arg(rc_name)
        .status()
        .expect("failed to start zig rc; install Zig or set ZIG");
    if !status.success() || !res_path.exists() {
        panic!("zig rc failed to compile Windows resources");
    }
    println!("cargo:rustc-link-arg={}", res_path.display());
}

fn version_value(version: &str) -> u64 {
    let mut parts = [0u64; 4];
    for (index, part) in version
        .split('-')
        .next()
        .unwrap_or("0")
        .split('.')
        .take(4)
        .enumerate()
    {
        parts[index] = part.parse().unwrap_or(0);
    }
    (parts[0] << 48) | (parts[1] << 32) | (parts[2] << 16) | parts[3]
}

#[cfg(not(windows))]
fn version_value_text(value: u64) -> String {
    format!(
        "{}.{}.{}.{}",
        value >> 48,
        (value >> 32) & 0xffff,
        (value >> 16) & 0xffff,
        value & 0xffff
    )
}
