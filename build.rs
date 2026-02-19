use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    let lib_name = match target_os.as_str() {
        "windows" => "pdfium.dll",
        "linux" => "libpdfium.so",
        "macos" => "libpdfium.dylib",
        _ => panic!("Unsupported target OS: {}", target_os),
    };

    let destination = Path::new(&manifest_dir).join(lib_name);

    if !destination.exists() {
        println!(
            "cargo:warning={} not found, attempting to download for {}/{}...",
            lib_name, target_os, target_arch
        );
        setup_pdfium(&manifest_dir, &target_os, &target_arch, lib_name);
    }

    println!("cargo:rerun-if-changed={}", lib_name);
}

fn setup_pdfium(manifest_dir: &str, os: &str, arch: &str, lib_name: &str) {
    let pdfium_os = match os {
        "windows" => "win",
        "linux" => "linux",
        "macos" => "mac",
        _ => os,
    };

    let pdfium_arch = match arch {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        _ => arch,
    };

    let asset_name = format!("pdfium-{}-{}.tgz", pdfium_os, pdfium_arch);
    let repo = "bblanchon/pdfium-binaries";

    download_asset(manifest_dir, repo, &asset_name, os);
    extract_and_install(manifest_dir, &asset_name, lib_name);
    cleanup(manifest_dir, &asset_name);

    println!("cargo:warning=Successfully installed {}", lib_name);
}

fn download_asset(manifest_dir: &str, repo: &str, asset_name: &str, os: &str) {
    let api_url = format!("https://api.github.com/repos/{}/releases/latest", repo);

    if os == "windows" {
        let ps_script = format!(
            r#"
            $ErrorActionPreference = "Stop"
            $response = Invoke-RestMethod -Uri "{api_url}"
            $asset = $response.assets | Where-Object {{ $_.name -eq "{asset_name}" }}
            if (-not $asset) {{ throw "Could not find asset {asset_name}" }}
            $downloadUrl = $asset.browser_download_url
            Invoke-WebRequest -Uri $downloadUrl -OutFile "{asset_name}"
            "#
        );
        let status = Command::new("powershell")
            .args(["-Command", &ps_script])
            .current_dir(manifest_dir)
            .status()
            .expect("Failed to execute PowerShell to download PDFium");
        if !status.success() {
            panic!("Failed to download PDFium via PowerShell");
        }
    } else {
        // Use curl for Linux/macOS
        let curl_script = format!(
            r#"curl -L $(curl -s {} | grep "browser_download_url" | grep "{}" | cut -d '"' -f 4) -o {}"#,
            api_url, asset_name, asset_name
        );
        let status = Command::new("sh")
            .args(["-c", &curl_script])
            .current_dir(manifest_dir)
            .status()
            .expect("Failed to execute curl to download PDFium");
        if !status.success() {
            panic!("Failed to download PDFium via curl");
        }
    }
}

fn extract_and_install(manifest_dir: &str, archive: &str, lib_name: &str) {
    let status = Command::new("tar")
        .args(["-xzf", archive])
        .current_dir(manifest_dir)
        .status()
        .expect("Failed to execute tar for extraction");

    if !status.success() {
        panic!("Failed to extract PDFium archive. Ensure 'tar' is in your PATH.");
    }

    let manifest_path = Path::new(manifest_dir);

    // PDFium binaries structure usually has bin/ (Windows) or lib/ (Linux/macOS)
    let possible_paths = [
        manifest_path.join("bin").join(lib_name),
        manifest_path.join("lib").join(lib_name),
        manifest_path.join(lib_name),
    ];

    let mut found = false;
    for path in possible_paths {
        if path.exists() {
            fs::copy(&path, manifest_path.join(lib_name))
                .expect("Failed to copy library to project root");
            found = true;
            break;
        }
    }

    if !found {
        panic!("Could not find {} in the extracted archive", lib_name);
    }
}

fn cleanup(manifest_dir: &str, archive: &str) {
    let manifest_path = Path::new(manifest_dir);
    let _ = fs::remove_file(manifest_path.join(archive));
    let _ = fs::remove_dir_all(manifest_path.join("bin"));
    let _ = fs::remove_dir_all(manifest_path.join("lib"));
    let _ = fs::remove_dir_all(manifest_path.join("include"));
    let _ = fs::remove_file(manifest_path.join("args.gn"));
    let _ = fs::remove_file(manifest_path.join("LICENSE"));
}
