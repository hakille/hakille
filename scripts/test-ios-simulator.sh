#!/usr/bin/env bash
# Exécute réellement XCTest ; aucun simulateur absent n'est traité comme succès.
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
runner=(python3 "$repo_root/scripts/mobile_process.py")
artifacts=${1:-"$repo_root/target/mobile-validation/ios"}
[[ $# -le 1 ]] || { printf 'Usage : %s [répertoire-résultats]\n' "$0" >&2; exit 2; }
[[ $(uname -s) == Darwin ]] || { echo 'Erreur : XCTest exige macOS avec Xcode.' >&2; exit 1; }
for tool in xcrun xcodebuild python3; do
    command -v "$tool" >/dev/null || { echo "Outil absent : $tool" >&2; exit 1; }
done
[[ -d "$repo_root/mobile/ios/Hakille.xcodeproj" ]] || { echo 'Générer le projet avec xcodegen avant les tests.' >&2; exit 1; }
mkdir -p "$artifacts"
artifacts=$(cd "$artifacts" && pwd -P)
[[ ! -e "$artifacts/HakilleTests.xcresult" ]] || { echo 'Le xcresult existe déjà ; choisir un nouveau répertoire.' >&2; exit 1; }
"${runner[@]}" --timeout 30 --label 'Liste simulateurs iOS' -- \
    xcrun simctl list devices available --json > "$artifacts/devices.json"
device=$(python3 - "$artifacts/devices.json" <<'PY'
import json, sys
devices = json.load(open(sys.argv[1]))['devices']
candidates = []
for runtime, entries in devices.items():
    if '.iOS-' not in runtime:
        continue
    version = tuple(int(p) for p in runtime.rsplit('.iOS-', 1)[1].split('-'))
    for entry in entries:
        if entry.get('isAvailable') and entry['name'].startswith('iPhone'):
            candidates.append((version, entry['name'], entry['udid']))
if not candidates:
    raise SystemExit('Aucun simulateur iPhone iOS disponible ; XCTest non exécuté.')
print(sorted(candidates, reverse=True)[0][2])
PY
)
printf 'Simulateur sélectionné : %s\n' "$device" | tee "$artifacts/destination.txt"
booted_here=false
state=$(python3 - "$artifacts/devices.json" "$device" <<'PY'
import json,sys
print(next(e['state'] for es in json.load(open(sys.argv[1]))['devices'].values() for e in es if e['udid'] == sys.argv[2]))
PY
)
cleanup() {
    if [[ "$booted_here" == true ]]; then
        "${runner[@]}" --timeout 30 --label 'Arrêt simulateur iOS' -- \
            xcrun simctl shutdown "$device" >/dev/null || echo 'Arrêt du simulateur non confirmé.' >&2
    fi
}
trap cleanup EXIT
if [[ "$state" != Booted ]]; then
    "${runner[@]}" --timeout 30 --label 'Démarrage simulateur iOS' -- xcrun simctl boot "$device"
    booted_here=true
fi
"${runner[@]}" --timeout 240 --label 'Attente démarrage iOS' -- \
    xcrun simctl bootstatus "$device" -b 2>&1 | tee "$artifacts/boot.log"
cd "$repo_root/mobile/ios"
"${runner[@]}" --timeout 900 --label 'Compilation iOS et XCTest' -- \
    xcodebuild -project Hakille.xcodeproj -scheme Hakille \
    -destination "platform=iOS Simulator,id=$device" -destination-timeout 60 \
    -derivedDataPath build CODE_SIGNING_ALLOWED=NO build-for-testing \
    2>&1 | tee "$artifacts/build.log"
"${runner[@]}" --timeout 600 --label 'Exécution XCTest iOS' -- \
    xcodebuild -project Hakille.xcodeproj -scheme Hakille \
    -destination "platform=iOS Simulator,id=$device" -destination-timeout 60 \
    -derivedDataPath build -parallel-testing-enabled NO \
    -resultBundlePath "$artifacts/HakilleTests.xcresult" \
    CODE_SIGNING_ALLOWED=NO test-without-building \
    2>&1 | tee "$artifacts/tests.log"
[[ -d "$artifacts/HakilleTests.xcresult" ]] || { echo 'Résultat XCTest manquant.' >&2; exit 1; }
"${runner[@]}" --timeout 120 --label 'Lecture résultat XCTest' -- \
    xcrun xcresulttool get test-results summary --path "$artifacts/HakilleTests.xcresult" \
    > "$artifacts/summary.json"
python3 - "$artifacts/summary.json" <<'PY'
import json,sys
summary = json.load(open(sys.argv[1]))
if summary.get('passedTests', 0) < 1 or summary.get('failedTests', 0) != 0:
    raise SystemExit('XCTest ne confirme aucun test réussi, ou contient des échecs.')
print(f"XCTest exécutés et réussis : {summary['passedTests']}")
PY

# Réouvrir l'application hors du runner XCTest, puis conserver deux vues pour
# revue humaine. Une capture n'est pas une assertion de qualité du rendu.
python3 - "$device" "$artifacts" "$repo_root/scripts" <<'PY'
import json, subprocess, sys, time
from pathlib import Path
sys.path.insert(0, sys.argv[3])
from mobile_process import run_command

device = sys.argv[1]
directory = Path(sys.argv[2]) / 'screenshots'
directory.mkdir(exist_ok=True)
proof = {'device': device, 'bundle_id': 'dev.hakille.mobile', 'rendering_asserted': False,
         'status': 'pending', 'images': []}
deadline = time.monotonic() + 40
try:
    launch = run_command(['xcrun', 'simctl', 'launch', '--terminate-running-process', device, 'dev.hakille.mobile'],
                         timeout=min(15, deadline - time.monotonic()), label='Ouverture UI iOS', capture=True)
    proof['launch_result'] = launch.stdout.strip()
    for delay, name in [(2, 'home-early.png'), (3, 'home-settled.png')]:
        if deadline - time.monotonic() <= delay:
            raise TimeoutError('Budget des captures iOS épuisé')
        time.sleep(delay)
        path = directory / name
        run_command(['xcrun', 'simctl', 'io', device, 'screenshot', '--type=png', str(path)],
                    timeout=min(10, deadline - time.monotonic()), label=f'Capture iOS {name}')
        with path.open('rb') as image:
            if image.read(8) != b'\x89PNG\r\n\x1a\n':
                raise ValueError('Capture iOS PNG invalide')
        proof['images'].append(name)
    proof['status'] = 'captured'
except (OSError, ValueError, subprocess.SubprocessError) as error:
    proof['status'] = 'failed'
    proof['error'] = str(error)
    print(f'Capture UI iOS non confirmée : {error}', file=sys.stderr, flush=True)
finally:
    (directory / 'capture.json').write_text(json.dumps(proof, ensure_ascii=False, indent=2) + '\n')
PY
