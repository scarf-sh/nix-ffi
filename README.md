# nix-ffi

A foreign function interface to [Nix](https://nixos.org/).

## Is this an official project?

No, and hopefully it never will be. If this proves useful and successful, it would be much better to upstream these capabilities directly into Nix itself, or even better refactor it to allow a proper FFI.

## Install

nix-ffi is built as a typical cmake project. Its direct dependencies are:

- Nix (new enough to contain [this PR](https://github.com/NixOS/nix/pull/4486))
- pkg-config
- boost
- nlohmann_json

The output will contain `lib/nix/plugins/libnix-ffi.so`.

## Usage

The FFI is exposed via a Nix plugin that adds the `ffi-helper` subcommand to the `nix` command. `nix ffi-helper` exposes a [protocol](protocol.org) over stdio to access native Nix functionality.

For now, the protocol is unstable, and thus there is tight coupling between your language's bindings to the helper and the plugin version used. Until stability, it is expected that any bindings will be included in this repo and end users should use those libraries instead of calling the helper directly.
