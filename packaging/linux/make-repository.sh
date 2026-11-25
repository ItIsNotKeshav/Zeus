#!/usr/bin/env bash
# Signs Zeus's RPMs in place and makes the apt and dnf repository that serves
# them, with the .deb files beside them, as a static site:
#
#   bash packaging/linux/make-repository.sh KEY PACKAGES SITE URL
#
# KEY is the fingerprint of a secret key already in $GNUPGHOME (the release
# workflow imports ZEUS_LINUX_SIGNING_KEY), PACKAGES the folder holding the
# release's .deb and .rpm files, SITE an empty folder for the site and URL the
# address it is served from. Needs gpg, rpmsign, createrepo_c and apt-ftparchive.
#
#   SITE/zeus.asc       the public key both package managers check against
#   SITE/zeus.sources   apt's source, signed by that key (deb822)
#   SITE/zeus.repo      dnf's and zypper's repository file
#   SITE/apt/           a flat apt repository: InRelease and Release.gpg sign it
#   SITE/rpm/           an rpm-md repository: repomd.xml.asc signs it, and
#                       every RPM carries its own signature as well
set -euo pipefail
key=${1:?Give the fingerprint of the signing key}
packages=$(realpath "${2:?Give the folder of .deb and .rpm files}")
site=${3:?Give a folder for the site}
url=${4:?Give the address the site is served from}
url=${url%/}
here=$(cd "$(dirname "$0")" && pwd)
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$here/../../Cargo.toml" | head -n 1)

shopt -s nullglob
debs=("$packages"/*.deb)
rpms=("$packages"/*.rpm)
[ ${#debs[@]} -gt 0 ] && [ ${#rpms[@]} -gt 0 ] || { echo "No .deb or .rpm in $packages" >&2; exit 1; }

mkdir -p "$site/apt" "$site/rpm"
site=$(realpath "$site")
gpg --batch --armor --export "$key" > "$site/zeus.asc"

# Each RPM signed, then checked against the public key alone, in a database
# of its own so the machine's is left alone.
rpmsign --addsign \
    --define "_gpg_name $key" \
    --define "_gpg_sign_cmd_extra_args --batch --pinentry-mode loopback" \
    "${rpms[@]}"
checkdb="$(mktemp -d)"
rpm --dbpath "$checkdb" --initdb
rpmkeys --dbpath "$checkdb" --import "$site/zeus.asc"
rpmkeys --dbpath "$checkdb" --checksig "${rpms[@]}"
rm -rf "$checkdb"

cp "${rpms[@]}" "$site/rpm/"
createrepo_c --quiet "$site/rpm"
gpg --batch --yes --local-user "$key" --armor --detach-sign \
    --output "$site/rpm/repodata/repomd.xml.asc" "$site/rpm/repodata/repomd.xml"

cp "${debs[@]}" "$site/apt/"
(
    cd "$site/apt"
    apt-ftparchive packages . > Packages
    gzip -9 --keep Packages
    apt-ftparchive \
        -o APT::FTPArchive::Release::Origin=Zeus \
        -o APT::FTPArchive::Release::Label=Zeus \
        -o APT::FTPArchive::Release::Suite=stable \
        -o APT::FTPArchive::Release::Codename=stable \
        -o "APT::FTPArchive::Release::Architectures=amd64 arm64" \
        -o "APT::FTPArchive::Release::Description=Zeus, a native WhatsApp client" \
        release . > Release
    gpg --batch --yes --local-user "$key" --clearsign --output InRelease Release
    gpg --batch --yes --local-user "$key" --armor --detach-sign --output Release.gpg Release
)

cat > "$site/zeus.sources" <<EOF
Types: deb
URIs: $url/apt/
Suites: ./
Signed-By: /etc/apt/keyrings/zeus.asc
EOF

cat > "$site/zeus.repo" <<EOF
[zeus]
name=Zeus
baseurl=$url/rpm
enabled=1
gpgcheck=1
repo_gpgcheck=1
gpgkey=$url/zeus.asc
EOF

sed -e "s|@URL@|$url|g" -e "s|@VERSION@|$version|g" "$here/repository.html" > "$site/index.html"
# Served as it is, without Jekyll.
touch "$site/.nojekyll"
echo "Made the repository for Zeus $version in $site"
