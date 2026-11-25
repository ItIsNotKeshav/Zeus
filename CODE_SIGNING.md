# Code signing policy

Zeus's Windows program and installer are signed so Windows can show who
published them. Free code signing provided by
[SignPath.io](https://about.signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

## What is signed

- **Windows:** `zeus.exe` and the installer,
  `zeus-v<version>-x86_64-pc-windows-msvc-setup.exe`, with the SignPath
  Foundation certificate.
- **Linux:** each `.rpm`, the apt and dnf package repository, and
  `packaging-checksums.txt` (as `packaging-checksums.txt.asc`), with Zeus's
  own GPG key, published as `zeus.asc` beside the repository.
- **Updates:** `checksums.txt`, which installed copies check every update
  against, with Zeus's Ed25519 update key
  (`assets/zeus-update-public-key.hex`). A copy refuses an update this key did
  not sign.

Only files built from this repository's source are signed. Every release is
built on GitHub Actions by [`zeus-release.yml`](.github/workflows/zeus-release.yml)
from a tag on `main`, never on a personal computer, and nothing is signed that
the workflow did not build in the same run. Libraries Zeus is built from
(listed in [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md)) are compiled
into it from their published source.

## Team roles

- Committers and reviewers: [Keshav](https://github.com/ItIsNotKeshav)
- Approvers: [Keshav](https://github.com/ItIsNotKeshav)

Changes from anyone else arrive as pull requests and are reviewed before
they are merged. Every signing request is approved by hand, one release at a
time. Everyone in these roles must use multi-factor authentication for GitHub
and SignPath.

## Privacy

Zeus connects only to the services below, and only for what each says:

- **WhatsApp's servers**, for the account you link. Your messages go to
  WhatsApp end to end encrypted, as from WhatsApp's own apps, and your chats
  are kept in an encrypted archive on your computer. WhatsApp's
  [privacy policy](https://www.whatsapp.com/legal/privacy-policy) applies.
- **GitHub**, once a day, to see whether a newer Zeus is out, and to download
  it when you choose **Restart to update**. GitHub's
  [privacy statement](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement)
  applies.
- **GIPHY**, only while the picker's GIF tab is open, for its trending GIFs
  and your searches. GIPHY's
  [privacy policy](https://support.giphy.com/hc/en-us/articles/360032872931)
  applies.
- **The page a link points to**, when you type a link into a message, to show
  its preview the way WhatsApp does (YouTube's own preview address for YouTube
  links). Turning off link previews in WhatsApp's privacy settings stops this.

Zeus has no telemetry, no analytics and no server of its own.

## Reporting

Report a signed file you think is wrong, or a security problem, in the
repository's [issues](https://github.com/ItIsNotKeshav/Zeus/issues).
