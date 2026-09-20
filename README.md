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

## 개발 환경 준비

먼저 `./scripts/setup-check.sh`를 실행하세요. 아래 항목을 모두 점검하고, 빠진 것이 있으면 설치 명령을 그대로 알려줍니다.

### Rust

[rustup](https://rustup.rs)으로 설치합니다. `scripts/check.sh`가 `cargo fmt`와 `cargo clippy`를 쓰므로 두 컴포넌트가 필요합니다.

```bash
rustup component add rustfmt clippy
```

### Liberica NIK 25 Full (렌더러 네이티브 이미지)

데스크톱 렌더러는 GraalVM native-image로 빌드합니다. **macOS에서는 upstream GraalVM이 동작하지 않습니다.** Darwin에서 AWT 지원을 건너뛰기 때문에([oracle/graal#13272](https://github.com/oracle/graal/issues/13272)) Compose Desktop을 이미지에 링크할 수 없습니다. BellSoft Liberica NIK 25 **Full**을 쓰세요. 표준 버전이 아니라 Full이어야 합니다.

```bash
brew install --cask liberica-nik-full
# 또는 https://bell-sw.com/pages/downloads/native-image-kit/ 에서 "NIK 25 Full" 내려받기
```

`dioxus-compose-renderer/native/scripts/env.sh`가 다음 순서로 찾습니다.

1. `$GRAALVM_HOME`(설정된 경우)
2. `~/Library/Java/JavaVirtualMachines/bellsoft-liberica-vm-full-openjdk25*/Contents/Home` 중 최신

개발에 사용한 설치 경로는 `~/Library/Java/JavaVirtualMachines/bellsoft-liberica-vm-full-openjdk25-25.0.4.1`입니다. 스크립트는 `lib/static/darwin-*/libawt_lwawt.a`가 없는 설치를 거부합니다. 순정 GraalVM을 긴 빌드 끝의 링크 실패가 아니라 시작 전에 잡아내기 위한 것입니다.

macOS에서는 Xcode 명령줄 도구(`xcode-select --install`)도 필요합니다. `cc`, `ld`와 `dioxus-compose-renderer/native/c/`가 쓰는 AppKit 헤더 때문입니다.

현재 스크립트가 지원하는 것은 macOS뿐입니다. Linux와 Windows 네이티브 이미지 빌드는 아직입니다.

### Kotlin

설치할 것이 없습니다. `dioxus-compose-renderer/kotlin`(Windows는 `kotlin.bat`)이 자체 부트스트랩 래퍼라, 처음 실행할 때 고정된 버전의 툴체인을 내려받습니다.

## 빌드와 실행

### JVM 개발 셸

렌더러 작업에서 가장 빠른 반복 경로입니다. hot reload와 `@Preview`를 쓸 수 있고(NFR-5) 네이티브 이미지 빌드가 필요 없습니다.

```bash
cd dioxus-compose-renderer
./kotlin run -m desktop   # Compose 개발 셸
./kotlin run -m native    # 렌더러 모듈 자체를 JVM에서 실행
```

### 렌더러 네이티브 공유 라이브러리

`dioxus-compose-renderer/build/native-image/dist/lib/`에 렌더러, Skia, `libjawt`/`libawt_lwawt` 보조 라이브러리를 만듭니다(SPEC PR-8). 몇 분 걸립니다.

```bash
cd dioxus-compose-renderer
./native/scripts/build-native.sh
```

### C 스모크 호스트

최소한의 C 호스트를 빌드해 `dioxus_compose_renderer_run`을 호출합니다. 창이 뜨고, 닫으면 0을 반환해야 합니다(PR-8 수용 기준).

```bash
cd dioxus-compose-renderer
./native/scripts/smoke-test.sh
```

무인 실행이 필요하면 `DIOXUS_COMPOSE_AUTOEXIT_MS=6000`으로 창이 스스로 닫히게 할 수 있습니다.

### Rust 데모

네이티브 라이브러리를 빌드한 뒤에 실행합니다.

```bash
cargo run -p dioxus-compose --example desktop_demo --features native-renderer
```

빌드 스크립트는 워크스페이스의 `dioxus-compose-renderer/build/native-image/dist/lib`에서 렌더러를 찾습니다. 다른 위치에 둔 렌더러를 쓰려면 `DIOXUS_COMPOSE_RENDERER_DIR`을 설정하세요(NFR-10).

## 품질 게이트

```bash
./scripts/check.sh              # fmt, clippy, 테스트, 빠른 벤치마크, Kotlin 빌드와 테스트
./scripts/check.sh --full       # 위와 같되 벤치마크를 전체 샘플로
./scripts/check.sh --no-kotlin  # Rust만 (DXC_SKIP_KOTLIN=1도 동일)
```

## 라이선스

Apache License 2.0. [LICENSE](LICENSE)를 참고하세요.
