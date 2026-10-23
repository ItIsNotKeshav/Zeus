#!/usr/bin/env bash
# Rebuilds the README pictures: demo screenshots, each framed in a MacBook.
# Needs a release demo build (cargo build --release --features demo) and xvfb.
#   scripts/readme-shots.sh [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-docs/readme}"
raw="$(mktemp -d)"
mkdir -p "$out"

# name | --demo-page flags
shots=(
  "chat-dark|chat,nothing"
  "chat-light|chat,nothing-light"
  "chat-dracula|chat,dracula"
  "chat-catppuccin|chat,catppuccin"
  "chat-tokyo-night|chat,tokyo-night"
  "chat-messages|chat,messages"
  "settings-appearance|settings,nothing,section-0"
  "settings-chats|settings,nothing-light,section-1"
  "settings-notifications|settings,dracula,section-2"
  "settings-calls|voice-settings,nothing"
  "settings-privacy|settings,nord,section-4"
  "doodles|chat,nothing-light,doodles-accent,doodle-style-travel,doodle-intensity-200"
  "composer|chat,nothing,composer-tools"
  "reactions|chat,nothing,reactions"
)

for entry in "${shots[@]}"; do
  name="${entry%%|*}"; page="${entry#*|}"
  xvfb-run -a -s "-screen 0 1600x1000x24" \
    ./target/release/zeus --demo-size 1600x1000 --demo-shot "$raw/$name.png" --demo-page "$page"
  bg=light; [[ "$name" == *light* ]] || bg=dark
  python3 -I scripts/macbook-mockup.py "$raw/$name.png" "$out/$name.png" --bg "$bg"
done
rm -rf "$raw"
