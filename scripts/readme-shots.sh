#!/usr/bin/env bash
# Rebuilds the README pictures: demo screenshots, each framed in a MacBook.
# Needs a release demo build (cargo build --release --features demo) and xvfb.
#   scripts/readme-shots.sh [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."
out="${1:-docs/readme}"
bin="${ZEUS_BIN:-./target/release/zeus}"
raw="$(mktemp -d)"
mkdir -p "$out"

# name | --demo-page flags
shots=(
  "chat-dark|chat,zeus,mode-dark,loaded"
  "chat-light|chat,zeus,mode-light,loaded"
  "chat-dracula|chat,dracula,mode-dark,loaded"
  "chat-catppuccin|chat,catppuccin,mode-dark,loaded"
  "chat-nord|chat,nord,mode-dark,loaded"
  "chat-messages|chat,messages,mode-dark,loaded"
  "doodles|chat,zeus,mode-light,loaded,doodles-accent,doodle-style-travel,doodle-intensity-200"
  "composer|chat,zeus,mode-dark,loaded,composer-tools"
  "reactions|chat,zeus,mode-dark,loaded,reactions"
  "pdf|chat,catppuccin,mode-dark,loaded,pdf"
  "chat-search|chat,zeus,mode-light,loaded,chat-search"
  "settings-appearance|settings,zeus,mode-dark,section-0"
  "settings-chats|settings,zeus,mode-light,section-1"
  "settings-notifications|settings,dracula,mode-dark,section-2"
  "settings-camera|camera-settings,zeus,mode-dark"
  "settings-app-icon|zeus,mode-light,settings-search=App icon"
)

for entry in "${shots[@]}"; do
  name="${entry%%|*}"; page="${entry#*|}"
  xvfb-run -a -s "-screen 0 1600x1000x24" \
    "$bin" --demo-size 1600x1000 --demo-shot "$raw/$name.png" --demo-page "$page"
  bg=dark; [[ "$page" == *mode-light* ]] && bg=light
  python3 -I scripts/macbook-mockup.py "$raw/$name.png" "$out/$name.png" --bg "$bg"
done
rm -rf "$raw"
