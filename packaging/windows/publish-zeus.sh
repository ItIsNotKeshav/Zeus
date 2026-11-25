#!/usr/bin/env bash
# Starts a Zeus release: tags the commit on main and runs zeus-release.yml,
# which builds every platform on GitHub's machines, signs the files and
# checksums.txt, and publishes the release once all of it is there. Run it
# through `packaging\windows\build-zeus.cmd publish`, or by itself from Git
# Bash; nothing is built here.
#
# The update key lives in the repository's ZEUS_UPDATE_SIGNING_KEY secret.
# Keep a backup of it (~/.zeus/update-signing-key.pem): installed copies only
# accept updates it signed.
set -euo pipefail
cd "$(dirname "$0")/../.."

repo=ItIsNotKeshav/Zeus
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
tag="v$version"
notes="packaging/release-notes/zeus-v$version.md"
commit=$(git rev-parse HEAD)

# The app's Release notes link opens this page; GitHub's own notes for direct
# commits say nothing but "Full Changelog".
[ -f "$notes" ] || { echo "Write $notes first." >&2; exit 1; }

# The tag names what is built, so that commit must be the whole of it, and
# already on main.
git diff --quiet HEAD || { echo "Commit your changes first." >&2; exit 1; }
git fetch -q origin main
git merge-base --is-ancestor "$commit" origin/main ||
    { echo "Push $commit to main first." >&2; exit 1; }

# Installed copies ignore a release that is not newer than theirs.
latest=$(gh release view --repo "$repo" --json tagName --jq .tagName 2>/dev/null || true)
if [ -n "$latest" ] && [ "$(printf '%s\n%s\n' "${latest#v}" "$version" | sort -V | tail -n 1)" != "$version" ] ||
   [ "$latest" = "$tag" ]; then
    echo "$tag is not above the latest release, $latest: bump the version." >&2
    exit 1
fi

if git ls-remote --exit-code --tags origin "refs/tags/$tag" > /dev/null; then
    echo "$tag is already on GitHub; rerunning the release workflow for it."
else
    git tag "$tag" "$commit"
    git push -q origin "refs/tags/$tag"
fi
gh workflow run zeus-release.yml --repo "$repo" --ref main -f tag="$tag"
echo "Started the release of Zeus $version. Follow it with:"
echo "  gh run watch --repo $repo \$(gh run list --repo $repo --workflow zeus-release.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
echo "With SignPath set up, approve its two signing requests (zeus.exe, then the installer) on SignPath's site."
