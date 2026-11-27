{
  description = "Train Master の開発環境(SDL2・Rust 1.88・Buildroot のホスト道具)";

  inputs = {
    # Rust 1.88 / SDL2 2.30 の CI(Ubuntu 24.04)に近い構成。
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    # 24.11 の mise は 2024.10.8 で、mise.toml の settings.lockfile や
    # components/targets の配列を読めない。mise だけ unstable から取る。
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      nixpkgs-unstable,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      eachSystem = f: nixpkgs.lib.genAttrs systems f;
      pkgsFor =
        system:
        import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };
    in
    {
      devShells = eachSystem (
        system:
        let
          pkgs = pkgsFor system;
          pkgsUnstable = import nixpkgs-unstable { inherit system; };
          rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in
        {
          default = pkgs.mkShell {
            packages = [
              # Rust(版は rust-toolchain.toml。mise.toml と同じ 1.88.0)
              rust
              pkgs.cargo-deny
              pkgs.cargo-about
              pkgs.cargo-llvm-cov

              # 開発機でのビルドとテスト(24.11 の SDL2 2.30 は CI の libsdl2-dev と同じ系)
              pkgs.SDL2
              pkgs.pkg-config
              pkgs.cmake

              # このリポジトリの検査
              pkgs.actionlint
              pkgs.shellcheck
              pkgs.jq

              # mise のタスク(mise run ...)も使えるようにする
              pkgsUnstable.mise

              # Buildroot のホスト側の道具(br:build / br:menuconfig)
              pkgs.gnumake
              pkgs.bc
              pkgs.bzip2
              pkgs.cpio
              pkgs.diffutils
              pkgs.file
              pkgs.findutils
              pkgs.gawk
              pkgs.gzip
              pkgs.ncurses
              pkgs.patch
              pkgs.perl
              pkgs.python3
              pkgs.rsync
              pkgs.gnutar
              pkgs.unzip
              pkgs.wget
              pkgs.curl
              pkgs.xz
              pkgs.which
            ];

            shellHook = ''
              echo "Train Master devshell"
              echo "  rustc: $(rustc --version)"
              echo "  cargo: $(cargo --version)"
              echo "  SDL2:  ${pkgs.SDL2.dev}/include/SDL2"
              echo
              echo "  mise run test       # 全部のテスト(libsdl2)"
              echo "  mise run run        # 開発機で起動"
              echo "  mise run br:build   # ラズパイ 3 のイメージ"
            '';
          };
        }
      );

      formatter = eachSystem (system: (pkgsFor system).nixfmt-rfc-style);
    };
}
