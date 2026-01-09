//! bun.lock (text) and bun.lockb (binary) parser
//!
//! As of Bun 1.0+, text format (bun.lock) is the default. Binary format (bun.lockb)
//! is supported for backwards compatibility with older projects.
//!
//! The parser prefers bun.lock when both exist. Binary format fallback reads
//! logical dependencies from each package's dependencies/resolutions slices.
//!
//! Binary format ported from Bun (MIT License):
//! Copyright (c) 2022 Oven-sh
//! https://github.com/oven-sh/bun/blob/main/src/install/lockfile/
//!
//! Key source files:
//! - bun.lockb.zig: Main serializer, file format layout
//! - Buffers.zig: Buffer serialization (trees, hoisted_deps, resolutions, dependencies, extern_strings, string_bytes)
//! - Package.zig: Package struct and MultiArrayList serialization
//! - semver/SemverString.zig: String encoding (inline vs external)

use crate::{DependencyTree, PackageError, TreeNode};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Project name and version from package.json or lockfile.
struct ProjectInfo {
    name: String,
    version: String,
}

/// Get installed version from bun.lock or bun.lockb
pub fn installed_version(package: &str, project_root: &Path) -> Option<String> {
    // Try text format first (bun.lock)
    if let Some(v) = installed_version_text(package, project_root) {
        return Some(v);
    }
    // Fall back to binary format
    installed_version_binary(package, project_root)
}

fn installed_version_text(package: &str, project_root: &Path) -> Option<String> {
    let lockfile = find_text_lockfile(project_root)?;
    let content = std::fs::read_to_string(&lockfile).ok()?;
    let parsed: serde_json::Value = serde_json_lenient::from_str(&content).ok()?;

    // packages section: "pkg": ["pkg@version", registry, {deps}, hash]
    if let Some(packages) = parsed.get("packages").and_then(|p| p.as_object())
        && let Some(pkg_info) = packages.get(package)
        && let Some(arr) = pkg_info.as_array()
        && let Some(first) = arr.first().and_then(|v| v.as_str())
        && let Some(version) = extract_version_from_spec(first)
    {
        return Some(version);
    }

    // Also check workspaces for direct deps
    if let Some(workspaces) = parsed.get("workspaces").and_then(|w| w.as_object()) {
        for (_ws_path, ws_info) in workspaces {
            for dep_type in ["dependencies", "devDependencies", "optionalDependencies"] {
                if let Some(deps) = ws_info.get(dep_type).and_then(|d| d.as_object())
                    && deps.contains_key(package)
                    && let Some(packages) = parsed.get("packages").and_then(|p| p.as_object())
                    && let Some(pkg_info) = packages.get(package)
                    && let Some(arr) = pkg_info.as_array()
                    && let Some(first) = arr.first().and_then(|v| v.as_str())
                    && let Some(version) = extract_version_from_spec(first)
                {
                    return Some(version);
                }
            }
        }
    }

    None
}

fn installed_version_binary(package: &str, project_root: &Path) -> Option<String> {
    let lockfile = find_binary_lockfile(project_root)?;
    let data = std::fs::read(&lockfile).ok()?;
    let parsed = BunLockb::parse(&data)?;
    parsed.find_package_version(package)
}

/// Build dependency tree from bun.lock or bun.lockb
pub fn dependency_tree(project_root: &Path) -> Option<Result<DependencyTree, PackageError>> {
    // Try text format first (preferred - has version info)
    if let Some(lockfile) = find_text_lockfile(project_root) {
        let content = std::fs::read_to_string(&lockfile).ok()?;
        let parsed: serde_json::Value = serde_json_lenient::from_str(&content).ok()?;
        return Some(build_tree_text(&parsed, project_root));
    }

    // Fall back to binary format
    if let Some(lockfile) = find_binary_lockfile(project_root) {
        if lockfile.exists() {
            return Some(build_tree_binary(project_root));
        }
    }

    None
}

// ============================================================================
// Binary format parser (bun.lockb)
//
// File format (from bun.lockb.zig):
//   - Header: 42 bytes ("#!/usr/bin/env bun\nbun-lockfile-format-v0\n")
//   - Format version: u32 LE
//   - Meta hash: 32 bytes
//   - Total buffer size: u64 LE
//   - Packages MultiArrayList (header + data)
//   - Buffers (6 sequential): trees, hoisted_deps, resolutions, dependencies, extern_strings, string_bytes
//   - Zero marker: u64 = 0
//
// Buffer format (from Buffers.zig):
//   - start_pos: u64 LE (absolute file position where data starts)
//   - end_pos: u64 LE (absolute file position where data ends)
//   - Type description string + alignment padding
//   - Data bytes at [start_pos..end_pos]
//   - Next buffer header starts at end_pos
//
// String encoding (from SemverString.zig):
//   - 8 bytes total
//   - Inline if bytes[7] & 0x80 == 0: null-terminated string in bytes[0..7]
//   - External if bytes[7] & 0x80 != 0: Pointer { off: u32, len: u32 } via bitcast
//     - ptr() truncates to u63 then bitcasts: off = low 32 bits, len = next 32 bits
// ============================================================================

