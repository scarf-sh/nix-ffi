# nix-fci

A foreign command-line interface to [Nix](https://nixos.org/).

## What's a foreign command-line interface?

Many projects expose a [foreign function interface](https://en.wikipedia.org/wiki/Foreign_function_interface) to enable programmatic interfacing with the project, typically by exposing functions with the platform's C calling convention. Others, such as [git](https://git-scm.com/), discourage library use but expose appropriate primitives via the distribution's command line tools.

Currently, neither approach is appropriate for Nix. The Nix codebase is heavily based on C++ idioms, has global C++ state, and assumes a lot of common process startup routines have been executed, making it challenging to write a C interface that can be used like any other library. On the other hand, its command line tools are optimized for human or script usage, making certain tasks very non-ergonomic and others not exposed at all.

This project attempts to bridge that gap, by providing C++ tools, compiled against the Nix codebase, that are suitable for building an "FFI" for projects using Nix from other languages. It should only be used by programmers, and usually wrapped in appropriate language-specific bindings.

## Is this an official project?

No, and hopefully it never will be. If this proves useful and successful, it would be much better to upstream these capabilities directly into Nix itself, or even better refactor it to allow a proper FFI.

## What capabilities do you expose?

Currently none, as this is the initial commit! However, the initial plan, based on our current needs at [Scarf](https://about.scarf.sh), is to include:

- Tooling to set up and use an isolated Nix store for automated test-suties
- Temporary gc-root management

More functionality will be added as-needed, either by us or actual users when we have them.
