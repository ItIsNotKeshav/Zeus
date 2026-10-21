#!/usr/bin/env bash
# Publishes the installer build-zeus.cmd made as the GitHub release installed
# copies of Zeus update themselves from: the setup program, checksums.txt and
# its Ed25519 signature. Run through `packaging\windows\build-zeus.cmd publish`.
#
# The private key stays outside the repository, at
# ~/.zeus/update-signing-key.pem (ZEUS_UPDATE_SIGNING_KEY names another).
# Keep a backup of it: installed copies only accept updates it signed.
set -euo pipefail
cd "$(dirname "$0")/../.."

key="${ZEUS_UPDATE_SIGNING_KEY:-$HOME/.zeus/update-signing-key.pem}"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
setup="zeus-v$version-x86_64-pc-windows-msvc-setup.exe"
commit=$(git rev-parse HEAD)

# The tag names the commit the installer was built from, so that commit must
# be the whole of what was built, and already on main.
git diff --quiet HEAD || { echo "Commit your changes first." >&2; exit 1; }
git fetch -q origin main
git merge-base --is-ancestor "$commit" origin/main ||
    { echo "Push $commit to main first." >&2; exit 1; }
[ -f "dist/$setup" ] || { echo "No dist/$setup: run build-zeus.cmd." >&2; exit 1; }

# Any other key would publish an update every installed copy refuses.
public=$(openssl pkey -in "$key" -pubout -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n')
[ "$public" = "$(tr -d '[:space:]' < assets/zeus-update-public-key.hex)" ] ||
    { echo "$key is not the key Zeus trusts." >&2; exit 1; }

(
    cd dist
    sha256sum "$setup" > checksums.txt
    openssl pkeyutl -sign -inkey "$key" -rawin -in checksums.txt -out checksums.txt.sig
)

gh release create "v$version" --repo ItIsNotKeshav/zapfast --target "$commit" \
    --title "Zeus $version" --generate-notes --latest \
    "dist/$setup" dist/checksums.txt dist/checksums.txt.sig
