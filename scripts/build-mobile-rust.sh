#!/usr/bin/env bash
# Compile le cœur Rust et régénère ses bindings ; n'installe aucun outil.
# Compatible avec Bash 3.2 fourni par macOS.
set -euo pipefail

usage() {
    cat <<'HELP'
Usage : scripts/build-mobile-rust.sh android|ios [options]

  --release             Optimisation release (debug par défaut).
  --offline             Interdit les téléchargements Cargo.
  --ios-x86_64-sim       Ajoute les simulateurs iOS Intel au XCFramework.
  -h, --help            Affiche cette aide.

Android : Linux x86_64 ou macOS, ANDROID_SDK_ROOT (ou ANDROID_HOME),
NDK 27.2.12479018 et cibles Rust aarch64-linux-android/x86_64-linux-android.
Compilation API 26, ABI arm64-v8a et x86_64, segments ELF alignés sur 16 Kio.
iOS : macOS avec Xcode, cibles Rust aarch64-apple-ios/aarch64-apple-ios-sim
(et x86_64-apple-ios avec --ios-x86_64-sim). Déploiement minimum iOS 16.

Cargo utilise --locked et peut télécharger les dépendances manquantes.
Aucun SDK, NDK, outil ou cible Rust n'est installé automatiquement.
Les bibliothèques et bindings générés sous mobile/ sont ignorés par Git.
Relancer après chaque modification de l'API Rust ; les bindings sont toujours
régénérés. CARGO_TARGET_DIR est respecté, sinon le target/ du dépôt est utilisé.
HELP
}

fail() { printf 'Erreur : %s\n' "$*" >&2; exit 1; }
require_command() { command -v "$1" >/dev/null 2>&1 || fail "outil requis absent : $1"; }
require_file() { [[ -s "$1" ]] || fail "fichier requis absent ou vide : $1"; }
require_target() {
    case $'\n'"$installed_targets"$'\n' in
        *$'\n'"$1"$'\n'*) ;;
        *) fail "cible Rust absente : $1 (à installer avec rustup target add $1)" ;;
    esac
}

platform=''
profile=debug
offline=false
intel_sim=false
for argument in "$@"; do
    case "$argument" in
        android|ios)
            [[ -z "$platform" ]] || fail 'une seule plateforme est autorisée'
            platform=$argument ;;
        --release) profile=release ;;
        --offline) offline=true ;;
        --ios-x86_64-sim) intel_sim=true ;;
        -h|--help) usage; exit 0 ;;
        *) fail "argument inconnu : $argument" ;;
    esac
done
[[ -n "$platform" ]] || { usage >&2; exit 2; }
[[ "$platform" == ios || "$intel_sim" == false ]] || fail '--ios-x86_64-sim exige ios'

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$repo_root"
require_command cargo
require_command rustc
require_command rustup
require_command mktemp
host_os=$(uname -s)
host_target=$(rustc -vV | sed -n 's/^host: //p')
[[ -n "$host_target" ]] || fail 'rustc ne fournit pas sa cible hôte'
installed_targets=$(rustup target list --installed)
require_target "$host_target"

cargo_flags=(--locked)
if [[ "$offline" == true ]]; then
    cargo_flags+=(--offline)
    # UniFFI invoque également cargo metadata : appliquer la même politique.
    export CARGO_NET_OFFLINE=true
else
    printf '%s\n' 'Cargo --locked : téléchargement possible des dépendances manquantes.'
fi
if [[ "$profile" == release ]]; then cargo_flags+=(--release); fi