/// Header magic for bun.lockb files
const HEADER_MAGIC: &[u8] = b"#!/usr/bin/env bun\nbun-lockfile-format-v0\n";

/// Parsed Tree entry from bun.lockb
/// Trees represent physical node_modules layout (hoisted), not logical deps.
/// We now use packages' dependencies/resolutions for logical tree building.
/// Trees are kept for potential future use (physical layout visualization).
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct BunTree {
    id: u32,
    dependency_id: u32,
    parent: u32,
    deps_off: u32,
    deps_len: u32,
}

/// Parsed bun.lockb file
struct BunLockb<'a> {
    packages: Vec<BunPackage>,
    #[allow(dead_code)]
    trees: Vec<BunTree>,
    #[allow(dead_code)]
    hoisted_deps: &'a [u8], // DependencyID[] (u32 each) - for physical layout
    resolutions_buf: &'a [u8], // PackageID[] (u32 each) - resolved package indices
    dependencies: &'a [u8],    // Dependency.External[] (name at offset 0, 8 bytes)
    string_bytes: &'a [u8],
}

/// Slice reference {off: u32, len: u32}
#[derive(Debug, Clone, Copy, Default)]
struct Slice {
    off: u32,
    len: u32,
}

#[derive(Debug, Clone)]
struct BunPackage {
    name: String,
    version: String,
    /// Slice into dependencies buffer (Dependency.External array)
    dependencies: Slice,
    /// Slice into resolutions buffer (PackageID array)
    /// Not currently used - deps indices work for both. Kept for future version parsing.
    #[allow(dead_code)]
    resolutions: Slice,
}

impl<'a> BunLockb<'a> {
    fn parse(data: &'a [u8]) -> Option<Self> {
        // Validate header
        if data.len() < HEADER_MAGIC.len() + 100 {
            return None;
        }
        if !data.starts_with(HEADER_MAGIC) {
            return None;
        }

        let mut offset = HEADER_MAGIC.len(); // 42

        // Format version (u32 LE)
        // v0-v2: Resolution uses u32 version (64 bytes)
        // v3+: Resolution uses u64 version (72 bytes)
        let format_version = read_u32_le(data, &mut offset)?;
        if format_version > 10 {
            return None;
        }

        // Meta hash (32 bytes)
        offset += 32;

        // Total buffer size (u64 LE)
        offset += 8;

        // Packages MultiArrayList header (from Package.zig Serializer.save):
        //   list_len: u64, alignment: u64, field_count: u64, begin_at: u64, end_at: u64
        let packages_count = read_u64_le(data, &mut offset)? as usize;
        let _alignment = read_u64_le(data, &mut offset)?;
        let _field_count = read_u64_le(data, &mut offset)?;
        let pkg_begin = read_u64_le(data, &mut offset)? as usize;
        let pkg_end = read_u64_le(data, &mut offset)? as usize;

        if packages_count == 0 || packages_count > 100_000 {
            return None;
        }
        if pkg_begin >= pkg_end || pkg_end > data.len() {
            return None;
        }

        // Buffers start at pkg_end (sorted by alignment):
        // 0: trees, 1: hoisted_deps, 2: resolutions, 3: dependencies, 4: extern_strings, 5: string_bytes
        let trees_buf = Self::find_buffer(data, pkg_end, 0)?;
        let hoisted_deps = Self::find_buffer(data, pkg_end, 1)?;
        let resolutions_buf = Self::find_buffer(data, pkg_end, 2)?;
        let dependencies = Self::find_buffer(data, pkg_end, 3)?;
        let string_bytes = Self::find_buffer(data, pkg_end, 5)?;

        // Parse trees (20 bytes each: id, dep_id, parent, deps.off, deps.len)
        let trees = Self::parse_trees(trees_buf);

        // Package MultiArrayList field offsets (fields stored as arrays, sorted by alignment):
        // - names:        offset 0,              each 8 bytes (String)
        // - name_hashes:  offset pkg_count * 8,  each 8 bytes (u64)
        // - resolution:   offset pkg_count * 16, each (v2: 64, v3+: 72) bytes
        // - dependencies: after resolutions, each 8 bytes (DependencySlice)
        // - resolutions:  after dependencies, each 8 bytes (PackageIDSlice)
        let resolution_size = if format_version <= 2 { 64 } else { 72 };
        let packages = Self::extract_packages(
            data,
            pkg_begin,
            packages_count,
            string_bytes,
            resolution_size,
            format_version,
        );

        Some(Self {
            packages,
            trees,
            hoisted_deps,
            resolutions_buf,
            dependencies,
            string_bytes,
        })
    }

