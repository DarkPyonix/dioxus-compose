# dioxus-compose-renderer

The Kotlin half of dioxus-compose: the Compose Multiplatform renderer that interprets the
protocol the Rust Host sends and draws it with real Compose widgets.

Built with the [Kotlin Toolchain](https://kotlin-toolchain.org/dev/). The `kotlin` (macOS,
Linux) and `kotlin.bat` (Windows) wrappers in this directory bootstrap the pinned toolchain
on first use, so nothing has to be installed separately.

## Modules

| Module | What it is |
|---|---|
| `native` | **The renderer.** The schema interpreter, the `HostConnection` implementations, the C entry points, and the macOS native-image build (`native/scripts/`, `native/c/`). This is the module that matters. |
| `shared` | Compose code shared across platforms, including the IME test screen used to verify text input in a native build. |
| `desktop` | JVM development shell for working on Compose code with hot reload and `@Preview`. |
| `iosrenderer` | **The renderer for iOS.** The same interpreter sources (`iosrenderer/src/shared/` symlinks `native/src/`) compiled by Kotlin/Native, plus the iOS half of the boundary: `IosHostConnection`, the UIKit entry, and the `java.nio` shim the generated codec needs. |
| `iosentry` | The two `@CName` functions that become the C symbols of the iOS static library. Separate so that `-produce static` generates a C header for them and not for the whole of Compose. |
| `android`, `ios`, `web` | Platform targets from the project template. Designed but not implemented; see `docs/SPEC.md` PR-5 and PR-6. |

The generated protocol bindings live in `native/src/protocol/Protocol.gen.kt`. They are
produced from the Rust schema by `cargo run -p dioxus-compose --bin codegen`, edit the Rust
schema, never that file.

## Running

```bash
./kotlin run -m native     # the renderer's development harness on the JVM
./kotlin run -m desktop    # the Compose development shell
```

## Testing

```bash
./kotlin test              # everything
./kotlin test -m native    # the interpreter's tests
```

## Native image

The desktop renderer ships as a native shared library, built with Liberica NIK (upstream
GraalVM skips AWT on macOS). From this directory:

```bash
./native/scripts/build-native.sh   # build the shared library and stage lib/
./native/scripts/smoke-test.sh     # link a C host against it and open a window
```

## iOS

iOS ships as a Kotlin/Native static library that exports the same two C symbols the desktop
library does, so the Host does not know which platform it is linked against (SPEC PR-2, M5).
There is no GraalVM and therefore no isolate, so there is no C shim either: `@CName` puts the
symbol on the Kotlin function. `dioxus_compose_renderer_run` must be called on the process
main thread, where `UIApplicationMain` installs the run loop, and it never returns.

```bash
./native/scripts/build-ios.sh                    # arm64 simulator static library
./native/scripts/build-ios.sh --target device    # arm64 iPhone
./native/scripts/ios-smoke-test.sh --screenshot /tmp/ios.png   # run it on the simulator
```

The smoke test links `native/c/smoke_host.c`, the same stand-in Host the desktop smoke test
uses, so a passing run on both platforms is evidence that the C ABI really is one ABI.

See the root [README](../README.md) for prerequisites and the
[guide](http://darkpyonix.dev/dioxus-compose/) for everything else.
