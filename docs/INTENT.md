# INTENT

이 문서는 **왜** 이렇게 만드는지를 기록합니다. 결정이 바뀌면 이 문서를 먼저 고치고, 그다음 SPEC을 고칩니다.

## 1. 동기

- 웹 기반 LLM 채팅 인터페이스(claude.ai, ChatGPT 데스크톱 등)는 **메모리 사용량과 배포 용량이 너무 큽니다.** 반응성보다 이 무게 자체가 문제입니다.
- darkpyonix-ember는 여러 CLI를 상시 구동하는 개발 도구라서 항상 켜 두는 앱입니다. 가벼워야 합니다.
- 가벼우면서도 **텍스트·위젯 품질, 특히 한글 IME는 양보할 수 없습니다.** 채팅/IDE에서 텍스트 입력은 주 인터페이스이고, 90%만 완성된 IME는 곧 사용할 수 없는 앱입니다.

## 2. 협상 불가 조건

| ID | 조건 | 이유 |
|---|---|---|
| C1 | 웹뷰 금지 | 메모리/용량. Tauri, Electron, 웹뷰 기반 Dioxus 렌더러는 모두 탈락입니다 |
| C2 | JVM 동봉 금지 | jlink로 줄여도 80~120MB로, 목표에 비해 무겁습니다. JNI Invocation으로 JVM을 품는 방식도 같은 이유로 탈락입니다 |
| C3 | 수동 JNI 금지, Rust 측 타입 안전 | 경계를 넘을 때 Rust 타입 시스템의 이점이 증발해서는 안 됩니다 |
| C4 | UI는 Rust에서 선언형으로 작성 | 사용자 코드는 Rust에 있어야 합니다. Kotlin은 렌더러 구현 세부입니다 |
| C5 | Compose 수준의 텍스트/IME/위젯 품질 | Rust 네이티브 프레임워크(Iced 등)를 쓰지 않는 이유입니다 |

## 3. 핵심 결정

### D1. Rust가 UI를 **기술**하고, Compose는 **인터프리터**로 그린다

Rust에서 Compose API를 직접 호출하지 않습니다. GraalVM `@CEntryPoint`는 primitive와 word 타입만 넘길 수 있어서 `Modifier`나 `MutableState` 같은 객체를 노출하는 것은 원리적으로 불가능합니다.

대신 UI 트리를 **값**으로 보냅니다. Kotlin 측에 고정된 위젯 스키마를 해석하는 범용 인터프리터를 두고, Rust는 트리 변경분을 바이트로 보냅니다. Cash App Redwood, Jetpack Glance와 같은 계열의 검증된 패턴입니다.

### D2. Rust 측 작성 모델은 Dioxus

`dioxus-core`는 렌더러와 무관한 리컨사일러입니다. VirtualDom이 `Mutations` 스트림을 내보내므로 커스텀 렌더러를 붙이기 좋습니다(dioxus-tui, blitz가 같은 방식입니다). `rsx!`, 컴포넌트, 훅이 그대로 딸려오고 Compose에서 넘어온 개발자에게 사용감이 가깝습니다.

- 검토한 대안: Iced/Elm 스타일 빌더. 타입과 IDE 지원은 더 낫지만 중첩이 깊어질수록 읽기 어렵습니다.
- 감수하는 비용: `rsx!` 매크로 내부는 rust-analyzer 지원이 제한적입니다.

### D3. Compose 렌더러는 AOT 네이티브 라이브러리로 배포

