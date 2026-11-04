#!/usr/bin/env python3
"""Smoke UI réel sur émulateur dédié : efface uniquement les données de l'app testée.

Ne demande aucune permission, ne connecte aucun serveur et ne soumet aucun message.
Le microphone et la justesse de reconnaissance vocale ne sont pas testés ici.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

PACKAGE = "dev.hakille.mobile"
ACTIVITY = PACKAGE + "/.MainActivity"
DRAFT = "HakilleSmokeDraft2026"


class AppStoppedError(RuntimeError):
    """Un processus mort ne doit pas être masqué par des tentatives UI."""


class AndroidSmoke:
    def __init__(self, adb, serial, artifacts, timeout_scale=1.0):
        self.prefix = [str(adb), "-s", serial]
        self.artifacts = artifacts
        self.steps = []
        self.timeout_scale = timeout_scale

    def adb(self, *arguments, timeout=45, binary=False):
        result = subprocess.run(self.prefix + list(arguments), capture_output=True,
                                text=not binary, timeout=timeout * self.timeout_scale)
        if result.returncode:
            raise RuntimeError(f"adb {' '.join(arguments)} : {result.stderr!r} {result.stdout!r}")
        return result.stdout

    def alive(self):
        try:
            process = self.adb("shell", "pidof", PACKAGE).strip()
        except RuntimeError as error:
            raise AppStoppedError("Processus Hakille absent ou appareil inaccessible") from error
        if not process:
            raise AppStoppedError("Le processus de l'application s'est arrêté")

    def ui(self):
        self.alive()
        remote = "/data/local/tmp/hakille-smoke-ui.xml"
        self.adb("shell", "rm", "-f", remote)
        output = self.adb("shell", "uiautomator", "dump", remote)
        (self.artifacts / "uiautomator-last-output.txt").write_text(output)
        # Certaines erreurs uiautomator retournent 0 sans créer de fichier.
        # Les expliquer avant de tenter de parser le message de cat comme XML.
        if "dumped to:" not in output or remote not in output:
            raise RuntimeError("UIAutomator n'a pas confirmé de dump : " + output.strip()[:500])
        # Le protocole shell transporte le code de retour de cat, contrairement
        # à exec-out sur certains appareils. exec-out reste réservé aux PNG.
        data = self.adb("shell", "cat", remote)
        (self.artifacts / "uiautomator-last-content.txt").write_text(data)
        return ET.fromstring(data), data

    def wait_ui(self, predicate, label, seconds=90):
        print("WAIT", label, flush=True)
        started = time.monotonic()
        deadline = started + seconds * self.timeout_scale
        last_error = "critère UI absent"
        while time.monotonic() < deadline:
            try:
                tree, data = self.ui()
                (self.artifacts / f"{label}-last.xml").write_text(data)
                if predicate(tree):
                    (self.artifacts / f"{label}.xml").write_text(data)
                    (self.artifacts / f"{label}.png").write_bytes(
                        self.adb("exec-out", "screencap", "-p", binary=True))
                    self.steps.append(label)
                    print("PASS", label, flush=True)
                    return tree
                visible = [node.get("text") for node in tree.iter("node") if node.get("text")][:4]
                last_error = "critère UI absent ; textes : " + repr(visible)[:240]
            except AppStoppedError:
                raise
            except (RuntimeError, subprocess.TimeoutExpired, ET.ParseError) as error:
                last_error = str(error)
            print(f"RETRY {label} ({round(time.monotonic() - started)} s) : {last_error}", flush=True)
            time.sleep(1)
        raise RuntimeError(f"Échéance UI {label} : {last_error}")

    def tap(self, node):
        bounds = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", node.attrib.get("bounds", ""))
        if not bounds:
            raise RuntimeError("Bornes UI absentes")
        left, top, right, bottom = map(int, bounds.groups())
        self.adb("shell", "input", "tap", str((left + right) // 2), str((top + bottom) // 2))


def has_text(tree, *labels):
    return any(node.get("text") in labels or node.get("content-desc") in labels for node in tree.iter("node"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--serial", default=os.environ.get("ANDROID_SERIAL"))
    parser.add_argument("--artifacts", type=Path, default=Path("target/mobile-validation/android/runtime"))
    parser.add_argument("--timeout-scale", type=float, default=1.0,
                        help="Multiplie les délais adb/UI, entre 1 et 5 (TCG sans KVM : 3 ou 4)")
    parser.add_argument("--skip-install", action="store_true",
                        help="Réutilise seulement un APK installé dont le SHA256 est identique ; réinitialise ses données")
    args = parser.parse_args()
    if not math.isfinite(args.timeout_scale) or not 1 <= args.timeout_scale <= 5:
        raise ValueError("--timeout-scale doit être compris entre 1 et 5")
    sdk = os.environ.get("ANDROID_SDK_ROOT", os.environ.get("ANDROID_HOME"))
    adb = Path(sdk) / "platform-tools/adb" if sdk else shutil.which("adb")
    if not adb or not args.apk.is_file():
        raise ValueError("adb et un APK existant sont requis")
    serial = args.serial
    if not serial:
        devices = subprocess.check_output([str(adb), "devices"], text=True)
        candidates = [line.split()[0] for line in devices.splitlines()
                      if re.fullmatch(r"emulator-\d+\s+device", line.strip())]
        if len(candidates) != 1:
            raise ValueError("Sélectionner un émulateur unique avec --serial")
        serial = candidates[0]
    args.artifacts.mkdir(parents=True, exist_ok=True)
    smoke = AndroidSmoke(adb, serial, args.artifacts, args.timeout_scale)
    if smoke.adb("shell", "getprop", "ro.kernel.qemu").strip() != "1":
        raise ValueError("Ce test efface les données Hakille : émulateur dédié obligatoire")
    if smoke.adb("shell", "getprop", "sys.boot_completed").strip() != "1":
        raise ValueError("Le démarrage Android n'est pas terminé")
    rotation = {key: smoke.adb("shell", "settings", "get", "system", key).strip()
                for key in ["accelerometer_rotation", "user_rotation"]}
    started_at = smoke.adb("shell", "date", "+%s").strip() + ".000"
    result = {"serial": serial, "apk_sha256": hashlib.sha256(args.apk.read_bytes()).hexdigest(),
              "api_level": smoke.adb("shell", "getprop", "ro.build.version.sdk").strip(),
              "abi": smoke.adb("shell", "getprop", "ro.product.cpu.abi").strip(),
              "kernel_page_size": smoke.adb("shell", "getconf", "PAGE_SIZE").strip(),
              "display_size": smoke.adb("shell", "wm", "size").strip(),
              "display_density": smoke.adb("shell", "wm", "density").strip(),
              "timeout_scale": args.timeout_scale, "reused_installation": args.skip_install,
              "steps": smoke.steps, "passed": False,
              "physical_device": False, "microphone_tested": False, "network_connection_tested": False}
    try:
        if args.skip_install:
            print("VERIFY INSTALLED APK", flush=True)
            installed = smoke.adb("shell", "pm", "path", PACKAGE).strip()
            if not re.fullmatch(r"package:/data/app/[-A-Za-z0-9_~/+=.]+/base\.apk", installed):
                raise ValueError("APK installé unique introuvable ; retirer --skip-install")
            installed_hash = smoke.adb("shell", "sha256sum", installed.removeprefix("package:")).split()[0]
            if installed_hash != result["apk_sha256"]:
                raise ValueError("L'APK installé diffère de l'APK testé ; retirer --skip-install")
        else:
            print("INSTALL", args.apk, flush=True)
            smoke.adb("install", "-r", str(args.apk.resolve()), timeout=180)
        cleared = smoke.adb("shell", "pm", "clear", PACKAGE)
        if "Success" not in cleared:
            raise RuntimeError("Impossible de réinitialiser l'app sur l'émulateur dédié")
        smoke.adb("shell", "settings", "put", "system", "accelerometer_rotation", "0")
        smoke.adb("shell", "settings", "put", "system", "user_rotation", "0")
        smoke.adb("shell", "input", "keyevent", "KEYCODE_WAKEUP")
        smoke.adb("shell", "wm", "dismiss-keyguard")
        print("LAUNCH", ACTIVITY, flush=True)
        smoke.adb("shell", "am", "start", "-W", "-n", ACTIVITY, timeout=120)
        initial = smoke.wait_ui(lambda root: has_text(root, "Message to Hakille", "Message à Hakille"), "native_bridge_loaded")
        field = next((node for node in initial.iter("node") if node.get("class") == "android.widget.EditText"), None)
        if field is None:
            raise RuntimeError("Champ de brouillon absent")
        smoke.tap(field)
        smoke.adb("shell", "input", "text", DRAFT)
        smoke.adb("shell", "input", "keyevent", "KEYCODE_BACK")
        smoke.wait_ui(lambda root: has_text(root, DRAFT), "rust_draft_dispatch")
        smoke.adb("shell", "settings", "put", "system", "user_rotation", "1")
        smoke.wait_ui(lambda root: root.get("rotation") == "1" and has_text(root, DRAFT), "rotation_preserves_draft")
        smoke.adb("shell", "input", "keyevent", "KEYCODE_HOME")
        time.sleep(1)
        smoke.adb("shell", "am", "start", "-W", "-n", ACTIVITY)
        smoke.wait_ui(lambda root: has_text(root, DRAFT), "background_return_preserves_draft")
        smoke.adb("shell", "settings", "put", "system", "user_rotation", "0")
        smoke.wait_ui(lambda root: root.get("rotation") == "0" and has_text(root, DRAFT), "portrait_restored")
        smoke.adb("shell", "am", "force-stop", PACKAGE)
        smoke.adb("shell", "am", "start", "-W", "-n", ACTIVITY, timeout=120)
        tree = smoke.wait_ui(lambda root: has_text(root, DRAFT), "process_restart_restores_draft")
        for _ in range(6):
            tab = next((node for node in tree.iter("node") if node.get("text") in {"Connection", "Connexion"}), None)
            if tab is not None:
                smoke.tap(tab)
                break
            # La barre d'onglets est défilable ; se repérer sur ses vrais nœuds.
            known_tab = next((node for node in tree.iter("node") if node.get("text") in {"Chat", "Status", "Statut", "Agents"}), None)
            if known_tab is None:
                raise RuntimeError("Barre d'onglets absente")
            bounds = list(map(int, re.findall(r"\d+", known_tab.get("bounds", ""))))
            size = smoke.adb("shell", "wm", "size")
            width = int(re.findall(r"(\d+)x\d+", size)[-1])
            y = (bounds[1] + bounds[3]) // 2
            smoke.adb("shell", "input", "swipe", str(width * 4 // 5), str(y), str(width // 5), str(y), "350")
            tree, _ = smoke.ui()
        else:
            raise RuntimeError("Onglet connexion inaccessible après six défilements")
        smoke.wait_ui(lambda root: has_text(root, "Gateway base URL", "URL de base de la passerelle")
                      and has_text(root, "Mobile gateway token", "Jeton mobile de la passerelle"), "connection_page")
        smoke.alive()
        result["passed"] = True
    except Exception as error:
        result["error"] = str(error)
        raise
    finally:
        for key, value in rotation.items():
            try:
                if value == "null": smoke.adb("shell", "settings", "delete", "system", key)
                else: smoke.adb("shell", "settings", "put", "system", key, value)
            except (RuntimeError, subprocess.TimeoutExpired):
                pass
        diagnostic_error = None
        try:
            crashes = smoke.adb("logcat", "-b", "crash", "-d", "-v", "epoch", "-T", started_at)
            (args.artifacts / "crash-log.txt").write_text(crashes)
            if PACKAGE in crashes and result["passed"]:
                diagnostic_error = "Un crash Hakille est enregistré pendant le test"
        except (RuntimeError, subprocess.TimeoutExpired):
            if result["passed"]:
                diagnostic_error = "Le journal des crashs ne peut pas être vérifié"
        if diagnostic_error:
            result["passed"] = False
            result["error"] = diagnostic_error
        (args.artifacts / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        if diagnostic_error:
            raise RuntimeError(diagnostic_error)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, ET.ParseError) as error:
        print(f"ÉCHEC RUNTIME ANDROID : {error}", file=sys.stderr)
        sys.exit(1)
