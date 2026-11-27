#!/usr/bin/env bash
# `apt-get update` on a GitHub-hosted runner, without the runner's own
# third-party sources. The images preinstall packages.microsoft.com, which no
# job here uses; when it answers 403, apt-get update fails the whole job.
# Only for disposable CI runners: it deletes apt source files system-wide.
set -euo pipefail

if [[ "${GITHUB_ACTIONS:-}" != "true" ]]; then
    echo "ci-apt-update.sh: refusing to run outside GitHub Actions" >&2
    exit 1
fi

shopt -s nullglob
for source in /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; do
    if grep -q 'packages\.microsoft\.com' "$source"; then
        echo "==> Removing unused apt source $source"
        sudo rm -f -- "$source"
    fi
done

sudo apt-get update