| 플랫폼 | 방식 |
|---|---|
| 데스크톱 | native-image `--shared`. **macOS에서는 Liberica NIK Full이 필요합니다** — upstream GraalVM은 Darwin에서 AWT 지원을 건너뜁니다(oracle/graal#13272, 2026-09 기준 open). NIK는 AWT를 정적 링크합니다 |
| iOS | Kotlin/Native `-produce static` + `@CName` C 심볼 |
| Android | 대상 플랫폼. ART라서 native-image가 불가능합니다. Kotlin/Android 앱이 Rust cdylib을 로드하고, 생성된 JNI 심을 씁니다(D9) |
| Web | 대상 플랫폼. Compose wasmJs + Dioxus wasm. 브라우저에서 실행하는 것이라 앱이 웹뷰를 내장하는 것과는 다르고 C1에 해당하지 않습니다. JS 브리지 경유는 성능상 금지하고, wasm 모듈끼리 직결합니다(SPEC PR-5) |

### D4. 창 소유권은 관심사가 아니다. Compose Desktop의 AWT 경로를 그대로 쓴다

ComposeScene과 커스텀 `PlatformContext`로 Rust가 창을 소유하는 경로는 **채택하지 않습니다.** 그 경로에서는 IME(`NSTextInputClient`, TSF, ibus/fcitx)를 직접 배선해야 하고, 이는 C5를 위협합니다.

`ComposeWindow`는 AWT `JFrame`이고, AOT 컴파일은 코드 경로를 바꾸지 않습니다. 따라서 native-image에서도 AWT의 IME 경로가 유지됩니다. native-image에서 한글 입력이 깨진다면 **구현 문제가 아니라 설정 문제**로 취급합니다.

- ServiceLoader(`InputMethodDescriptor`) 리소스 포함
- JNI 및 리플렉션 config. tracing agent는 **실제로 한글을 입력하면서** 돌려야 해당 경로가 잡힙니다
- 문자셋/로케일 리소스, CJK 폰트 폴백

### D5. UI 로컬 상태는 Kotlin에, 도메인 상태는 Rust에

- **TextField는 비제어(uncontrolled) 위젯입니다.** 조합 중인 텍스트를 Rust로 왕복시키면 한글 조합이 깨집니다. Rust는 확정된 값(제출, 포커스 아웃, 디바운스)만 받고, 값을 바꿀 때는 조합이 끝난 뒤 명시적 명령으로만 바꿉니다.
- 스크롤 위치, 포커스, 애니메이션 진행 상태도 Kotlin이 소유합니다.
- 매 프레임 트리 전체를 보내지 않고 변경분만 보냅니다. Compose의 recomposition 스코프 이점을 보존하기 위해서입니다.

### D6. 타입 안전은 Rust 단일 소스 코드젠으로

C ABI는 `bytes`만 오가므로 타입 안전은 그 위에 얹습니다. 위젯 스키마, 속성, 이벤트 페이로드를 Rust에서 정의하고 Kotlin 타입과 코덱을 생성합니다. 스키마 해시로 양쪽 버전 불일치를 검출합니다.

### D7. 개발 경로와 배포 경로를 분리

- 개발: Kotlin 렌더러를 JVM 위에서 구동해 hot reload와 `@Preview`를 씁니다.
- 배포: 같은 소스를 native-image와 Kotlin/Native로 컴파일합니다.

native-image는 CI와 릴리스에서만 돌립니다.

### D8. 경계는 동기·동일 스레드 직접 호출, 데이터는 zero-copy

- 옛 React Native 브리지의 병목(직렬화, 비동기 전용, 스레드 홉)을 피하기 위해 JSI 방식을 택했습니다. VirtualDom은 Renderer UI 스레드에서 돌고, 양쪽은 서로를 직접 호출합니다.
- 버퍼는 큐가 아니라 한 번의 호출에서 Mutation 여러 개를 넘기는 인자입니다. 고정 레이아웃이라 제자리에서 읽습니다.
- 무거운 도메인 작업은 Host 워커 스레드에서 돌리고, UI 스레드에는 wake 신호만 보냅니다.
- Web에서도 JS 브리지를 거치지 않습니다.
- 근거는 SPEC PR-1~PR-6에 있습니다.

### D9-macOS. macOS 실행 모델과 우회책

2026-09-20에 macOS arm64에서 검증한 내용입니다(SPEC PR-7-macOS).

- Liberica NIK 25 Full로 Compose Desktop을 native-image로 빌드하면 창이 뜨고 렌더링됩니다. JDK 설치가 필요 없습니다.
- 정적 링크된 macOS AWT는 런타임에 세 가지를 파일 경로로 찾습니다. 각각 얇은 우회책으로 메웁니다.
  - `libawt_lwawt.dylib`: libawt 초기화가 경로로 로드합니다. JNI 함수는 실행 이미지 안에서 해석되므로 자리만 채우는 dylib을 둡니다.
  - `libjawt.dylib`: Skiko가 `<java.home>/lib`에서 dlopen합니다. 이미지 안의 `JAWT_GetAWT`로 넘기는 포워더를 둡니다.
  - `JNI_OnLoad_osxui`: 정적 JNI 라이브러리에 필수인 심볼인데 아카이브에 없어서 직접 정의합니다.
- **AppKit은 메인 스레드를 요구합니다.** 렌더러는 보조 스레드에서 돌고, 메인 스레드는 NSApplication을 직접 만들어 실행합니다. 이렇게 하면 AWT가 임베디드 모드(SWT/JavaFX 호스트와 같은 방식)로 동작합니다. AWT가 자기 루프를 갖게 두면 `[NSApp run]`을 무한히 다시 들어가서 창을 닫아도 Host로 제어가 돌아오지 않습니다.

### D9. Android는 Kotlin이 호스트, 경계 정의는 방향 중립

ART에서는 호스트 관계가 뒤집힙니다. 경계를 호출 방향과 무관한 논리 연산으로 정의하고 `LoopMode`로 구분하면, 사용자 Rust 코드와 메시지 포맷을 전 플랫폼에서 공유할 수 있습니다. JNI 심은 코드젠으로 생성합니다(수동 JNI 금지 충족). UniFFI(JNA)와 android-activity 계열 진입점은 채택하지 않았습니다(SPEC PR-5).

## 4. 폐기한 대안

| 대안 | 폐기 이유 |
|---|---|
| 비동기 큐 기반 브리지 (옛 React Native 방식) | 동기 반환 불가, 스레드 홉 지연 |
| UniFFI(JNA)로 Android 경계 생성 | 호출당 오버헤드가 크고, 데스크톱/iOS와 별도의 바인딩 층이 생깁니다 |
| Tauri | 웹뷰(C1). IME와 접근성은 웹뷰에 떠넘겨 해결하지만 무게가 문제입니다 |
| Compose + JVM 동봉 (jlink, AppCDS, JNI Invocation) | C2. AppCDS는 시작 시간 해법이지 용량 해법이 아닙니다 |
| UniFFI/JNI 기반 Kotlin 호스트 + Rust dylib | JVM이 필요하고(C2), UI 코드가 Kotlin에 있게 됩니다(C4) |
| Iced 단독 | 텍스트/IME 성숙도(C5) 부족. 모바일은 upstream이 out of scope로 선언했습니다 |
| Rust 소유 창 + ComposeScene + 직접 IME 배선 | C5 위험, `@InternalComposeUiApi` 의존, 접근성 상실 |
| Rust가 AWT 네이티브 피어를 대체 | JDK 내부 인터페이스라 사실상 AWT를 재구현하는 분량입니다 |
| 네이티브 위젯 바인딩(objc2, windows-rs, gtk-rs) | 크로스플랫폼 선언형 프레임워크가 아니라 FFI입니다 |

## 5. 알려진 비용

- **프레임워크를 하나 만드는 일입니다.** 스키마, 인터프리터, 프로토콜, 이벤트 라우팅, 코드젠이 모두 필요합니다.
- 스키마에 없는 Compose 기능은 쓸 수 없습니다. 위젯을 추가할 때마다 스키마, 인터프리터, 코드젠을 확장해야 합니다.
- native-image 바이너리는 Skia를 포함해 수십 MB가 하한입니다. Iced(10~20MB)보다는 크지만 웹뷰나 JVM 스택보다는 작습니다.
- LazyColumn은 단순 트리 diff로 가상화를 유지할 수 없어서 전용 윈도잉 프로토콜이 필요합니다.
