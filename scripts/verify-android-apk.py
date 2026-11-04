#!/usr/bin/env python3
"""Vérifie l'APK final : signature, alignement ZIP/ELF et politique réseau embarquée."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import zipfile


def run(*args):
    result = subprocess.run(args, capture_output=True, text=True, timeout=120)
    if result.returncode:
        raise ValueError(f"Échec {Path(args[0]).name} : {result.stdout}\n{result.stderr}")
    return result.stdout


def elf_alignment(data, machine):
    if len(data) < 64 or data[:6] != b"\x7fELF\x02\x01":
        raise ValueError("Bibliothèque ELF64 little-endian attendue")
    if struct.unpack_from("<H", data, 18)[0] != machine:
        raise ValueError("Architecture ELF différente du répertoire ABI")
    offset = struct.unpack_from("<Q", data, 32)[0]
    stride, count = struct.unpack_from("<HH", data, 54)
    if stride < 56 or count == 0 or offset + stride * count > len(data):
        raise ValueError("Table des segments ELF invalide")
    alignments = []
    for index in range(count):
        start = offset + stride * index
        kind = struct.unpack_from("<I", data, start)[0]
        if kind != 1:
            continue
        file_offset, address = struct.unpack_from("<QQ", data, start + 8)
        alignment = struct.unpack_from("<Q", data, start + 48)[0]
        if alignment < 16384 or alignment & (alignment - 1) or file_offset % 16384 != address % 16384:
            raise ValueError(f"Segment LOAD incompatible avec les pages 16 Kio : {alignment}")
        alignments.append(alignment)
    if not alignments:
        raise ValueError("Aucun segment ELF LOAD")
    return alignments


def network_policy(aapt, apk):
    manifest = run(str(aapt), "dump", "xmltree", str(apk), "--file", "AndroidManifest.xml")
    if not re.search(r":allowBackup\([^)]*\)=false", manifest):
        raise ValueError("Les sauvegardes de l'application doivent être désactivées")
    reference = re.search(r":networkSecurityConfig\([^)]*\)=@(0x[0-9a-f]+)", manifest)
    if not reference:
        raise ValueError("Politique réseau native absente du manifeste empaqueté")
    resources = run(str(aapt), "dump", "resources", str(apk))
    expected = rf"resource {reference[1]} xml/network_security_config\n\s+\(\) \(file\) res/xml/network_security_config.xml type=XML"
    if not re.search(expected, resources):
        raise ValueError("Le manifeste ne référence pas la politique réseau attendue")
    tree = run(str(aapt), "dump", "xmltree", str(apk), "--file", "res/xml/network_security_config.xml")
    # Contrat exact et volontairement fermé : HTTPS par défaut, une seule
    # exception HTTP locale explicite. Toute autre politique demande une revue.
    normalized = [re.sub(r" \(line=\d+\)$", "", line.strip()) for line in tree.splitlines() if line.strip()]
    if normalized != ["E: network-security-config", "E: base-config", "A: cleartextTrafficPermitted=false",
                       "E: domain-config", "A: cleartextTrafficPermitted=true", "E: domain",
                       "A: includeSubdomains=false", "T: '127.0.0.1'"]:
        raise ValueError("Politique réseau différente de HTTPS + exception 127.0.0.1 uniquement")
    return "HTTPS par défaut ; HTTP permis uniquement vers 127.0.0.1"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--sdk", type=Path, default=os.environ.get("ANDROID_SDK_ROOT", os.environ.get("ANDROID_HOME")))
    parser.add_argument("--build-tools", default="35.0.0")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    if not args.apk.is_file() or args.sdk is None:
        raise ValueError("APK existant et --sdk/ANDROID_SDK_ROOT requis")
    tools = args.sdk / "build-tools" / args.build_tools
    signature = run(str(tools / "apksigner"), "verify", "--verbose", str(args.apk))
    run(str(tools / "zipalign"), "-c", "-P", "16", "4", str(args.apk))
    native = {}
    expected = {f"lib/{abi}/{name}" for abi in ["arm64-v8a", "x86_64"] for name in ["libhakille_mobile.so", "libjnidispatch.so"]}
    with zipfile.ZipFile(args.apk) as archive:
        names = archive.namelist()
        if len(names) != len(set(names)):
            raise ValueError("Entrées ZIP dupliquées")
        present = {name for name in names if name.startswith("lib/") and name.endswith(".so")}
        if not expected <= present:
            raise ValueError(f"Bibliothèques natives manquantes : {sorted(expected - present)}")
        for name in sorted(present):
            parts = name.split("/")
            if len(parts) != 3 or parts[1] not in {"arm64-v8a", "x86_64"}:
                raise ValueError(f"ABI ou chemin natif inattendu : {name}")
            entry = archive.getinfo(name)
            if entry.file_size > 128 * 1024 * 1024 or entry.compress_type != zipfile.ZIP_STORED:
                raise ValueError(f"Bibliothèque trop volumineuse ou compressée : {name}")
            native[name] = elf_alignment(archive.read(name), 183 if "/arm64-v8a/" in name else 62)
    report = {"apk": str(args.apk), "sha256": hashlib.sha256(args.apk.read_bytes()).hexdigest(),
              "signature_verified": True, "zip_alignment_16k_verified": True,
              "elf_load_alignments": native, "network_policy": network_policy(tools / "aapt2", args.apk),
              "signature_details": signature.strip(), "runtime_tested": False}
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, subprocess.TimeoutExpired, zipfile.BadZipFile) as error:
        print(f"ÉCHEC APK : {error}", file=sys.stderr)
        sys.exit(1)
