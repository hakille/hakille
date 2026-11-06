#!/usr/bin/env python3
"""Commandes de validation bornées, avec progression même lorsque l'outil reste muet."""
import argparse
import math
import os
import signal
import subprocess
import sys
import time


def terminate(process):
    """Arrête uniquement le groupe créé pour cette commande (compilateurs inclus)."""
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            return
    elif process.poll() is None:
        process.terminate()
    try:
        process.wait(timeout=3)
    except subprocess.TimeoutExpired:
        pass
    # Le parent peut être sorti alors qu'un enfant garde ses tubes ouverts.
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    elif process.poll() is None:
        process.kill()
    process.wait(timeout=2)


def run_command(command, *, timeout, label, capture=False, binary=False, heartbeat=10):
    if not math.isfinite(timeout) or timeout <= 0:
        raise ValueError("Délai de commande positif et fini requis")
    started = time.monotonic()
    deadline = started + timeout
    print(f"START {label} (limite {timeout:.0f}s)", file=sys.stderr, flush=True)
    process = subprocess.Popen(command, stdout=subprocess.PIPE if capture else None,
                               stderr=subprocess.PIPE if capture else None,
                               text=not binary, start_new_session=os.name == "posix")
    try:
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise subprocess.TimeoutExpired(command, timeout)
            try:
                if capture:
                    stdout, stderr = process.communicate(timeout=min(heartbeat, remaining))
                else:
                    process.wait(timeout=min(heartbeat, remaining))
                    stdout = stderr = None
                break
            except subprocess.TimeoutExpired:
                elapsed = time.monotonic() - started
                print(f"RUNNING {label} ({elapsed:.0f}s / {timeout:.0f}s)", file=sys.stderr, flush=True)
        if process.returncode:
            raise subprocess.CalledProcessError(process.returncode, command, stdout, stderr)
        print(f"DONE {label} ({time.monotonic() - started:.1f}s)", file=sys.stderr, flush=True)
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
    except BaseException:
        terminate(process)
        raise
    finally:
        if process.stdout:
            process.stdout.close()
        if process.stderr:
            process.stderr.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--timeout", required=True, type=float)
    parser.add_argument("--label", required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("commande manquante après --")
    run_command(command, timeout=args.timeout, label=args.label)


if __name__ == "__main__":
    try:
        main()
    except subprocess.TimeoutExpired as error:
        print(f"ÉCHEC : échéance atteinte ({error.timeout}s)", file=sys.stderr)
        sys.exit(124)
    except subprocess.CalledProcessError as error:
        print(f"ÉCHEC : commande terminée avec {error.returncode}", file=sys.stderr)
        sys.exit(error.returncode if error.returncode > 0 else 1)
    except (OSError, ValueError) as error:
        print(f"ÉCHEC : {error}", file=sys.stderr)
        sys.exit(1)
