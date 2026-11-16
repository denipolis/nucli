# Nucli Uplink

Nucli Uplink is the iPhone and iPad app for Nucli. It is a remote control for the computers running Nucli: watch, type into and start their agents, browse files, review changes and commit, from your phone. Everything runs on your computers. The phone connects to them directly and end-to-end encrypted, with no server in between.

There is no App Store build. You build it on a Mac and install it on your own phone. What the app does is described in [SPEC.md](../docs/SPEC.md#nucli-uplink).

## What you need

- A Mac with [Xcode](https://apps.apple.com/app/xcode/id497799835) and a recent stable Rust installed with `rustup`
- An Apple ID. A free one works, but then the app stops opening after 7 days until you install it again. A paid Apple Developer account keeps it working for a year.
- An iPhone or iPad with iOS 17 or later, and a cable

## Installing on your phone

Once:

1. Open Xcode, go to Xcode › Settings › Accounts and add your Apple ID.
2. Connect the phone with the cable, unlock it and tap Trust.
3. On the phone, turn on Settings › Privacy & Security › Developer Mode and restart it when asked.
4. Pick a bundle ID of your own, like `com.yourname.nucli.uplink`. Apple ties a bundle ID to one account, so you cannot use the one from this repo.
5. In Xcode, create a new project: File › New › Project › iOS › App. Name it `uplink`, choose your Apple ID as the team and set the organization identifier to the rest of your bundle ID (`com.yourname.nucli`). Pick your phone at the top of the window and press Run. Xcode creates a provisioning profile for the bundle ID and installs an empty app. You can keep this project anywhere.

Then build and install Nucli Uplink:

1. `git clone https://github.com/denipolis/nucli.git`
2. `cd nucli`
3. `xcrun devicectl list devices` and copy your phone's identifier
4. `BUNDLE_ID=com.yourname.nucli.uplink DEVICE=<identifier> ./scripts/mobile.sh device`

It replaces the empty app with Nucli Uplink. With a free Apple ID, the first launch says the developer is not trusted: open Settings › General › VPN & Device Management, tap your Apple ID and trust it.

If you have more than one signing certificate, pick one with `IDENTITY=<certificate name>` (`security find-identity -v -p codesigning` lists them).

With a free Apple ID, run the `uplink` project from Xcode again every 7 days to renew the profile, then run the script again.

## Pairing

1. On your computer, turn on Uplink in Nucli: Settings › Uplink.
2. Open Nucli Uplink and allow Local Network access when asked.
3. Tap your computer under computers nearby, or scan the QR code from Connect a device in Nucli.
4. Check that both show the same six digits and allow the phone on the computer.

## Simulator and desktop

- `./scripts/mobile.sh` builds the app for the iOS Simulator. With `SIMULATOR=<identifier>` (from `xcrun simctl list devices`) it installs and starts it there. The simulator has no camera, so pair by name or pairing link.
- `cargo run -p nucli-uplink` opens the same app in a phone-sized window on any desktop.
