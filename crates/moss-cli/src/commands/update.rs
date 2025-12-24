//! Self-update command for moss CLI.

use std::io::Read;

/// Run the update command
pub fn cmd_update(check_only: bool, json: bool) -> i32 {
    const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
    const GITHUB_REPO: &str = "pterror/moss";

    let client = ureq::agent();

    // Fetch latest release from GitHub API
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let response = match client
        .get(&url)
        .set("User-Agent", "moss-cli")
        .set("Accept", "application/vnd.github+json")
        .call()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to check for updates: {}", e);
            return 1;
        }
    };

    let body: serde_json::Value = match response.into_json() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to parse response: {}", e);
            return 1;
        }
    };

    let latest_version = body["tag_name"]
        .as_str()
        .unwrap_or("unknown")
        .trim_start_matches('v');

    let is_update_available = latest_version != CURRENT_VERSION
        && version_gt(latest_version, CURRENT_VERSION);

    if json && check_only {
        println!(
            "{}",
            serde_json::json!({
                "current_version": CURRENT_VERSION,
                "latest_version": latest_version,
                "update_available": is_update_available
            })
        );
        return 0;
    }

    if !json {
        println!("Current version: {}", CURRENT_VERSION);
        println!("Latest version:  {}", latest_version);
    }

    if !is_update_available {
        if !json {
            println!("You are running the latest version.");
        }
        return 0;
    }

    if check_only {
        if !json {
            println!();
            println!("Update available! Run 'moss update' to install.");
        }
        return 0;
    }

    // Perform the update
    println!();
    println!("Downloading update...");

    let target = get_target_triple();
    let asset_name = get_asset_name(&target);

    // Find the asset URL
    let assets = body["assets"].as_array();
    let asset_url = assets.and_then(|arr| {
        arr.iter()
            .find(|a| a["name"].as_str() == Some(&asset_name))
            .and_then(|a| a["browser_download_url"].as_str())
    });

    let asset_url = match asset_url {
        Some(url) => url,
        None => {
            eprintln!("No binary available for your platform: {}", target);
            eprintln!("Available assets:");
            if let Some(arr) = assets {
                for a in arr {
                    if let Some(name) = a["name"].as_str() {
                        eprintln!("  - {}", name);
                    }
                }
            }
            return 1;
        }
    };

    // Download the archive
    println!("  Downloading {}...", asset_name);
    let archive_response = match client.get(asset_url).call() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to download update: {}", e);
            return 1;
        }
    };

    let mut archive_data = Vec::new();
    if let Err(e) = archive_response.into_reader().read_to_end(&mut archive_data) {
        eprintln!("Failed to read download: {}", e);
        return 1;
    }

    // Download checksums
    let checksum_url = assets.and_then(|arr| {
        arr.iter()
            .find(|a| a["name"].as_str() == Some("SHA256SUMS.txt"))
            .and_then(|a| a["browser_download_url"].as_str())
    });

    if let Some(checksum_url) = checksum_url {
        println!("  Verifying checksum...");
        if let Ok(resp) = client.get(checksum_url).call() {
            if let Ok(checksums) = resp.into_string() {
                let expected = checksums
                    .lines()
                    .find(|line| line.contains(&asset_name))
                    .and_then(|line| line.split_whitespace().next());

                if let Some(expected) = expected {
                    let mut hasher = Sha256::new();
                    hasher.update(&archive_data);
                    let hash = hasher.finalize();
                    let actual: String = hash.iter().map(|b| format!("{:02x}", b)).collect();

                    if actual != expected {
                        eprintln!("Checksum mismatch!");
                        eprintln!("  Expected: {}", expected);
                        eprintln!("  Got:      {}", actual);
                        return 1;
                    }
                }
            }
        }
    }

    // Extract binary from archive
    println!("  Extracting...");
    let binary_data = if asset_name.ends_with(".tar.gz") {
        extract_tar_gz(&archive_data)
    } else if asset_name.ends_with(".zip") {
        extract_zip(&archive_data)
    } else {
        eprintln!("Unknown archive format: {}", asset_name);
        return 1;
    };

    let binary_data = match binary_data {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to extract archive: {}", e);
            return 1;
        }
    };

    // Replace current binary
    println!("  Installing...");
    if let Err(e) = self_replace(&binary_data) {
        eprintln!("Failed to replace binary: {}", e);
        eprintln!("You may need to run with elevated permissions.");
        return 1;
    }

    println!();
    println!("Updated successfully to v{}!", latest_version);
    println!("Restart moss to use the new version.");

    0
}

