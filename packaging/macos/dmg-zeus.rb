# frozen_string_literal: true

# Zeus's DMG, as dmg.rb makes ZapFast's, but laid out: only Zeus.app and an
# Applications link on a background with an arrow between them.
#
# Finder lays the window out (AppleScript on a writable copy), as create-dmg
# does: a .DS_Store written by hand (dmgbuild) kept its icon positions on
# macOS 15 but lost the background. Icon positions are centres, in points,
# and must match the arrow drawn into dmg-background.png (y = 190, x = 165
# and 495).
require "fileutils"
require "tmpdir"

payload, output = ARGV
abort "usage: dmg-zeus.rb PAYLOAD OUTPUT.dmg" unless payload && output
abort "output already exists: #{output}" if File.exist?(output)
app = Dir.glob(File.join(payload, "*.app")).first or abort "no .app in #{payload}"
volume = File.basename(app, ".app")
abort "a volume named #{volume} is already mounted" if File.exist?("/Volumes/#{volume}")
here = __dir__

def run(*command)
  system(*command) or abort "failed: #{command.join(" ")}"
end

Dir.mktmpdir("zeus-dmg-") do |work|
  staging = File.join(work, "staging")
  FileUtils.mkdir_p(File.join(staging, ".background"))
  FileUtils.cp_r(app, staging, preserve: true)
  File.symlink("/Applications", File.join(staging, "Applications"))
  run("tiffutil", "-cathidpicheck", File.join(here, "dmg-background.png"),
    File.join(here, "dmg-background@2x.png"),
    "-out", File.join(staging, ".background", "background.tiff"))

  # As in dmg.rb: hdiutil's own size estimate has run short on GitHub's
  # runners, so give it the payload's size with room to spare, and retry.
  kilobytes = Integer(`du -sk "#{staging}"`.split.first)
  megabytes = kilobytes * 13 / 10 / 1024 + 64
  writable = File.join(work, "writable.dmg")
  created = 3.times.any? do |attempt|
    FileUtils.rm_f(writable)
    sleep(5 * attempt)
    system("hdiutil", "create", "-volname", volume, "-srcfolder", staging,
      "-size", "#{megabytes}m", "-format", "UDRW", writable)
  end
  unless created
    system("df", "-h", Dir.tmpdir, File.dirname(File.expand_path(output)))
    abort "DMG creation failed"
  end

  attached = `hdiutil attach "#{writable}" -readwrite -noverify -noautoopen`
  device = attached[%r{^(/dev/disk\d+)}, 1] or abort "attach failed: #{attached}"
  begin
    script = <<~APPLESCRIPT
      tell application "Finder"
        tell disk "#{volume}"
          open
          set current view of container window to icon view
          set toolbar visible of container window to false
          set statusbar visible of container window to false
          set the bounds of container window to {200, 120, 860, 520}
          set viewOptions to the icon view options of container window
          set arrangement of viewOptions to not arranged
          set icon size of viewOptions to 128
          set text size of viewOptions to 13
          set background picture of viewOptions to file ".background:background.tiff"
          set position of item "#{File.basename(app)}" of container window to {165, 190}
          set position of item "Applications" of container window to {495, 190}
          close
          open
          update without registering applications
          delay 2
          close
        end tell
      end tell
    APPLESCRIPT
    # Finder can be slow to answer on a fresh runner; try again before failing.
    laid_out = 3.times.any? do |attempt|
      sleep(5 * attempt)
      system("osascript", "-e", script)
    end
    abort "Finder layout failed" unless laid_out
    run("sync")
  ensure
    3.times.any? { |attempt| sleep(2 * attempt); system("hdiutil", "detach", device) } or
      system("hdiutil", "detach", device, "-force")
  end

  run("hdiutil", "convert", writable, "-format", "UDZO", "-imagekey", "zlib-level=9",
    "-o", output)
end
abort "DMG verification failed" unless system("hdiutil", "verify", output)