    /// Parse trees buffer into BunTree structs
    fn parse_trees(trees_buf: &[u8]) -> Vec<BunTree> {
        const TREE_SIZE: usize = 20;
        let count = trees_buf.len() / TREE_SIZE;
        let mut trees = Vec::with_capacity(count);

        for i in 0..count {
            let off = i * TREE_SIZE;
            if off + TREE_SIZE > trees_buf.len() {
                break;
            }
            trees.push(BunTree {
                id: u32::from_le_bytes(trees_buf[off..off + 4].try_into().unwrap_or([0; 4])),
                dependency_id: u32::from_le_bytes(
                    trees_buf[off + 4..off + 8].try_into().unwrap_or([0; 4]),
                ),
                parent: u32::from_le_bytes(
                    trees_buf[off + 8..off + 12].try_into().unwrap_or([0; 4]),
                ),
                deps_off: u32::from_le_bytes(
                    trees_buf[off + 12..off + 16].try_into().unwrap_or([0; 4]),
                ),
                deps_len: u32::from_le_bytes(
                    trees_buf[off + 16..off + 20].try_into().unwrap_or([0; 4]),
                ),
            });
        }

        trees
    }

    /// Find buffer by index (0-indexed). Buffers are sequential, each header at previous end_pos.
    fn find_buffer(data: &[u8], buffers_start: usize, buffer_index: usize) -> Option<&[u8]> {
        let mut offset = buffers_start;

        for i in 0..=buffer_index {
            if offset + 16 > data.len() {
                return None;
            }

            let start_pos = u64::from_le_bytes(data[offset..offset + 8].try_into().ok()?) as usize;
            let end_pos =
                u64::from_le_bytes(data[offset + 8..offset + 16].try_into().ok()?) as usize;

            // Validate - 0xDEADBEEF means placeholder wasn't patched
            if start_pos == 0xDEADBEEF || end_pos == 0xDEADBEEF {
                return None;
            }
            if start_pos > end_pos || end_pos > data.len() {
                return None;
            }

            if i == buffer_index {
                return Some(&data[start_pos..end_pos]);
            }

            // Next buffer's header starts at this buffer's end_pos
            offset = end_pos;
        }

        None
    }

    /// Extract packages using proper String encoding and slice references
    ///
    /// MultiArrayList layout (fields stored in declaration order):
    /// - names[count]:        8 bytes each (String)
    /// - name_hashes[count]:  8 bytes each (u64)
    /// - resolution[count]:   (v2: 64, v3+: 72) bytes each (Resolution)
    /// - dependencies[count]: 8 bytes each (DependencySlice = {off: u32, len: u32})
    /// - resolutions[count]:  8 bytes each (PackageIDSlice = {off: u32, len: u32})
    /// - meta[count], bin[count], scripts[count]: remaining fields
    fn extract_packages(
        data: &[u8],
        pkg_begin: usize,
        count: usize,
        string_bytes: &[u8],
        resolution_size: usize,
        format_version: u32,
    ) -> Vec<BunPackage> {
        let mut packages = Vec::with_capacity(count);

        // Field offsets (from pkg_begin):
        // - names: 0
        // - name_hashes: count * 8
        // - resolution: count * 16
        // - dependencies: count * (16 + resolution_size)
        // - resolutions: count * (16 + resolution_size + 8)
        let names_off = 0;
        let resolution_off = count * 16;
        let deps_off = count * (16 + resolution_size);
        let res_off = count * (16 + resolution_size + 8);

        for i in 0..count {
            // Read name
            let name_offset = pkg_begin + names_off + i * 8;
            if name_offset + 8 > data.len() {
                break;
            }

            let name_bytes: [u8; 8] = match data[name_offset..name_offset + 8].try_into() {
                Ok(b) => b,
                Err(_) => break,
            };

            let name = match Self::read_string(&name_bytes, string_bytes) {
                Some(n) if Self::is_valid_package_name(&n) => n,
                _ => continue,
            };

            // Read dependencies slice {off: u32, len: u32}
            let dep_slice_off = pkg_begin + deps_off + i * 8;
            let dependencies = if dep_slice_off + 8 <= data.len() {
                Slice {
                    off: u32::from_le_bytes(
                        data[dep_slice_off..dep_slice_off + 4]
                            .try_into()
                            .unwrap_or([0; 4]),
                    ),
                    len: u32::from_le_bytes(
                        data[dep_slice_off + 4..dep_slice_off + 8]
                            .try_into()
                            .unwrap_or([0; 4]),
                    ),
                }
            } else {
                Slice::default()
            };

            // Read resolutions slice (PackageID[]) - not used (deps indices work for both)
            // Kept for potential future version parsing from Resolution field
            let res_slice_off = pkg_begin + res_off + i * 8;
            let resolutions = if res_slice_off + 8 <= data.len() {
                Slice {
                    off: u32::from_le_bytes(
                        data[res_slice_off..res_slice_off + 4]
                            .try_into()
                            .unwrap_or([0; 4]),
                    ),
                    len: u32::from_le_bytes(
                        data[res_slice_off + 4..res_slice_off + 8]
                            .try_into()
                            .unwrap_or([0; 4]),
                    ),
                }
            } else {
                Slice::default()
            };

            // Read version from Resolution field
            // v2: 64 bytes, v3+: 72 bytes
            // Layout: tag(1) + padding(7) + value(56 or 64)
            let res_addr = pkg_begin + resolution_off + i * resolution_size;
            let version = Self::read_resolution_version(data, res_addr, format_version);

            packages.push(BunPackage {
                name,
                version,
                dependencies,
                resolutions,
            });
        }

        packages
    }