if [[ "$platform" == android ]]; then
    require_target aarch64-linux-android
    require_target x86_64-linux-android
    android_sdk=${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}
    [[ -n "$android_sdk" ]] || fail 'définir ANDROID_SDK_ROOT ou ANDROID_HOME'
    ndk_root="$android_sdk/ndk/27.2.12479018"
    require_file "$ndk_root/source.properties"
    case "$host_os:$(uname -m)" in
        Linux:x86_64) ndk_host=linux-x86_64 ;;
        Darwin:arm64|Darwin:x86_64) ndk_host=darwin-x86_64 ;;
        *) fail 'hôte non pris en charge par ce script Android (Linux x86_64 ou macOS requis)' ;;
    esac
    ndk_bin="$ndk_root/toolchains/llvm/prebuilt/$ndk_host/bin"
    for ndk_tool in aarch64-linux-android26-clang x86_64-linux-android26-clang llvm-ar llvm-readelf; do
        [[ -x "$ndk_bin/$ndk_tool" ]] || fail "outil NDK absent : $ndk_bin/$ndk_tool"
    done
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ndk_bin/aarch64-linux-android26-clang"
    export CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER="$ndk_bin/x86_64-linux-android26-clang"
    export CC_aarch64_linux_android="$CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER"
    export CC_x86_64_linux_android="$CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER"
    export AR_aarch64_linux_android="$ndk_bin/llvm-ar"
    export AR_x86_64_linux_android="$ndk_bin/llvm-ar"
else
    [[ "$host_os" == Darwin ]] || fail 'la compilation iOS exige macOS avec Xcode'
    require_command xcodebuild
    require_command xcrun
    xcrun --sdk iphoneos --show-sdk-path >/dev/null || fail 'SDK iPhoneOS indisponible'
    xcrun --sdk iphonesimulator --show-sdk-path >/dev/null || fail 'SDK simulateur iOS indisponible'
    require_target aarch64-apple-ios
    require_target aarch64-apple-ios-sim
    if [[ "$intel_sim" == true ]]; then require_target x86_64-apple-ios; fi
    export IPHONEOS_DEPLOYMENT_TARGET=16.0
fi

# Un seul répertoire de compilation, partagé avec les autres commandes Cargo.
target_dir=${CARGO_TARGET_DIR:-"$repo_root/target"}
mkdir -p "$target_dir"
target_dir=$(cd "$target_dir" && pwd -P)
export CARGO_TARGET_DIR="$target_dir"
staging=$(mktemp -d "$target_dir/mobile-build.XXXXXX")
trap 'rm -rf -- "$staging"' EXIT

# Cible hôte explicite : CARGO_BUILD_TARGET ne doit pas produire un générateur
# Android/iOS inexécutable sur la machine de compilation.
cargo build -p hakille-mobile --features cli --bin hakille-mobile-bindgen \
    --target "$host_target" "${cargo_flags[@]}"
bindgen="$target_dir/$host_target/$profile/hakille-mobile-bindgen"
[[ -x "$bindgen" ]] || fail "générateur hôte absent : $bindgen"

build_library() {
    if [[ "$platform" == android ]]; then
        # Les arguments finaux concernent ce crate, pas ses dépendances. Cargo
        # conserve ainsi RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS et leur priorité native.
        # NDK 27 ne garantit pas cet alignement par défaut (Android pages 16 Kio).
        cargo rustc -p hakille-mobile --features ffi --lib --target "$1" "${cargo_flags[@]}" -- \
            -C link-arg=-Wl,-z,max-page-size=16384 \
            -C link-arg=-Wl,-z,common-page-size=16384
    else
        cargo build -p hakille-mobile --features ffi --lib --target "$1" "${cargo_flags[@]}"
    fi
}

verify_android_alignment() {
    local headers alignments alignment load_count
    headers=$("$ndk_bin/llvm-readelf" --program-headers --wide "$1") \
        || fail "lecture ELF impossible : $1"
    alignments=$(printf '%s\n' "$headers" | awk '$1 == "LOAD" { print $NF }')
    [[ -n "$alignments" ]] || fail "aucun segment ELF LOAD : $1"
    load_count=0
    while IFS= read -r alignment; do
        [[ "$alignment" =~ ^0x[0-9a-fA-F]{1,15}$ ]] \
            || fail "alignement ELF illisible ($alignment) : $1"
        (( alignment >= 16384 )) \
            || fail "segment ELF LOAD aligné à $alignment, inférieur à 16 Kio : $1"
        load_count=$((load_count + 1))
    done <<< "$alignments"
    printf 'ELF 16 Kio vérifié : %s (%s segments LOAD)\n' "$1" "$load_count"
}

