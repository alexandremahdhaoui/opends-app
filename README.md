# opends-app

The Rust app for OpenDS. Reads a DualSense or DualShock 4 on Windows. Maps buttons to keys and mouse. Feeds a virtual Xbox pad so games see analog input over XInput.

Part of the OpenDS project. OpenDS is a Rust tool for Windows. It reads a Sony pad. It maps the pad to keyboard and mouse. It presents the pad to games as an Xbox pad. It is independent from DS4Windows.

## What lives here

- `OpenDS.exe`. The main app. A tray app with a GUI. Shows pad status. Maps buttons. Runs the virtual pad feed.
- `OpenDS-Setup.exe`. The installer. Installs the driver. Signs it. Adds an uninstall entry. Repairs a broken install.
- Hexagonal layout. Adapters talk to Windows. Controllers hold logic. Drivers wire adapters to controllers.

## Build and test

This repo builds on Linux and cross compiles to Windows.

```sh
forge build
forge test-all
```

`forge test-all` is the gate. Not `cargo test`. A Windows only stage runs the cross built tests on real Windows through WSL.

## Depends on

- `opends-core` for the pure pad decode and mapping logic.
- `opends-spec` for the config schema and the driver protocol.
- `opends-uhid` for the driver this app installs and talks to.

## License

Apache License 2.0.