    /// Read version from Resolution struct
    /// v2: 64 bytes (tag(1) + padding(7) + VersionedURL(56))
    /// v3+: 72 bytes (tag(1) + padding(7) + VersionedURL(64))
    /// VersionedURL: url(8) + Version
    /// v2 Version: major(u32) + minor(u32) + patch(u32) + padding(4) + tag(32) = 48 bytes
    /// v3 Version: major(u64) + minor(u64) + patch(u64) + tag(32) = 56 bytes
    fn read_resolution_version(data: &[u8], res_offset: usize, format_version: u32) -> String {
        const TAG_NPM: u8 = 2;
        const TAG_ROOT: u8 = 1;
        const TAG_WORKSPACE: u8 = 72;

        let res_size = if format_version <= 2 { 64 } else { 72 };
        if res_offset + res_size > data.len() {
            return "?".to_string();
        }

        let tag = data[res_offset];

        match tag {
            TAG_NPM => {
                // npm: version at offset 16 (tag+padding=8, url=8)
                let ver_offset = res_offset + 16;

                if format_version <= 2 {
                    // v2: u32 version fields
                    if ver_offset + 12 > data.len() {
                        return "?".to_string();
                    }
                    let major = u32::from_le_bytes(
                        data[ver_offset..ver_offset + 4]
                            .try_into()
                            .unwrap_or([0; 4]),
                    );
                    let minor = u32::from_le_bytes(
                        data[ver_offset + 4..ver_offset + 8]
                            .try_into()
                            .unwrap_or([0; 4]),
                    );
                    let patch = u32::from_le_bytes(
                        data[ver_offset + 8..ver_offset + 12]
                            .try_into()
                            .unwrap_or([0; 4]),
                    );
                    format!("{}.{}.{}", major, minor, patch)
                } else {
                    // v3+: u64 version fields
                    if ver_offset + 24 > data.len() {
                        return "?".to_string();
                    }
                    let major = u64::from_le_bytes(
                        data[ver_offset..ver_offset + 8]
                            .try_into()
                            .unwrap_or([0; 8]),
                    );
                    let minor = u64::from_le_bytes(
                        data[ver_offset + 8..ver_offset + 16]
                            .try_into()
                            .unwrap_or([0; 8]),
                    );
                    let patch = u64::from_le_bytes(
                        data[ver_offset + 16..ver_offset + 24]
                            .try_into()
                            .unwrap_or([0; 8]),
                    );
                    format!("{}.{}.{}", major, minor, patch)
                }
            }
            TAG_ROOT | TAG_WORKSPACE => "0.0.0".to_string(),
            _ => "0.0.0".to_string(),
        }
    }

    /// Read a String value (from SemverString.zig)
    fn read_string(bytes: &[u8; 8], string_bytes: &[u8]) -> Option<String> {
        // isInline: bytes[7] & 0x80 == 0
        if bytes[7] & 0x80 == 0 {
            // Inline string: scan for null byte in bytes[0..8]
            let end_pos = bytes.iter().position(|&b| b == 0).unwrap_or(8);
            if end_pos == 0 {
                return None; // Empty string
            }
            std::str::from_utf8(&bytes[..end_pos])
                .ok()
                .map(|s| s.to_string())
        } else {
            // External string: Pointer { off: u32, len: u32 }
            // ptr() method: @as(Pointer, @bitCast(@as(u64, @as(u63, @truncate(@as(u64, @bitCast(this)))))))
            // This truncates to 63 bits (clears bit 63), then bitcasts to Pointer
            let raw = u64::from_le_bytes(*bytes);
            let truncated = raw & 0x7FFF_FFFF_FFFF_FFFF; // Clear high bit (u63 truncation)
            let off = (truncated & 0xFFFF_FFFF) as usize;
            let len = ((truncated >> 32) & 0xFFFF_FFFF) as usize;

            if len > 0 && off + len <= string_bytes.len() {
                std::str::from_utf8(&string_bytes[off..off + len])
                    .ok()
                    .map(|s| s.to_string())
            } else {
                None
            }
        }
    }

    fn is_valid_package_name(name: &str) -> bool {
        !name.is_empty()
            && name.len() <= 214
            && name.chars().all(|c| {
                c.is_ascii_alphanumeric()
                    || c == '-'
                    || c == '_'
                    || c == '@'
                    || c == '/'
                    || c == '.'
            })
    }

    fn find_package_version(&self, package: &str) -> Option<String> {
        self.packages
            .iter()
            .find(|p| p.name == package)
            .map(|p| p.version.clone())
    }

    /// Get dependency name from dependencies buffer by index
    /// Dependency.External layout: name (8 bytes), name_hash (8), behavior (1), version (9) = 26 bytes
    fn get_dep_name(&self, dep_id: u32) -> Option<String> {
        const DEP_SIZE: usize = 26;
        let offset = dep_id as usize * DEP_SIZE;
        if offset + 8 > self.dependencies.len() {
            return None;
        }
        let name_bytes: [u8; 8] = self.dependencies[offset..offset + 8].try_into().ok()?;
        Self::read_string(&name_bytes, self.string_bytes)
    }

