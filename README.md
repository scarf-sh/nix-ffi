# nix-fci

A foreign command-line interface to [Nix](https://nixos.org/).

## What's a foreign command-line interface?

Many projects expose a [foreign function interface](https://en.wikipedia.org/wiki/Foreign_function_interface) to enable programmatic interfacing with the project, typically by exposing functions with the platform's C calling convention. Others, such as [git](https://git-scm.com/), discourage library use but expose appropriate primitives via the distribution's command line tools.

Currently, neither approach is appropriate for Nix. The Nix codebase is heavily based on C++ idioms, has global C++ state, and assumes a lot of common process startup routines have been executed, making it challenging to write a C interface that can be used like any other library. On the other hand, its command line tools are optimized for human or script usage, making certain tasks very non-ergonomic and others not exposed at all.

This project attempts to bridge that gap, by providing C++ tools, compiled against the Nix codebase, that are suitable for building an "FFI" for projects using Nix from other languages. It should only be used by programmers, and usually wrapped in appropriate language-specific bindings.

## Is this an official project?

No, and hopefully it never will be. If this proves useful and successful, it would be much better to upstream these capabilities directly into Nix itself, or even better refactor it to allow a proper FFI.

## Install

nix-fci is built as a typical cmake project. Its direct dependencies are:

- Nix (new enough to contain [this PR](https://github.com/NixOS/nix/pull/4486))
- pkg-config
- boost
- nlohmann_json

The output will contain `lib/nix/plugins/libnix-fci.so`.

## Usage

The FCI is exposed as a Nix plugin adding subcommands to the `nix` command. As such, your Nix settings (whether specified through config file or command line) must satisfy the following:

- `experimental-features` must include `nix-command`
- `plugin-files` must include the full path to `libnix-fci.so` or to a directory containing it.

You will then be able to run `nix fci` and its subcommands.

### testsuite

Usage: `nix fci testsuite --test-root DIR run --command PROG ARG ARG...`

Run `PROG` with the given `ARG`s in an environment where nix commands are isolated to a store rooted at `DIR`. Intended for project testsuites that want to confirm proper nix store interaction without polluting the user's store.