if [[ "$platform" == android ]]; then
    build_library aarch64-linux-android
    build_library x86_64-linux-android
    arm_library="$target_dir/aarch64-linux-android/$profile/libhakille_mobile.so"
    intel_library="$target_dir/x86_64-linux-android/$profile/libhakille_mobile.so"
    require_file "$arm_library"
    require_file "$intel_library"
    verify_android_alignment "$arm_library"
    verify_android_alignment "$intel_library"
    # UniFFI lit les métadonnées ELF ; il n'exécute pas la bibliothèque Android.
    # Le uniffi.toml local est découvert via cargo metadata (configuration crate).
    "$bindgen" generate --library "$arm_library" --language kotlin \
        --crate hakille_mobile --metadata-no-deps --no-format --out-dir "$staging/kotlin"
    kotlin_file="$staging/kotlin/uniffi/hakille_mobile/hakille_mobile.kt"
    require_file "$kotlin_file"
    android_main="$repo_root/mobile/android/app/src/main"
    mkdir -p "$android_main/jniLibs/arm64-v8a" "$android_main/jniLibs/x86_64" \
        "$android_main/java/uniffi/hakille_mobile"
    cp "$arm_library" "$android_main/jniLibs/arm64-v8a/libhakille_mobile.so"
    cp "$intel_library" "$android_main/jniLibs/x86_64/libhakille_mobile.so"
    cp "$kotlin_file" "$android_main/java/uniffi/hakille_mobile/hakille_mobile.kt"
    printf '%s\n' "Android ($profile) : bibliothèques arm64-v8a/x86_64 et bindings Kotlin générés."
else
    build_library aarch64-apple-ios
    build_library aarch64-apple-ios-sim
    device_library="$target_dir/aarch64-apple-ios/$profile/libhakille_mobile.a"
    simulator_library="$target_dir/aarch64-apple-ios-sim/$profile/libhakille_mobile.a"
    require_file "$device_library"
    require_file "$simulator_library"
    if [[ "$intel_sim" == true ]]; then
        build_library x86_64-apple-ios
        intel_library="$target_dir/x86_64-apple-ios/$profile/libhakille_mobile.a"
        require_file "$intel_library"
        xcrun lipo -create "$simulator_library" "$intel_library" \
            -output "$staging/libhakille_mobile.a"
        simulator_library="$staging/libhakille_mobile.a"
    fi
    "$bindgen" generate --library "$device_library" --language swift \
        --crate hakille_mobile --metadata-no-deps --no-format --out-dir "$staging/swift"
    require_file "$staging/swift/HakilleMobile.swift"
    require_file "$staging/swift/HakilleMobileFFI.h"
    require_file "$staging/swift/HakilleMobileFFI.modulemap"
    mkdir -p "$staging/headers"
    cp "$staging/swift/HakilleMobileFFI.h" "$staging/headers/"
    # Xcode recherche module.modulemap à la racine des en-têtes d'une slice.
    cp "$staging/swift/HakilleMobileFFI.modulemap" "$staging/headers/module.modulemap"
    xcodebuild -create-xcframework \
        -library "$device_library" -headers "$staging/headers" \
        -library "$simulator_library" -headers "$staging/headers" \
        -output "$staging/HakilleMobile.xcframework"
    require_file "$staging/HakilleMobile.xcframework/Info.plist"
    ios_root="$repo_root/mobile/ios"
    mkdir -p "$ios_root/Generated" "$ios_root/Frameworks"
    cp "$staging/swift/HakilleMobile.swift" "$staging/swift/HakilleMobileFFI.h" \
        "$staging/swift/HakilleMobileFFI.modulemap" "$ios_root/Generated/"
    rm -rf -- "$ios_root/Frameworks/HakilleMobile.xcframework"
    mv "$staging/HakilleMobile.xcframework" "$ios_root/Frameworks/"
    printf '%s\n' "iOS ($profile) : XCFramework appareil/simulateur et bindings Swift générés."
fi
