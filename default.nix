let
  nix-src = fetchFromGitHubBoot {
    owner = "shlevy";
    repo = "nix";
    rev = "d7fd8a03d9270d4ac3342b4f8f820798222f9adb";
    sha256 = "14vnh5hflwicb0zaglm9fhy1ab9xh6kzws8w0lyzldd3j15xmpvd";
  };
  fetchFromGitHubBoot = (import <nixpkgs> {}).fetchFromGitHub;
in
{ pkgs ? import <nixpkgs> {
    overlays = [ (import nix-src).overlay ];
  }
, stdenv ? pkgs.stdenv
, nix ? pkgs.nix
, fetchFromGitHub ? pkgs.fetchFromGitHub
, cmake ? pkgs.cmake
, pkg-config ? pkgs.pkg-config
, boost ? pkgs.boost
, nlohmann_json ? pkgs.nlohmann_json
, rustPlatform ? pkgs.rustPlatform
}: let
  # TODO gitignore, don't rebuild everything when you touch a readme
  baseSrc = stdenv.lib.cleanSource ./.;
in rec {
  plugin = stdenv.mkDerivation {
    pname = "nix-ffi";
    version = "1.0.0";
    src = baseSrc;
    nativeBuildInputs = [ cmake pkg-config ];
    buildInputs = [ nix boost nlohmann_json ];
  };
  # Note that pending https://github.com/rust-lang/cargo/issues/2552 this
  # is just useful to confirm it builds. To actually depend on this crate
  # we need to pull it to some other (ultimately executable) crate.
  rust = rustPlatform.buildRustPackage {
    pname = "nix-ffi";
    version = "1.0.0";
    src = baseSrc + "/rust";
    nativeBuildInputs = [ nix ]; # for testing
    NIX_FFI_PLUGIN_PREFIX = "${plugin}";
    cargoSha256 = "10fg35n1bchaqbzyr0amb5nl9pq97gj32i938bydxmh2lwardq79";
  };
}
