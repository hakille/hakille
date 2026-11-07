#!/usr/bin/env python3
"""Génère des fixtures vocales SYNTHÉTIQUES ; aucun téléchargement ni installation."""

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import wave


CASES = (
    ("fr_hakille_command", "fr", "Hakille, ouvre le calendrier.", True),
    ("fr_hakille_only", "fr", "Hakille", True),
    ("fr_command_negative", "fr", "Ouvre le calendrier.", False),
    ("fr_greeting_negative", "fr", "Bonjour, quelle heure est-il ?", False),
    ("en_hakille_command", "en-us", "Hakille, what time is it?", True),
    ("en_greeting_negative", "en-us", "Hello, what time is it?", False),
    ("fr_akille_command_variant", "fr", "Akille, ouvre le calendrier.", True),
    ("fr_akille_only_variant", "fr", "Akille", True),
)
SAMPLE_RATE = 16_000
SPEED = 150


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_command(arguments):
    # Aucun shell : textes, chemins et arguments restent des valeurs littérales.
    return subprocess.run(
        arguments, check=True, capture_output=True, text=True, timeout=120
    ).stdout


def tool_info(command, version_flag):
    executable = shutil.which(command)
    if executable is None:
        raise ValueError(f"Exécutable absent : {command}. Installer cet outil séparément.")
    path = Path(executable).resolve()
    version = run_command([str(path), version_flag]).strip().splitlines()
    if not version:
        raise ValueError(f"Version de l'exécutable indisponible : {path}")
    return {"path": str(path), "version": version[0], "sha256": sha256(path)}


def validate_audio(wav_path, pcm_path):
    with wave.open(str(wav_path), "rb") as audio:
        if (audio.getnchannels(), audio.getsampwidth(), audio.getframerate()) != (
            1, 2, SAMPLE_RATE
        ):
            raise ValueError(f"Format WAV inattendu : {wav_path.name}")
        samples = audio.getnframes()
        if samples <= 0 or len(audio.readframes(samples)) != samples * 2:
            raise ValueError(f"WAV vide ou tronqué : {wav_path.name}")
    pcm = pcm_path.read_bytes()
    if len(pcm) != samples * 4:
        raise ValueError(f"Taille PCM incohérente : {pcm_path.name}")
    nonzero = False
    for (sample,) in struct.iter_unpack("<f", pcm):
        if not math.isfinite(sample) or abs(sample) > 1.0:
            raise ValueError(f"PCM non fini ou non normalisé : {pcm_path.name}")
        nonzero |= sample != 0.0
    if not nonzero:
        raise ValueError(f"Voix synthétique silencieuse : {pcm_path.name}")
    return samples


def prepare(options):
    output = options.output.expanduser().resolve()
    names = [name + suffix for name, *_ in CASES for suffix in (".wav", ".f32le")]
    names.append("manifest.json")
    if not options.force:
        existing = [name for name in names if os.path.lexists(output / name)]
        if existing:
            raise FileExistsError(
                "Fichiers existants refusés (utiliser --force pour les remplacer) : "
                + ", ".join(existing)
            )
    tools = {
        "espeak_ng": tool_info(options.espeak_ng, "--version"),
        "ffmpeg": tool_info(options.ffmpeg, "-version"),
    }
    manifest = {
        "synthetic": True,
        "source": "eSpeak NG local ; aucune voix humaine enregistrée",
        "caution": (
            "Échantillons synthétiques uniquement : aucune preuve de précision sur "
            "microphone réel ou de couverture des accents. Le texte d'entrée de synthèse "
            "ne doit jamais être substitué à une transcription ASR."
        ),
        "sample_rate": SAMPLE_RATE,
        "channels": 1,
        "pcm_format": "IEEE754 float32 little-endian raw; PCM16 normalized by ffmpeg",
        "provenance": {
            "tools": tools,
            "generator_sha256": sha256(Path(__file__)),
            "python_version": sys.version.split()[0],
            "espeak_data_path_override": os.environ.get("ESPEAK_DATA_PATH"),
            "note": (
                "Versions et empreintes des exécutables utilisés, sans attestation "
                "de leur paquet d'installation. Les données vocales et versions des "
                "outils peuvent changer les octets générés."
            ),
        },
        "fixtures": [],
    }
    output.mkdir(parents=True, exist_ok=True)
    # Générer et valider tout le lot avant d'installer les résultats définitifs.
    with tempfile.TemporaryDirectory(prefix=".voice-fixtures-", dir=output) as temporary:
        staging = Path(temporary)
        for name, voice, text, positive in CASES:
            native = staging / (name + ".native.wav")
            wav_path = staging / (name + ".wav")
            pcm_path = staging / (name + ".f32le")
            run_command([
                tools["espeak_ng"]["path"], "-v", voice, "-s", str(SPEED),
                "-w", str(native), text,
            ])
            run_command([
                tools["ffmpeg"]["path"], "-nostdin", "-hide_banner", "-loglevel",
                "error", "-y", "-i", str(native), "-ar", str(SAMPLE_RATE),
                "-ac", "1", "-c:a", "pcm_s16le", str(wav_path),
            ])
            run_command([
                tools["ffmpeg"]["path"], "-nostdin", "-hide_banner", "-loglevel",
                "error", "-y", "-i", str(wav_path), "-f", "f32le", "-c:a",
                "pcm_f32le", str(pcm_path),
            ])
            samples = validate_audio(wav_path, pcm_path)
            manifest["fixtures"].append({
                "name": name, "voice": voice, "input_text": text,
                "contains_intended_wake_name": positive, "synthetic": True,
                "speed_words_per_minute": SPEED, "samples": samples,
                "duration_seconds": samples / SAMPLE_RATE,
                "wav": str(output / wav_path.name), "wav_sha256": sha256(wav_path),
                "pcm": str(output / pcm_path.name), "pcm_sha256": sha256(pcm_path),
            })
        (staging / "manifest.json").write_text(
            json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        for name in names:
            if options.force:
                os.replace(staging / name, output / name)
            else:
                # Création exclusive, même si un fichier apparaît après le contrôle initial.
                os.link(staging / name, output / name)
    print(f"8 fixtures SYNTHÉTIQUES (WAV + PCM mono 16 kHz) : {output / 'manifest.json'}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True, help="Dossier des 17 fichiers générés")
    parser.add_argument("--espeak-ng", default="espeak-ng", help="Exécutable eSpeak NG (PATH par défaut)")
    parser.add_argument("--ffmpeg", default="ffmpeg", help="Exécutable FFmpeg (PATH par défaut)")
    parser.add_argument("--force", action="store_true", help="Remplacer les fichiers générés existants")
    options = parser.parse_args()
    try:
        prepare(options)
    except subprocess.CalledProcessError as error:
        print(f"Échec de l'outil ({error.returncode}) : {error.stderr.strip()}", file=sys.stderr)
        return 1
    except (OSError, ValueError, wave.Error, subprocess.TimeoutExpired) as error:
        print(f"Échec : {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
