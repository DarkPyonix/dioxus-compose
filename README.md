# dioxus-compose

Rust에서 **Dioxus 방식으로 선언형 UI를 작성**하고, 렌더링·텍스트·IME는 **AOT 컴파일된 Compose Multiplatform**이 맡도록 만드는 GUI 스택입니다.

- Rust 개발자가 `rsx!`와 컴포넌트, 훅으로 UI를 작성합니다.
- `dioxus-core`의 VirtualDom이 변경분(Mutations)을 만들고, 이를 좁은 C ABI를 통해 네이티브 Compose 렌더러로 보냅니다.
- Compose 렌더러는 JVM 없이 네이티브 라이브러리로 배포됩니다. 데스크톱은 GraalVM native-image, iOS는 Kotlin/Native로 만듭니다.
- 웹뷰를 쓰지 않고, JVM을 동봉하지 않으며, 수동 JNI도 쓰지 않습니다.

> 상태: **설계 단계 (Spec Driven Development)**. 구현은 `docs/SPEC.md`의 요구사항과 수용 기준을 따라 진행합니다.

## 저장소 구조

```
dioxus-compose/
├─ dioxus-compose/            # Rust 측: Dioxus 렌더러 크레이트 (호스트)
├─ dioxus-compose-renderer/   # Kotlin 측: Compose Multiplatform 렌더러 (Amper)
│  ├─ shared/                 #   위젯 스키마 인터프리터 (공통)
│  ├─ desktop/                #   JVM 개발 셸 / native-image 빌드 대상
│  ├─ ios/  android/  web/    #   플랫폼 타깃
└─ docs/
   ├─ INTENT.md               # 왜 만드는가, 확정된 결정과 폐기한 대안
   └─ SPEC.md                 # 요구사항, 프로토콜, 수용 기준
```

## 문서

| 문서 | 내용 |
|---|---|
| [PROJECT.md](PROJECT.md) | 범위, 개발 방식(SDD), 마일스톤, 열린 질문 |
| [docs/INTENT.md](docs/INTENT.md) | 동기, 협상 불가 조건, 아키텍처 결정 기록 |
| [docs/SPEC.md](docs/SPEC.md) | 기능/비기능 요구사항, 경계 프로토콜, 검증 기준 |

## Development setup

Run `./scripts/setup-check.sh` first. It verifies everything below and prints the exact
command to install whatever is missing.

### Rust

Install the toolchain with [rustup](https://rustup.rs). `scripts/check.sh` runs
`cargo fmt` and `cargo clippy`, so both components are required:

```bash
rustup component add rustfmt clippy
```

### Liberica NIK 25 Full (renderer native image)

The desktop renderer is built with GraalVM native-image. **On macOS, upstream GraalVM
does not work**: it skips AWT on Darwin ([oracle/graal#13272](https://github.com/oracle/graal/issues/13272)),
so Compose Desktop cannot be linked into an image. Use BellSoft Liberica NIK 25 **Full**
(the Full variant; the standard one is not enough).

```bash
brew install --cask liberica-nik-full
# or download "NIK 25 Full" from https://bell-sw.com/pages/downloads/native-image-kit/
```

`dioxus-compose-renderer/native/scripts/env.sh` finds it in this order:

1. `$GRAALVM_HOME`, if set.
2. Otherwise the newest match of
   `~/Library/Java/JavaVirtualMachines/bellsoft-liberica-vm-full-openjdk25*/Contents/Home`.

The installation used for development is
`~/Library/Java/JavaVirtualMachines/bellsoft-liberica-vm-full-openjdk25-25.0.4.1`.
The scripts reject an installation without `lib/static/darwin-*/libawt_lwawt.a`, which is
how a plain GraalVM is caught before a long build fails at the link step.

macOS also needs the Xcode command line tools (`xcode-select --install`) for `cc`, `ld`
and the AppKit headers used by `dioxus-compose-renderer/native/c/`.

Only macOS is scripted so far. Linux and Windows native-image builds are not yet.

### Kotlin

Nothing to install. `dioxus-compose-renderer/kotlin` (and `kotlin.bat` on Windows) is a
self-bootstrapping Kotlin Toolchain wrapper: it downloads the pinned toolchain on first
use.

## Building and running

### JVM development shell

The fastest loop for renderer work, with hot reload and `@Preview` (NFR-5). No
native-image build required:

```bash
cd dioxus-compose-renderer
./kotlin run -m desktop   # Compose dev shell
./kotlin run -m native    # the renderer module itself, on the JVM
```

### Renderer native shared library

Produces `dioxus-compose-renderer/build/native-image/dist/lib/` with the renderer,
Skia, and the `libjawt` / `libawt_lwawt` shims (SPEC PR-8). Takes a few minutes:

```bash
cd dioxus-compose-renderer
./native/scripts/build-native.sh
```

### C smoke host

Links a minimal C host against the built library and calls
`dioxus_compose_renderer_run`. A window opens; closing it must return 0 (PR-8
acceptance criterion):

```bash
cd dioxus-compose-renderer
./native/scripts/smoke-test.sh
```

Set `DIOXUS_COMPOSE_AUTOEXIT_MS=6000` to have the window close itself, for unattended runs.

### Rust demo

After the native library is built:

```bash
cargo run -p dioxus-compose --example desktop_demo --features native-renderer
```

The build script looks for the renderer in this workspace's
`dioxus-compose-renderer/build/native-image/dist/lib`. Set
`DIOXUS_COMPOSE_RENDERER_DIR` to use a renderer staged somewhere else.

## Quality gate

```bash
./scripts/check.sh              # fmt, clippy, tests, quick benchmarks, Kotlin build + test
./scripts/check.sh --full       # same, with the full benchmark sample
./scripts/check.sh --no-kotlin  # Rust only (also: DXC_SKIP_KOTLIN=1)
```

## 라이선스

Apache License 2.0. [LICENSE](LICENSE)를 참고하세요.
