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

See the root [README](../README.md) for prerequisites and the
[guide](http://darkpyonix.dev/dioxus-compose/) for everything else.
