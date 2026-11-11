# Releasing

## A release

1. Set the new version in `Cargo.toml` (`version = "0.2.0"`), run `cargo build` so
   `Cargo.lock` follows, rename `## Unreleased` in `CHANGELOG.md` to the version and
   the day (`## 0.2.0 (2026-11-08)`) with a new, empty `## Unreleased` above it, and
   commit. `Cargo.toml` is the only place the
   version lives: the bundle script writes it into the app's Info.plist.
2. Tag it and push the tag:

   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```

The Release workflow (`.github/workflows/release.yml`) then runs the tests, builds
`Odeion.app` for Apple silicon and Intel in one, and publishes `Odeion-0.2.0.zip` (and
the same as `Odeion-macOS.zip`) on the repo's Releases page; a second job then builds
Windows and adds `Odeion-0.2.0-Windows.zip` and `Odeion-Windows.zip`. A tag that doesn't match `Cargo.toml` fails the run before
anything is built. The release notes are the changelog's section for the version, then
how to install; without a section the run warns and the notes only say how to install.

A last job signs `Odeion-macOS.zip` and `Odeion-Windows.zip` for Odeion's updater and
adds `Odeion-macOS.zip.minisig` and `Odeion-Windows.zip.minisig`, a few minutes after
the release appears. Until then Check for Updates offers the download by hand.

A tag on a mistake can go again: delete the release and the tag on GitHub
(`gh release delete v0.2.0 --cleanup-tag`), fix, tag again.

## The update key

Check for Updates installs a release itself only when its zip is signed with Odeion's
release key, a [minisign](https://jedisct1.github.io/minisign/) key without a password.
Its public half is `PUBLIC_KEY` in `src/update.rs`, its secret half the repository
secret `MINISIGN_KEY` (the whole key file). Keep a copy of the key file in a password
manager: GitHub never shows a secret again.

A lost or leaked key is replaced: `minisign -G -W -p odeion.pub -s odeion.key`, the
new public key (the second line of `odeion.pub`) into `PUBLIC_KEY`,
`gh secret set MINISIGN_KEY < odeion.key`, and the key file into the password manager.
Copies from before the change refuse updates signed with the new key, so their users
download the next release by hand once.

## Signing

Without signing secrets the app is only ad-hoc signed. It runs, but macOS blocks a
downloaded copy the first time and people have to allow it in System Settings, Privacy
& Security, Open Anyway (the release notes say so). Signed and notarized, it opens like
any other app. That needs a paid Apple Developer Program membership (99 USD a year).

Once:

1. **Join** the Apple Developer Program at developer.apple.com, as an individual.
2. **Make a Developer ID Application certificate**: Xcode, Settings, Accounts, your
   Apple ID, Manage Certificates, + and "Developer ID Application". (Or at
   developer.apple.com, Certificates, with a signing request from Keychain Access.)
3. **Export it** from Keychain Access (My Certificates, the "Developer ID Application:
   ..." entry with its key, right-click, Export) as `odeion.p12`, with a password.
4. **Make an app-specific password** at account.apple.com, Sign-In and Security,
   App-Specific Passwords, named for instance "Odeion notarization". Notarization uses it,
   never your real password.
5. **Find your team ID**: developer.apple.com, Account, Membership details (ten
   letters and digits). The signing identity is the certificate's full name, e.g.
   `Developer ID Application: Your Name (TEAMID1234)`; `security find-identity -v -p
   codesigning` lists it.
6. **Give them to GitHub** as repository secrets:

   ```sh
   base64 -i odeion.p12 | gh secret set MACOS_CERTIFICATE
   gh secret set MACOS_CERTIFICATE_PASSWORD      # the .p12's password
   gh secret set MACOS_SIGNING_IDENTITY          # Developer ID Application: ... (TEAMID)
   gh secret set APPLE_ID                        # your Apple ID email
   gh secret set APPLE_TEAM_ID                   # TEAMID1234
   gh secret set APPLE_APP_PASSWORD              # the app-specific password
   ```

   Then delete `odeion.p12` from disk.

From the next tag on, the workflow signs with the hardened runtime, sends the app to
Apple's notary service (a few minutes), staples the ticket to it and publishes that.

To check a downloaded release: `spctl -a -vv /Applications/Odeion.app` says "accepted,
source=Notarized Developer ID" for a signed one.
