# Package Repository Coverage

This document tracks moss-packages coverage against known package repositories.
Source: [Repology Statistics](https://repology.org/repositories/statistics)

Legend: `[x]` implemented, `[ ]` not implemented, `[~]` partial/needs verification

## Linux Distributions

### Debian/Ubuntu Family
- [x] apt (Debian/Ubuntu) - debian/ubuntu/linuxmint/pop_os/elementary/mx_linux/devuan/pureos/raspbian/tails/antiX/lmde/deepin/kali/parrot
- [x] termux - Android terminal packages

### Arch Family
- [x] pacman (Arch Linux official repos)
- [x] aur (Arch User Repository)
- [x] artix - Artix Linux
- [x] cachyos - CachyOS
- [x] endeavouros - EndeavourOS
- [x] manjaro - Manjaro Linux
- [ ] archlinuxcn - Chinese Arch repo
- [x] chaotic_aur - Pre-built AUR packages
- [ ] blackarch - Security/pentesting packages
- [ ] garuda - Garuda Linux
- [ ] rebornos - RebornOS

### Red Hat Family
- [x] dnf (Fedora/RHEL/CentOS)
- [x] copr - Fedora Copr (community builds, like AUR for Fedora)
- [ ] epel - Extra Packages for Enterprise Linux
- [ ] centos - CentOS Stream
- [ ] almalinux - AlmaLinux
- [ ] rocky - Rocky Linux
- [ ] rosa - ROSA Linux
- [ ] mageia - Mageia Linux
- [ ] openmandriva - OpenMandriva

### SUSE Family
- [~] opensuse - openSUSE (API needs verification)
- [ ] obs - Open Build Service (generic)

### Gentoo Family
- [~] gentoo - Gentoo Linux (API needs verification)
- [ ] calculate - Calculate Linux
- [ ] funtoo - Funtoo Linux
- [ ] exherbo - Exherbo Linux
- [ ] guru - Gentoo User Repository

### Independent Distros
- [x] apk (Alpine Linux)
- [x] nix (NixOS)
- [x] guix (GNU Guix)
- [~] void - Void Linux (API needs verification)
- [~] slackware - SlackBuilds.org (via GitHub raw files)
- [ ] solus - Solus Linux
- [ ] chimera - Chimera Linux
- [ ] adelie - Adelie Linux
- [ ] apertis - Apertis Linux
- [ ] kiss - KISS Linux
- [ ] t2 - T2 SDE
- [ ] crux - CRUX Linux
- [ ] gobolinux - GoboLinux
- [ ] pisi - PisiLinux
- [ ] mer - Mer/Sailfish
- [ ] postmarketos - PostmarketOS
- [ ] serene - Serene Linux
- [ ] ataraxia - Ataraxia Linux

### Specialty/Embedded
- [ ] buildroot - Buildroot packages
- [ ] yocto - Yocto Project/OpenEmbedded
- [ ] openwrt - OpenWrt router firmware
- [ ] lede - LEDE (merged into OpenWrt)
- [ ] entware - Entware (routers/NAS)

## BSD Systems

- [~] freebsd - FreeBSD ports/packages (API needs verification)
- [~] openbsd - OpenBSD ports (API needs verification)
- [~] netbsd - pkgsrc (API needs verification)
- [ ] dragonfly - DragonflyBSD dports
- [ ] midnightbsd - MidnightBSD mports
- [ ] ravenports - Ravenports (multi-platform)

## Windows

- [x] winget - Windows Package Manager
- [x] scoop - Scoop package manager
- [x] choco - Chocolatey
- [x] msys2 - MSYS2/MinGW packages
- [x] vcpkg - Microsoft C++ package manager
- [ ] cygwin - Cygwin packages
- [ ] npackd - Npackd Windows packages

## macOS

- [x] brew - Homebrew formulae
- [x] homebrew_casks - Homebrew Casks (GUI apps)
- [x] macports - MacPorts
- [ ] fink - Fink (Debian-derived for macOS)
- [ ] rudix - Rudix

## Language Package Managers

### Rust
- [x] cargo - crates.io

### JavaScript/TypeScript
- [x] npm - npmjs.com
- [x] deno - deno.land/x
- [x] jsr - JSR (JavaScript Registry)
- [ ] yarn - Yarn (uses npm registry)
- [ ] pnpm - pnpm (uses npm registry)

### Python
- [x] pip - PyPI
- [x] conda - conda-forge (repodata.json)
- [ ] conda_forge - conda-forge (same as conda)

### Ruby
- [x] gem - RubyGems

### Java/JVM
- [x] maven - Maven Central
- [ ] gradle - Gradle plugins
- [x] clojars - Clojure libraries

### .NET
- [x] nuget - NuGet Gallery

### Go
- [x] go - Go modules (proxy.golang.org)

### PHP
- [x] composer - Packagist

### Elixir/Erlang
- [x] hex - hex.pm
- [ ] rebar3 - Rebar3 (uses hex)

### Haskell
- [x] hackage - Hackage (hackage.haskell.org API)
- [ ] stackage - Stackage

### Perl
- [x] cpan - MetaCPAN (fastapi.metacpan.org)

### Lua
- [x] luarocks - LuaRocks (manifest parsing)

### R
- [x] cran - CRAN (crandb.r-pkg.org API)
- [x] bioconductor - Bioconductor (r-universe.dev API)

### OCaml
- [x] opam - OPAM (GitHub opam-repository)

### Swift
- [ ] swiftpm - Swift Package Manager

### Dart/Flutter
- [x] pub - pub.dev

### Julia
- [x] julia - Julia General registry (GitHub)

### Racket
- [x] racket - Racket packages (pkgs.racket-lang.org)

### Nim
- [x] nimble - Nimble (GitHub packages.json)

### Zig
- [ ] zig - Zig packages

### D
- [x] dub - DUB (D packages)

### C/C++
- [~] conan - Conan Center (CLI only, no REST API)
- [x] vcpkg - vcpkg (GitHub baseline.json)
- [x] hunter - Hunter C++ (GitHub cmake parsing)
- [ ] biicode - Biicode (discontinued)

### TeX
- [x] ctan - CTAN (TeX packages via ctan.org JSON API)
- [ ] texlive - TeX Live

## Mobile/App Stores

- [x] fdroid - F-Droid (Android FOSS)
- [ ] appimage - AppImage packages
- [x] flatpak - Flathub (API)
- [x] snap - Snapcraft

## Containers

- [x] docker - Docker Hub
- [ ] ghcr - GitHub Container Registry

## Other Package Systems

- [ ] chromebrew - ChromeOS packages
- [ ] pkgin - pkgin (pkgsrc-based)
- [ ] homebrew_linux - Linuxbrew

## Implementation Priority

### High Priority (popular ecosystems)
1. [x] hackage - Haskell **DONE**
2. [x] cpan/metacpan - Perl **DONE**
3. [x] luarocks - Lua **DONE**
4. [x] homebrew_casks - macOS GUI apps **DONE**
5. [x] flatpak - Modern Linux app distribution **DONE**
6. [x] conda - Data science ecosystem **DONE**

### Medium Priority
1. [x] manjaro - Popular Arch derivative **DONE**
2. [x] msys2 - Windows development **DONE**
3. [x] vcpkg - C++ ecosystem **DONE**
4. [x] pub - Dart/Flutter **DONE**
5. [x] opam - OCaml **DONE**
6. [x] cran - R packages **DONE**
7. [x] julia - Julia **DONE**
8. [x] nimble - Nim **DONE**
9. [x] macports - macOS **DONE**
10. [x] snap - Snapcraft **DONE**
11. [x] dub - D packages **DONE**
12. [x] clojars - Clojure **DONE**
13. [x] docker - Docker Hub **DONE**
14. [x] fdroid - F-Droid **DONE**

### Low Priority (niche or redundant)
- Derivatives that reuse parent repos (manjaro uses Arch, elementary uses apt)
- Discontinued/abandoned projects (biicode, rudix)
- Highly specialized (bioconductor, stackage)

## Notes

- Many Linux derivatives use their parent distro's package manager (apt, pacman, dnf)
- Some registries don't have public JSON APIs and require HTML scraping or binary format parsing
- fetch_all support varies: some have bulk download, others require pagination
