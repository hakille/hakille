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
from mobile_process import run_command

PACKAGE = "dev.hakille.mobile"
ACTIVITY = PACKAGE + "/.MainActivity"
DRAFT = "HakilleSmokeDraft2026"


class AppStoppedError(RuntimeError):
    """Un processus mort ne doit pas être masqué par des tentatives UI."""


class SmokeDeadlineError(RuntimeError):
    """Le budget global ne se multiplie pas avec les phases ou tentatives."""


class UnexpectedSystemDialogError(RuntimeError):
    """Ne ferme jamais automatiquement une permission ou un dialogue de crash."""


class AndroidSmoke:
    def __init__(self, adb, serial, artifacts, timeout_scale=1.0, max_duration=600):
        self.prefix = [str(adb)] + (["-s", serial] if serial else [])
        self.artifacts = artifacts
        self.steps = []
        self.timeout_scale = timeout_scale
        self.started = time.monotonic()
        self.deadline = self.started + max_duration
        self.phase_deadline = None

    def begin_cleanup(self):
        self.phase_deadline = None
        self.deadline = time.monotonic() + 20
        print("CLEANUP (budget total 20s)", flush=True)

    def adb(self, *arguments, timeout=45, binary=False):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise SmokeDeadlineError("Budget global du scénario Android épuisé")
        budget = min(timeout * self.timeout_scale, remaining)
        if self.phase_deadline is not None:
            budget = min(budget, self.phase_deadline - time.monotonic())
        if budget <= 0:
            raise RuntimeError("Budget de la phase UI épuisé")
        try:
            result = run_command(self.prefix + list(arguments), timeout=budget,
                                 label="adb " + " ".join(arguments[:3]), capture=True, binary=binary)
        except subprocess.CalledProcessError as error:
            raise RuntimeError(f"adb {' '.join(arguments)} : {error.stderr!r} {error.stdout!r}") from error
        except subprocess.TimeoutExpired as error:
            if time.monotonic() >= self.deadline:
                raise SmokeDeadlineError("Budget global du scénario Android épuisé") from error
            raise
        return result.stdout

    def alive(self):
        try:
            process = self.adb("shell", "pidof", PACKAGE).strip()
        except SmokeDeadlineError:
            raise
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
        tree = ET.fromstring(data)
        blocked = any(node.get("package") in {"com.android.permissioncontroller", "com.google.android.permissioncontroller"}
                      or node.get("resource-id") in {"android:id/aerr_close", "android:id/aerr_wait"}
                      for node in tree.iter("node"))
        if blocked:
            (self.artifacts / "unexpected-system-dialog.xml").write_text(data)
            raise UnexpectedSystemDialogError("Permission ou erreur système inattendue ; aucune action automatique autorisée")
        return tree, data

    def wait_ui(self, predicate, label, seconds=90):
        print("WAIT", label, flush=True)
        started = time.monotonic()
        deadline = min(self.deadline, started + seconds * self.timeout_scale)
        last_error = "critère UI absent"
        previous_deadline = self.phase_deadline
        self.phase_deadline = deadline
        try:
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
                except (AppStoppedError, SmokeDeadlineError, UnexpectedSystemDialogError):
                    raise
                except (RuntimeError, subprocess.TimeoutExpired, ET.ParseError) as error:
                    last_error = str(error)
                print(f"RETRY {label} ({round(time.monotonic() - started)} s) : {last_error}", flush=True)
                time.sleep(min(1, max(0, deadline - time.monotonic())))
            raise RuntimeError(f"Échéance UI {label} : {last_error}")
        finally:
            self.phase_deadline = previous_deadline

    def tap(self, node):
        bounds = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", node.attrib.get("bounds", ""))
        if not bounds:
            raise RuntimeError("Bornes UI absentes")
        left, top, right, bottom = map(int, bounds.groups())
        self.adb("shell", "input", "tap", str((left + right) // 2), str((top + bottom) // 2))


def has_text(tree, *labels):
    return any(node.get("text") in labels or node.get("content-desc") in labels for node in tree.iter("node"))


def tagged(tree, tag):
    return next((node for node in tree.iter("node") if node.get("resource-id") in {tag, f"{PACKAGE}:id/{tag}"}), None)


def field_in(tree):
    field = tagged(tree, "draft_input")
    if field is None:
        return next((node for node in tree.iter("node") if node.get("class") == "android.widget.EditText"), None)
    return next((node for node in field.iter("node") if node.get("class") == "android.widget.EditText"), field)


def draft_matches(tree):
    field = field_in(tree)
    return field is not None and any(node.get("text") == DRAFT for node in field.iter("node"))


def bounds_of(node):
    values = re.fullmatch(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", node.get("bounds", ""))
    if values is None:
        raise RuntimeError("Bornes du contrôle absentes")
    return tuple(map(int, values.groups()))


def find_scrolled(smoke, tree, tag, container):
    for attempt in range(9):
        node = tagged(tree, tag)
        if node is not None:
            left, top, right, bottom = bounds_of(node)
            if right - left >= 8 and bottom - top >= 8:
                return tree, node
        if attempt == 8:
            break
        region = tagged(tree, container)
        if region is None:
            raise RuntimeError(f"Conteneur {container} absent pour atteindre {tag}")
        left, top, right, bottom = bounds_of(region)
        x = (left + right) // 2
        print(f"SCROLL {tag} ({attempt + 1}/8)", flush=True)
        smoke.adb("shell", "input", "swipe", str(x), str(top + (bottom - top) * 4 // 5),
                  str(x), str(top + (bottom - top) // 5), "300")
        tree, data = smoke.ui()
        (smoke.artifacts / f"scroll-{tag}-last.xml").write_text(data)
    raise RuntimeError(f"Contrôle {tag} inaccessible après huit défilements")


def reveal_draft(smoke, tree=None):
    if tree is None:
        tree, _ = smoke.ui()
    # Le nouvel accueil montre d'abord la présence vocale. Écrire ne soumet
    # aucun prompt et n'autorise pas le microphone ; il défile vers le brouillon.
    write = tagged(tree, "voice_write")
    if write is not None and write.get("enabled") == "true":
        smoke.tap(write)
    elif field_in(tree) is None and tagged(tree, "screen_voice") is not None:
        find_scrolled(smoke, tree, "draft_input", "screen_voice")


def hide_keyboard(smoke):
    state = smoke.adb("shell", "dumpsys", "input_method")
    if re.search(r"(?:mInputShown|mIsInputViewShown)=true", state):
        smoke.adb("shell", "input", "keyevent", "KEYCODE_BACK")


def connection_fields(tree):
    return ((tagged(tree, "connection_endpoint") is not None and tagged(tree, "connection_token") is not None)
            or (has_text(tree, "Gateway base URL", "URL de base de la passerelle")
                and has_text(tree, "Mobile gateway token", "Jeton mobile de la passerelle")))


def open_connection(smoke, tree):
    hub = tagged(tree, "nav_hub")
    if hub is not None:
        smoke.tap(hub)
        tree = smoke.wait_ui(lambda root: tagged(root, "destination_connection") is not None, "hub_navigation")
        smoke.tap(tagged(tree, "destination_connection"))
        tree, _ = smoke.ui()
        find_scrolled(smoke, tree, "connection_token", "screen_hub")
    else:
        # Compatibilité des anciens APK : aucune hypothèse sur les coordonnées.
        for _ in range(6):
            tab = next((node for node in tree.iter("node") if node.get("text") in {"Connection", "Connexion"}), None)
            if tab is not None:
                smoke.tap(tab)
                break
            known = next((node for node in tree.iter("node") if node.get("text") in {"Chat", "Status", "Statut", "Agents"}), None)
            if known is None:
                raise RuntimeError("Navigation connexion absente")
            _, top, _, bottom = bounds_of(known)
            width = int(re.findall(r"(\d+)x\d+", smoke.adb("shell", "wm", "size"))[-1])
            y = (top + bottom) // 2
            smoke.adb("shell", "input", "swipe", str(width * 4 // 5), str(y), str(width // 5), str(y), "350")
            tree, _ = smoke.ui()
        else:
            raise RuntimeError("Connexion inaccessible après six défilements")
    return smoke.wait_ui(connection_fields, "connection_page")


def permission_snapshot(smoke, label):
    state = smoke.adb("shell", "dumpsys", "package", PACKAGE)
    (smoke.artifacts / f"{label}.txt").write_text(state)
    if f"Package [{PACKAGE}]" not in state or "requested permissions:" not in state:
        raise RuntimeError("État de permissions Android inexploitable")
    sensitive = {"RECORD_AUDIO", "CAMERA", "ACCESS_FINE_LOCATION", "ACCESS_COARSE_LOCATION",
                 "READ_CONTACTS", "READ_CALENDAR", "READ_MEDIA_IMAGES", "READ_MEDIA_VIDEO", "READ_EXTERNAL_STORAGE"}
    grants = re.findall(r"android\.permission\.([A-Z_]+):\s+granted=true", state)
    unexpected = sorted(sensitive.intersection(grants))
    if unexpected:
        raise RuntimeError(f"Permissions sensibles accordées sans action de ce scénario : {unexpected}")
    smoke.steps.append(label)
    print("PASS", label, flush=True)
    return {"source": "dumpsys package après réinitialisation de l'app", "granted_sensitive_permissions": []}


def device_controls(smoke, tree):
    # Les quatre destinations sont visitées sans action réseau ni saisie sensible.
    for destination in ("memory", "activity", "hub"):
        nav = tagged(tree, f"nav_{destination}")
        if nav is None:
            raise RuntimeError(f"Navigation nav_{destination} absente")
        smoke.tap(nav)
        tree = smoke.wait_ui(lambda root: tagged(root, f"screen_{destination}") is not None,
                             f"navigation_{destination}")
    tree, control = find_scrolled(smoke, tree, "destination_capabilities", "screen_hub")
    smoke.tap(control)
    tree = smoke.wait_ui(lambda root: tagged(root, "screen_capabilities") is not None, "phone_capabilities")
    tree, control = find_scrolled(smoke, tree, "device_file_choose", "screen_hub")
    smoke.tap(control)
    smoke.wait_ui(lambda root: any(node.get("package") in {"com.android.documentsui", "com.google.android.documentsui"}
                                  for node in root.iter("node")), "system_file_picker")
    smoke.adb("shell", "input", "keyevent", "KEYCODE_BACK")
    tree = smoke.wait_ui(lambda root: tagged(root, "screen_capabilities") is not None, "file_picker_cancelled")
    tree, control = find_scrolled(smoke, tree, "device_open_settings", "screen_hub")
    smoke.tap(control)
    tree = smoke.wait_ui(lambda root: tagged(root, "device_app_confirm") is not None
                        and tagged(root, "device_app_cancel") is not None, "settings_confirmation")
    smoke.tap(tagged(tree, "device_app_cancel"))
    tree = smoke.wait_ui(lambda root: tagged(root, "screen_capabilities") is not None
                        and tagged(root, "device_app_confirm") is None, "settings_cancelled")
    tree, control = find_scrolled(smoke, tree, "device_open_settings", "screen_hub")
    smoke.tap(control)
    tree = smoke.wait_ui(lambda root: tagged(root, "device_app_confirm") is not None, "settings_confirmation_again")
    smoke.tap(tagged(tree, "device_app_confirm"))
    smoke.wait_ui(lambda root: any(node.get("package") == "com.android.settings" for node in root.iter("node")),
                  "system_settings_opened")
    smoke.adb("shell", "input", "keyevent", "KEYCODE_BACK")
    smoke.wait_ui(lambda root: tagged(root, "screen_capabilities") is not None, "settings_returned")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("apk", type=Path)
    parser.add_argument("--serial", default=os.environ.get("ANDROID_SERIAL"))
    parser.add_argument("--artifacts", type=Path, default=Path("target/mobile-validation/android/runtime"))
    parser.add_argument("--timeout-scale", type=float, default=1.0,
                        help="Multiplie les délais adb/UI, entre 1 et 5 (TCG sans KVM : 3 ou 4)")
    parser.add_argument("--skip-install", action="store_true",
                        help="Réutilise seulement un APK installé dont le SHA256 est identique ; réinitialise ses données")
    parser.add_argument("--max-duration-seconds", type=int, default=600,
                        help="Budget global 30..1200s, indépendant de timeout-scale, puis 20s de diagnostics")
    parser.add_argument("--device-controls", action="store_true",
                        help="Exige les nouveaux contrôles Téléphone, SAF/annulation et Réglages/confirmation")
    args = parser.parse_args()
    if not math.isfinite(args.timeout_scale) or not 1 <= args.timeout_scale <= 5:
        raise ValueError("--timeout-scale doit être compris entre 1 et 5")
    if not 30 <= args.max_duration_seconds <= 1200:
        raise ValueError("--max-duration-seconds doit être compris entre 30 et 1200")
    sdk = os.environ.get("ANDROID_SDK_ROOT", os.environ.get("ANDROID_HOME"))
    adb = Path(sdk) / "platform-tools/adb" if sdk else shutil.which("adb")
    if not adb or not args.apk.is_file():
        raise ValueError("adb et un APK existant sont requis")
    args.artifacts.mkdir(parents=True, exist_ok=True)
    serial = args.serial
    smoke = AndroidSmoke(adb, serial, args.artifacts, args.timeout_scale, args.max_duration_seconds)
    if not serial:
        devices = smoke.adb("devices")
        candidates = [line.split()[0] for line in devices.splitlines()
                      if re.fullmatch(r"emulator-\d+\s+device", line.strip())]
        if len(candidates) != 1:
            raise ValueError("Sélectionner un émulateur unique avec --serial")
        serial = candidates[0]
        smoke.prefix.extend(["-s", serial])
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
              "max_duration_seconds": args.max_duration_seconds,
              "device_controls_requested": args.device_controls,
              "device_controls_tested": False,
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
        initial = smoke.wait_ui(lambda root: tagged(root, "screen_voice") is not None
                                or has_text(root, "Message to Hakille", "Message à Hakille"), "voice_home")
        if args.device_controls:
            if tagged(initial, "screen_voice") is None:
                raise RuntimeError("La nouvelle navigation Voix/Téléphone est requise pour --device-controls")
            result["permissions_at_launch"] = permission_snapshot(smoke, "no_sensitive_grants_at_launch")
        reveal_draft(smoke, initial)
        initial = smoke.wait_ui(lambda root: field_in(root) is not None
                                and field_in(root).get("enabled") == "true", "native_bridge_loaded")
        field = field_in(initial)
        if field is None:
            raise RuntimeError("Champ de brouillon absent")
        smoke.tap(field)
        smoke.adb("shell", "input", "text", DRAFT)
        hide_keyboard(smoke)
        smoke.wait_ui(draft_matches, "rust_draft_dispatch")
        smoke.adb("shell", "settings", "put", "system", "user_rotation", "1")
        tree = smoke.wait_ui(lambda root: root.get("rotation") == "1"
                             and (tagged(root, "screen_voice") is not None or draft_matches(root)), "landscape_layout")
        reveal_draft(smoke, tree)
        smoke.wait_ui(lambda root: root.get("rotation") == "1" and draft_matches(root), "rotation_preserves_draft")
        hide_keyboard(smoke)
        smoke.adb("shell", "input", "keyevent", "KEYCODE_HOME")
        time.sleep(1)
        smoke.adb("shell", "am", "start", "-W", "-n", ACTIVITY)
        tree = smoke.wait_ui(lambda root: tagged(root, "screen_voice") is not None or draft_matches(root), "voice_resumed")
        reveal_draft(smoke, tree)
        smoke.wait_ui(draft_matches, "background_return_preserves_draft")
        hide_keyboard(smoke)
        smoke.adb("shell", "settings", "put", "system", "user_rotation", "0")
        tree = smoke.wait_ui(lambda root: root.get("rotation") == "0"
                             and (tagged(root, "screen_voice") is not None or draft_matches(root)), "portrait_layout")
        reveal_draft(smoke, tree)
        smoke.wait_ui(lambda root: root.get("rotation") == "0" and draft_matches(root), "portrait_restored")
        hide_keyboard(smoke)
        smoke.adb("shell", "am", "force-stop", PACKAGE)
        smoke.adb("shell", "am", "start", "-W", "-n", ACTIVITY, timeout=120)
        tree = smoke.wait_ui(lambda root: tagged(root, "screen_voice") is not None or draft_matches(root), "voice_restarted")
        reveal_draft(smoke, tree)
        smoke.wait_ui(draft_matches, "process_restart_restores_draft")
        hide_keyboard(smoke)
        tree, _ = smoke.ui()
        tree = open_connection(smoke, tree)
        if args.device_controls:
            device_controls(smoke, tree)
            result["permissions_after_device_controls"] = permission_snapshot(smoke, "no_sensitive_grants_after_controls")
            result["device_controls_tested"] = True
        smoke.alive()
        result["passed"] = True
    except Exception as error:
        result["error"] = str(error)
        raise
    finally:
        smoke.begin_cleanup()
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
        result["elapsed_seconds"] = round(time.monotonic() - smoke.started, 2)
        (args.artifacts / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        if diagnostic_error:
            raise RuntimeError(diagnostic_error)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError, ET.ParseError) as error:
        print(f"ÉCHEC RUNTIME ANDROID : {error}", file=sys.stderr)
        sys.exit(1)
