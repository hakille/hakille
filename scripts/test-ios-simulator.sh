#!/usr/bin/env bash
# Exécute réellement XCTest ; aucun simulateur absent n'est traité comme succès.
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
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
    if [[ "$booted_here" == true ]]; then xcrun simctl shutdown "$device" >/dev/null 2>&1 || true; fi
}
trap cleanup EXIT
if [[ "$state" != Booted ]]; then
    xcrun simctl boot "$device"
    booted_here=true
fi
python3 - "$device" <<'PY'
import subprocess,sys
subprocess.run(['xcrun','simctl','bootstatus',sys.argv[1],'-b'], check=True, timeout=240)
PY
cd "$repo_root/mobile/ios"
xcodebuild -project Hakille.xcodeproj -scheme Hakille \
    -destination "platform=iOS Simulator,id=$device" -destination-timeout 60 \
    -derivedDataPath build CODE_SIGNING_ALLOWED=NO build-for-testing \
    2>&1 | tee "$artifacts/build.log"
xcodebuild -project Hakille.xcodeproj -scheme Hakille \
    -destination "platform=iOS Simulator,id=$device" -destination-timeout 60 \
    -derivedDataPath build -parallel-testing-enabled NO \
    -resultBundlePath "$artifacts/HakilleTests.xcresult" \
    CODE_SIGNING_ALLOWED=NO test-without-building \
    2>&1 | tee "$artifacts/tests.log"
[[ -d "$artifacts/HakilleTests.xcresult" ]] || { echo 'Résultat XCTest manquant.' >&2; exit 1; }
xcrun xcresulttool get test-results summary --path "$artifacts/HakilleTests.xcresult" \
    > "$artifacts/summary.json"
python3 - "$artifacts/summary.json" <<'PY'
import json,sys
summary = json.load(open(sys.argv[1]))
if summary.get('passedTests', 0) < 1 or summary.get('failedTests', 0) != 0:
    raise SystemExit('XCTest ne confirme aucun test réussi, ou contient des échecs.')
print(f"XCTest exécutés et réussis : {summary['passedTests']}")
PY
