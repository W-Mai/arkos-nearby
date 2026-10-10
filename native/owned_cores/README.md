# Owned multiplayer cores

`nestopia_nearby` installs at `/opt/arkos-nearby/cores/nestopia_nearby_libretro.so`. The multiplayer context selects it for NES games using the verified ARM64 Nestopia build. Room matching and launch validation retain the selected core and verify its exact digest.

The Nestopia patch restores the eight input bookkeeping bytes at the end of the effective NST state and clears the remaining serialization buffer. [source.json](nestopia/source.json) records the upstream revision, archive, patch, build inputs, output digest and native metadata. [COPYING](nestopia/COPYING) contains the GNU GPL version 2.

## Build

`scripts/build_owned_cores.py check` validates the prepared artifact and source records offline. Release bundling uses the same validation.

The explicit build command uses locally prepared input archives, an AArch64 glibc sysroot, runtime libraries, Clang and LLVM tools. Archive filenames and checksums are listed in `source.json`. `--inputs` contains `nestopia-7dfdc25.tar.gz` and the three recorded `.deb` files; the builder extracts the required headers, CRT objects and copyright records. Compiler outputs go to `/tmp/arkos-rust-build/owned-cores/nestopia/`.

```sh
python3 scripts/build_owned_cores.py build \
  --inputs /path/to/input-archives \
  --sysroot /path/to/aarch64-sysroot \
  --runtime /path/to/cpp-runtime \
  --libc /path/to/libc.so.6 \
  --libm /path/to/libm.so.6 \
  --builtins /path/to/libclang_rt.builtins-aarch64.a \
  --clang /path/to/clang++ \
  --linker /path/to/rust-lld \
  --strip /path/to/llvm-strip
```

The build starts from an empty build directory, applies the checked patch, and verifies the resulting binary against the accepted digest before copying it into the release asset cache. The recorded tool versions and input libraries reproduce the tested ARM64 artifact.

## Source distribution

Distribute the exact upstream source archive together with `nestopia/state-footer.patch`, `nestopia/source.json`, `nestopia/COPYING` and `scripts/build_owned_cores.py`. The patch changes state footer restoration and serialization padding on 2026-10-10. The binary bundle includes the source record and license; `release-sources.json` associates them with the installed artifact.