/// Get the target triple for the current platform
fn get_target_triple() -> String {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "unknown"
    };

    let os = if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        "unknown"
    };

    format!("{}-{}", arch, os)
}

/// Get the expected asset name for a target
fn get_asset_name(target: &str) -> String {
    if target.contains("windows") {
        format!("moss-{}.zip", target)
    } else {
        format!("moss-{}.tar.gz", target)
    }
}

/// Simple SHA256 hasher
struct Sha256 {
    state: [u32; 8],
    buffer: Vec<u8>,
    total_len: u64,
}

impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
            ],
            buffer: Vec::new(),
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.total_len += data.len() as u64;

        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.process_block(&block);
            self.buffer.drain(..64);
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        // Padding
        self.buffer.push(0x80);
        while (self.buffer.len() % 64) != 56 {
            self.buffer.push(0);
        }

        // Length in bits
        let bit_len = self.total_len * 8;
        self.buffer.extend_from_slice(&bit_len.to_be_bytes());

        // Process remaining blocks - clone buffer to avoid borrow conflict
        let buffer = std::mem::take(&mut self.buffer);
        for chunk in buffer.chunks(64) {
            let block: [u8; 64] = chunk.try_into().unwrap();
            self.process_block(&block);
        }

        // Output
        let mut result = [0u8; 32];
        for (i, &val) in self.state.iter().enumerate() {
            result[i * 4..(i + 1) * 4].copy_from_slice(&val.to_be_bytes());
        }
        result
    }

    fn process_block(&mut self, block: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
            0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
            0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
            0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
            0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
            0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
            0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
        ];

        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

/// Extract the moss binary from a tar.gz archive
fn extract_tar_gz(data: &[u8]) -> Result<Vec<u8>, String> {
    let decoder = flate2::read::GzDecoder::new(data);
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?;

        if path.file_name().map(|n| n == "moss").unwrap_or(false) {
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents).map_err(|e| e.to_string())?;
            return Ok(contents);
        }
    }

    Err("moss binary not found in archive".to_string())
}

/// Extract the moss binary from a zip archive
fn extract_zip(data: &[u8]) -> Result<Vec<u8>, String> {
    use std::io::Cursor;

    let reader = Cursor::new(data);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| e.to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();

        if name == "moss.exe" || name == "moss" {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).map_err(|e| e.to_string())?;
            return Ok(contents);
        }
    }

    Err("moss binary not found in archive".to_string())
}

/// Replace the current binary with new data
fn self_replace(new_binary: &[u8]) -> Result<(), String> {
    use std::fs;
    use std::io::Write;

    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;

    // Create temp file in same directory (for atomic rename on same filesystem)
    let temp_path = current_exe.with_extension("new");
    let backup_path = current_exe.with_extension("old");

    // Write new binary to temp file
    let mut temp_file = fs::File::create(&temp_path).map_err(|e| e.to_string())?;
    temp_file.write_all(new_binary).map_err(|e| e.to_string())?;
    temp_file.sync_all().map_err(|e| e.to_string())?;
    drop(temp_file);

    // Set executable permission on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&temp_path, perms).map_err(|e| e.to_string())?;
    }

    // Rename current to backup
    if backup_path.exists() {
        fs::remove_file(&backup_path).ok();
    }
    fs::rename(&current_exe, &backup_path).map_err(|e| format!("backup failed: {}", e))?;

    // Rename new to current
    if let Err(e) = fs::rename(&temp_path, &current_exe) {
        // Try to restore backup
        let _ = fs::rename(&backup_path, &current_exe);
        return Err(format!("install failed: {}", e));
    }

    // Remove backup
    fs::remove_file(&backup_path).ok();

    Ok(())
}

/// Simple version comparison (semver-like)
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.split('-').next()?.parse().ok())
            .collect()
    };

    let va = parse(a);
    let vb = parse(b);

    for (a, b) in va.iter().zip(vb.iter()) {
        match a.cmp(b) {
            std::cmp::Ordering::Greater => return true,
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => continue,
        }
    }
    va.len() > vb.len()
}