    /// Get hoisted dependency IDs for a tree node (physical layout)
    #[allow(dead_code)]
    fn get_hoisted_deps(&self, tree: &BunTree) -> Vec<u32> {
        let start = tree.deps_off as usize * 4; // u32 = 4 bytes
        let end = start + tree.deps_len as usize * 4;
        if end > self.hoisted_deps.len() {
            return Vec::new();
        }

        (0..tree.deps_len as usize)
            .filter_map(|i| {
                let off = start + i * 4;
                if off + 4 <= self.hoisted_deps.len() {
                    Some(u32::from_le_bytes(
                        self.hoisted_deps[off..off + 4].try_into().ok()?,
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Build a map of package name -> (version, list of dep names) from logical dependencies
    fn build_deps_map(&self) -> HashMap<String, (String, Vec<String>)> {
        let mut deps_map = HashMap::new();

        for pkg in self.packages.iter() {
            let mut dep_names = Vec::new();

            // For each dependency in this package's dependencies slice:
            // - Read dep name from dependencies buffer
            // - Read resolved PackageID from resolutions buffer (same indices as deps)
            // - Look up package name by PackageID
            //
            // Note: dependencies and resolutions are parallel arrays with matching indices.
            // We use dependencies.off for both since resolutions slice may not be stored
            // in older lockfiles (the Package.resolutions field had garbage values).
            for j in 0..pkg.dependencies.len as usize {
                // Get dependency name from dependencies buffer
                if let Some(dep_name) = self.get_dep_name(pkg.dependencies.off + j as u32) {
                    // Get resolved PackageID from resolutions buffer
                    // Use dependencies offset since deps and resolutions are parallel
                    let res_idx = pkg.dependencies.off as usize + j;
                    let res_offset = res_idx * 4; // PackageID is u32
                    if res_offset + 4 <= self.resolutions_buf.len() {
                        let resolved_pkg_id = u32::from_le_bytes(
                            self.resolutions_buf[res_offset..res_offset + 4]
                                .try_into()
                                .unwrap_or([0; 4]),
                        );

                        // Get package name by ID
                        if let Some(resolved_pkg) = self.packages.get(resolved_pkg_id as usize) {
                            dep_names.push(resolved_pkg.name.clone());
                        } else {
                            // Fallback to dependency name if resolution not found
                            dep_names.push(dep_name);
                        }
                    }
                }
            }

            deps_map.insert(pkg.name.clone(), (pkg.version.clone(), dep_names));
        }

        deps_map
    }

    /// Recursively build a TreeNode using logical dependencies
    fn build_node(
        &self,
        name: &str,
        deps_map: &HashMap<String, (String, Vec<String>)>,
        visited: &mut HashSet<String>,
        max_depth: usize,
    ) -> TreeNode {
        let version = deps_map
            .get(name)
            .map(|(v, _)| v.clone())
            .unwrap_or_else(|| "?".to_string());

        if visited.contains(name) || max_depth == 0 {
            return TreeNode {
                name: name.to_string(),
                version,
                dependencies: Vec::new(),
            };
        }

        visited.insert(name.to_string());

        let children = if let Some((_, deps)) = deps_map.get(name) {
            deps.iter()
                .map(|dep| self.build_node(dep, deps_map, visited, max_depth - 1))
                .collect()
        } else {
            Vec::new()
        };

        visited.remove(name);

        TreeNode {
            name: name.to_string(),
            version,
            dependencies: children,
        }
    }

    fn to_tree(&self, project_root: &Path) -> DependencyTree {
        let proj_info = get_project_info_from_package_json(project_root);

        // Build logical dependency map from packages
        let deps_map = self.build_deps_map();

        // Get direct dependencies from root package (package 0 is usually the root)
        let mut visited = HashSet::new();
        const MAX_DEPTH: usize = 50;

        let root_deps = if let Some(root_pkg) = self.packages.first() {
            if let Some((_, direct_deps)) = deps_map.get(&root_pkg.name) {
                direct_deps
                    .iter()
                    .map(|dep| self.build_node(dep, &deps_map, &mut visited, MAX_DEPTH))
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            // Fallback: flat list from packages (shouldn't happen)
            self.packages
                .iter()
                .skip(1) // Skip root
                .map(|p| TreeNode {
                    name: p.name.clone(),
                    version: p.version.clone(),
                    dependencies: Vec::new(),
                })
                .collect()
        };

        DependencyTree {
            roots: vec![TreeNode {
                name: proj_info.name,
                version: proj_info.version,
                dependencies: root_deps,
            }],
        }
    }
}

fn read_u32_le(data: &[u8], offset: &mut usize) -> Option<u32> {
    if *offset + 4 > data.len() {
        return None;
    }
    let bytes: [u8; 4] = data[*offset..*offset + 4].try_into().ok()?;
    *offset += 4;
    Some(u32::from_le_bytes(bytes))
}

fn read_u64_le(data: &[u8], offset: &mut usize) -> Option<u64> {
    if *offset + 8 > data.len() {
        return None;
    }
    let bytes: [u8; 8] = data[*offset..*offset + 8].try_into().ok()?;
    *offset += 8;
    Some(u64::from_le_bytes(bytes))
}

// ============================================================================
// Text format parser (bun.lock)
// ============================================================================

fn find_text_lockfile(project_root: &Path) -> Option<std::path::PathBuf> {
    let mut current = project_root.to_path_buf();
    loop {
        let lockfile = current.join("bun.lock");
        if lockfile.exists() {
            return Some(lockfile);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn find_binary_lockfile(project_root: &Path) -> Option<std::path::PathBuf> {
    let mut current = project_root.to_path_buf();
    loop {
        let lockfile = current.join("bun.lockb");
        if lockfile.exists() {
            return Some(lockfile);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn extract_version_from_spec(spec: &str) -> Option<String> {
    // Handle "@scope/pkg@version" or "pkg@version"
    if spec.starts_with('@') {
        let first_slash = spec.find('/')?;
        let version_at = spec[first_slash..].find('@').map(|i| i + first_slash)?;
        Some(spec[version_at + 1..].to_string())
    } else {
        let at_pos = spec.find('@')?;
        Some(spec[at_pos + 1..].to_string())
    }
}

/// Build a map of package name -> (version, deps) from bun.lock packages
fn build_deps_map_text(parsed: &serde_json::Value) -> HashMap<String, (String, Vec<String>)> {
    let mut deps_map = HashMap::new();

    if let Some(packages) = parsed.get("packages").and_then(|p| p.as_object()) {
        for (pkg_name, pkg_info) in packages {
            if let Some(arr) = pkg_info.as_array() {
                // arr[0] = "pkg@version", arr[2] = { dependencies: {...} }
                let version = arr
                    .first()
                    .and_then(|v| v.as_str())
                    .and_then(extract_version_from_spec)
                    .unwrap_or_else(|| "?".to_string());

                let mut deps = Vec::new();
                if let Some(dep_info) = arr.get(2).and_then(|d| d.as_object()) {
                    for dep_type in ["dependencies", "optionalDependencies"] {
                        if let Some(dep_map) = dep_info.get(dep_type).and_then(|d| d.as_object()) {
                            for dep_name in dep_map.keys() {
                                deps.push(dep_name.clone());
                            }
                        }
                    }
                }

                deps_map.insert(pkg_name.clone(), (version, deps));
            }
        }
    }

    deps_map
}

/// Recursively build a TreeNode for bun.lock text format
fn build_node_text(
    name: &str,
    deps_map: &HashMap<String, (String, Vec<String>)>,
    visited: &mut HashSet<String>,
    max_depth: usize,
) -> TreeNode {
    // Get version from deps map
    let version = deps_map
        .get(name)
        .map(|(v, _)| v.clone())
        .unwrap_or_else(|| "?".to_string());

    // Avoid cycles and limit depth
    if visited.contains(name) || max_depth == 0 {
        return TreeNode {
            name: name.to_string(),
            version,
            dependencies: Vec::new(),
        };
    }

    visited.insert(name.to_string());

    let children = if let Some((_, deps)) = deps_map.get(name) {
        deps.iter()
            .map(|dep| build_node_text(dep, deps_map, visited, max_depth - 1))
            .collect()
    } else {
        Vec::new()
    };

    visited.remove(name);

    TreeNode {
        name: name.to_string(),
        version,
        dependencies: children,
    }
}

fn build_tree_text(
    parsed: &serde_json::Value,
    project_root: &Path,
) -> Result<DependencyTree, PackageError> {
    let info = get_project_info(parsed, project_root);

    // Build the dependency map from packages
    let deps_map = build_deps_map_text(parsed);

    let mut root_deps = Vec::new();
    let mut visited = HashSet::new();
    const MAX_DEPTH: usize = 50;

    // Get direct dependencies from root workspace
    if let Some(workspaces) = parsed.get("workspaces").and_then(|w| w.as_object()) {
        if let Some(root_ws) = workspaces.get("") {
            for dep_type in ["dependencies", "devDependencies"] {
                if let Some(deps) = root_ws.get(dep_type).and_then(|d| d.as_object()) {
                    for dep_name in deps.keys() {
                        root_deps.push(build_node_text(
                            dep_name,
                            &deps_map,
                            &mut visited,
                            MAX_DEPTH,
                        ));
                    }
                }
            }
        }

        // Also add workspace packages (without recursing into their deps)
        for (ws_path, ws_info) in workspaces {
            if ws_path.is_empty() {
                continue;
            }
            if let Some(ws_name) = ws_info.get("name").and_then(|n| n.as_str()) {
                let ws_version = ws_info
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0");
                root_deps.push(TreeNode {
                    name: ws_name.to_string(),
                    version: ws_version.to_string(),
                    dependencies: Vec::new(),
                });
            }
        }
    }

    let root = TreeNode {
        name: info.name,
        version: info.version,
        dependencies: root_deps,
    };

    Ok(DependencyTree { roots: vec![root] })
}

fn build_tree_binary(project_root: &Path) -> Result<DependencyTree, PackageError> {
    let lockfile = find_binary_lockfile(project_root)
        .ok_or_else(|| PackageError::ParseError("bun.lockb not found".to_string()))?;

    let data = std::fs::read(&lockfile)
        .map_err(|e| PackageError::ParseError(format!("failed to read bun.lockb: {}", e)))?;

    let parsed = BunLockb::parse(&data)
        .ok_or_else(|| PackageError::ParseError("invalid bun.lockb format".to_string()))?;

    Ok(parsed.to_tree(project_root))
}

fn get_project_info(parsed: &serde_json::Value, project_root: &Path) -> ProjectInfo {
    if let Some(workspaces) = parsed.get("workspaces").and_then(|w| w.as_object())
        && let Some(root_ws) = workspaces.get("")
    {
        let name = root_ws
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("root");
        let version = root_ws
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0");
        return ProjectInfo {
            name: name.to_string(),
            version: version.to_string(),
        };
    }
    get_project_info_from_package_json(project_root)
}

fn get_project_info_from_package_json(project_root: &Path) -> ProjectInfo {
    let pkg_json = project_root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_json)
        && let Ok(pkg) = serde_json::from_str::<serde_json::Value>(&content)
    {
        let name = pkg
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("root")
            .to_string();
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();
        return ProjectInfo { name, version };
    }
    ProjectInfo {
        name: "root".to_string(),
        version: "0.0.0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Text format (bun.lock) tests ==========

    #[test]
    fn test_extract_version_simple() {
        assert_eq!(
            extract_version_from_spec("react@18.2.0"),
            Some("18.2.0".to_string())
        );
    }

    #[test]
    fn test_extract_version_scoped() {
        assert_eq!(
            extract_version_from_spec("@types/node@20.0.0"),
            Some("20.0.0".to_string())
        );
    }

    #[test]
    fn test_build_deps_map_text() {
        // bun.lock uses JSONC format (JSON with comments and trailing commas)
        let json = r#"{
            "lockfileVersion": 1,
            "workspaces": {
                "": {
                    "name": "my-app",
                    "dependencies": {
                        "react": "^18.0.0"
                    }
                }
            },
            "packages": {
                "react": ["react@18.2.0", "", {
                    "dependencies": {
                        "loose-envify": "^1.1.0"
                    }
                }],
                "loose-envify": ["loose-envify@1.4.0", "", {
                    "dependencies": {
                        "js-tokens": "^4.0.0"
                    }
                }],
                "js-tokens": ["js-tokens@4.0.0", "", {}],
                "@types/node": ["@types/node@20.11.0", "", {
                    "dependencies": {
                        "undici-types": "~5.26.0"
                    }
                }],
                "undici-types": ["undici-types@5.26.5", "", {}]
            }
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let deps_map = build_deps_map_text(&parsed);

        // Check react
        assert!(deps_map.contains_key("react"));
        let (version, deps) = deps_map.get("react").unwrap();
        assert_eq!(version, "18.2.0");
        assert!(deps.contains(&"loose-envify".to_string()));

        // Check transitive deps
        let (version, deps) = deps_map.get("loose-envify").unwrap();
        assert_eq!(version, "1.4.0");
        assert!(deps.contains(&"js-tokens".to_string()));

        // Check scoped package
        let (version, deps) = deps_map.get("@types/node").unwrap();
        assert_eq!(version, "20.11.0");
        assert!(deps.contains(&"undici-types".to_string()));

        // Check leaf package
        let (version, deps) = deps_map.get("js-tokens").unwrap();
        assert_eq!(version, "4.0.0");
        assert!(deps.is_empty());
    }

    #[test]
    fn test_build_deps_map_text_with_optional() {
        let json = r#"{
            "packages": {
                "esbuild": ["esbuild@0.21.0", "", {
                    "optionalDependencies": {
                        "@esbuild/linux-x64": "0.21.0",
                        "@esbuild/darwin-arm64": "0.21.0"
                    }
                }],
                "@esbuild/linux-x64": ["@esbuild/linux-x64@0.21.0", "", {}],
                "@esbuild/darwin-arm64": ["@esbuild/darwin-arm64@0.21.0", "", {}]
            }
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let deps_map = build_deps_map_text(&parsed);

        let (_, deps) = deps_map.get("esbuild").unwrap();
        assert_eq!(deps.len(), 2);
        assert!(deps.contains(&"@esbuild/linux-x64".to_string()));
        assert!(deps.contains(&"@esbuild/darwin-arm64".to_string()));
    }

    // ========== Binary format (bun.lockb) tests ==========

    #[test]
    fn test_is_valid_package_name() {
        assert!(BunLockb::is_valid_package_name("elysia"));
        assert!(BunLockb::is_valid_package_name("vue"));
        assert!(BunLockb::is_valid_package_name("@types/node"));
        assert!(BunLockb::is_valid_package_name("@babel/core"));
        assert!(BunLockb::is_valid_package_name("lodash.debounce"));
        assert!(!BunLockb::is_valid_package_name(""));
        assert!(!BunLockb::is_valid_package_name("has space"));
        assert!(!BunLockb::is_valid_package_name("has\ttab"));
    }

    #[test]
    fn test_read_string_inline() {
        // Inline string: "react" with null terminator, no high bit set
        let bytes: [u8; 8] = [b'r', b'e', b'a', b'c', b't', 0, 0, 0];
        let result = BunLockb::read_string(&bytes, &[]);
        assert_eq!(result, Some("react".to_string()));

        // Inline scoped package (short enough to fit)
        let bytes: [u8; 8] = [b'@', b'a', b'/', b'b', 0, 0, 0, 0];
        let result = BunLockb::read_string(&bytes, &[]);
        assert_eq!(result, Some("@a/b".to_string()));
    }

    #[test]
    fn test_read_string_external() {
        // External string: high bit set in byte[7]
        // Pointer { off: 5, len: 11 } for "@types/node" at offset 5
        let string_bytes = b"junk_@types/node_more";
        //                   01234567890123456789
        //                        ^--- offset 5, length 11

        // Build external pointer: off=5, len=11, with high bit set
        // Raw u64 = (len << 32) | off | (1 << 63)
        let off = 5u64;
        let len = 11u64;
        let raw = off | (len << 32) | (1u64 << 63);
        let bytes: [u8; 8] = raw.to_le_bytes();

        let result = BunLockb::read_string(&bytes, string_bytes);
        assert_eq!(result, Some("@types/node".to_string()));
    }

    #[test]
    fn test_parse_invalid_header() {
        // Too short
        assert!(BunLockb::parse(&[0u8; 10]).is_none());

        // Wrong magic
        let mut bad = vec![0u8; 200];
        bad[..10].copy_from_slice(b"wrong head");
        assert!(BunLockb::parse(&bad).is_none());
    }

    #[test]
    fn test_parse_real_lockb_embedded() {
        // Real bun.lockb from bun/bench/bundle (1139 bytes)
        // Contains: bundle (root) with devDependency bun-types@0.5.8
        const LOCKB_BASE64: &str = "IyEvdXNyL2Jpbi9lbnYgYnVuCmJ1bi1sb2NrZmlsZS1mb3JtYXQtdjAKAgAAALF9h1TvqtCSnpIhE6ZZVtegxB+KP4bOgQRHCtNLVpnM4wMAAAAAAAACAAAAAAAAAAgAAAAAAAAABwAAAAAAAACAAAAAAAAAABgCAAAAAAAAAABidW5kbGUAAAAAAAAJAACA9Ht4wQTAuqJ77GfNiBLrIgGdcG8BAAAAGJ5wbwEAAABwnXBvAQAAABzZM48BAFt3AAAAAAAAAAAAABAAAAAAADiQ8gIBAAAAQJ5wbwEAAAACAAAAAAAAAAkAAAA6AACAAAAAAAUAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAABAAAAAAAAAAAAAAABAAAAAQAAAAAAAAAAAP4P/gEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAQD+D/4BAAABAAAAAAAAAAAAAAAEVHwD0MAHo3wraYAeqTWH2NDmXOdGfC3wWWOnZvK93ytI6yq/LkgsCjDudWNmN7MlfPvJb2zoLMnkzhjxNwLsLwAAAAAbMY8BAGgjAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACQAAgAFeMC5gAgAAAAAAAHQCAAAAAAAACjxzcmMuaW5zdGFsbC5sb2NrZmlsZS5UcmVlPiAyMCBzaXplb2YsIDQgYWxpZ25vZgoAAAAAAAAAAAAA/v////////8AAAAAAQAAAKACAAAAAAAApAIAAAAAAAAKPHUzMj4gNCBzaXplb2YsIDQgYWxpZ25vZgoAAAAAANACAAAAAAAA1AIAAAAAAAAKPHUzMj4gNCBzaXplb2YsIDQgYWxpZ25vZgoAAQAAAAgDAAAAAAAAIgMAAAAAAAAKPFsyNl11OD4gMjYgc2l6ZW9mLCAxIGFsaWdub2YKAAAAAAAAAAAACQAAgHvsZ82IEusiCAFeMC41LjAAAGwDAAAAAAAAbAMAAAAAAAAKPHNyYy5pbnN0YWxsLnNlbXZlci5FeHRlcm5hbFN0cmluZz4gMTYgc2l6ZW9mLCA4IGFsaWdub2YKmAMAAAAAAADbAwAAAAAAAAo8dTg+IDEgc2l6ZW9mLCAxIGFsaWdub2YKAABidW4tdHlwZXNodHRwczovL3JlZ2lzdHJ5Lm5wbWpzLm9yZy9idW4tdHlwZXMvLS9idW4tdHlwZXMtMC41LjgudGd6AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD
            .decode(LOCKB_BASE64)
            .expect("invalid base64");

        let parsed = BunLockb::parse(&data).expect("failed to parse bun.lockb");

        // Should have 2 packages: root "bundle" and "bun-types"
        assert_eq!(parsed.packages.len(), 2);

        // Find bun-types package
        let bun_types = parsed
            .packages
            .iter()
            .find(|p| p.name == "bun-types")
            .expect("bun-types not found");
        assert_eq!(bun_types.version, "0.5.8"); // v2 format stores version in Resolution

        // Root package
        let root = parsed
            .packages
            .iter()
            .find(|p| p.name == "bundle")
            .expect("root package not found");
        assert_eq!(root.version, "0.0.0"); // Root packages have 0.0.0 version
    }
}
