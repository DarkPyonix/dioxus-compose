# dioxus-compose

[![CI](https://github.com/DarkPyonix/dioxus-compose/actions/workflows/ci.yml/badge.svg)](https://github.com/DarkPyonix/dioxus-compose/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/DarkPyonix/dioxus-compose/blob/main/LICENSE)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

[English](https://github.com/DarkPyonix/dioxus-compose/blob/main/README.md) · **한국어**

**HTML과 CSS로 쓴 Dioxus 앱을 compose-rust를 거쳐 Compose Multiplatform이 웹뷰 없이 네이티브로 그리는 Dioxus 렌더러입니다.**

*웹뷰도 동봉된 JVM도 없습니다.*

```rust
use dioxus_compose::html::prelude::*;
use dioxus_hooks::use_signal;

pub const STYLE: &str = r#"
.page { padding: 40px 48px; }
h1 { margin: 0 0 16px; font-size: 32px; line-height: 40px; }
"#;

pub fn app() -> Element {
    let mut hellos = use_signal(|| 0u32);

    rsx! {
        main { class: "page",
            h1 { "Hello, Dioxus" }
            a {
                href: "#",
                onclick: move |event| {
                    event.prevent_default();
                    hellos += 1;
                },
                "Say hello"
            }
        }
    }
}
```

[`samples/hello`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples/hello) 를 줄인
것입니다. **지금은 배치까지 되고, 렌더러 다리가 생기면 그려집니다**
([#43](https://github.com/DarkPyonix/dioxus-compose/issues/43)). HTML 경로는 Rust 에서 레이아웃과 그리기
계획을 계산하지만, 아직 화면에 픽셀을 놓지는 않습니다.

웹에서 쓰듯 Dioxus 앱을 씁니다. `div`, `span` 과 CSS 를 담은 `rsx!`, 훅과 시그널입니다. `blitz-dom` 이
Host 안에서 스타일과 레이아웃을 계산하고, 상자와 글자를 그리는 것은
[compose-rust](https://github.com/DarkPyonix/compose-rust) 의 AOT 컴파일된 Compose 렌더러입니다. Compose
의 텍스트 레이아웃과 플랫폼 IME 를 그대로 씁니다.

같은 크레이트는 `rsx!` 안에서 Compose 위젯 이름(`Column`, `Text`, `Button`)도 받습니다. 그 경로는 지금
렌더러가 그립니다.

---

## 목차

- [왜 만드는가](#왜-만드는가)
- [현재 상태](#현재-상태)
- [화면을 쓰는 두 가지 방법](#화면을-쓰는-두-가지-방법)
- [시작하기](#시작하기)
- [동작 방식](#동작-방식)
- [성능](#성능)
- [저장소 구조](#저장소-구조)
- [기여하기](#기여하기)
- [문서](#문서)
- [라이선스](#라이선스)

---

## 왜 만드는가

두 가지 문제가 여기서 만납니다.

**웹 기술로 만든 데스크톱 앱은 무겁습니다.** 하루 종일 켜 두는 앱에서는 반응 속도보다 메모리 사용량과
다운로드 크기가 문제입니다. 브라우저를 품거나 JVM 을 함께 배포하면 내 코드가 돌기도 전에 수십에서
수백 메가바이트가 듭니다.

**Rust 에는 Compose 수준의 텍스트를 가진 툴킷이 없습니다.** Rust GUI 툴킷은 잘 그리지만 텍스트 셰이핑,
선택, 접근성, 무엇보다 입력기가 아직 Compose 에 이르지 못했습니다. 타이핑이 곧 인터페이스인 앱에서
90% 만 되는 IME 는 안 되는 앱입니다. 한글, 일본어, 중국어 조합은 있으면 좋은 기능이 아닙니다.

그래서 dioxus-compose 는 앱이 이미 쓰고 있는 Dioxus 와 CSS 를 그대로 두고, 픽셀과 텍스트와 입력기는
Compose 에서 빌려 옵니다. Compose 쪽은 미리 네이티브 라이브러리로 컴파일되므로 배포하는 것에 JVM 이
없습니다.

### 무게

notepad 예제(위젯 경로 앱,
compose-rust의 [`samples/notepad`](https://github.com/DarkPyonix/compose-rust/tree/develop/samples/notepad))를
릴리스로 빌드해 스트립한 것입니다. 실행 파일 하나이고, 옆에 놓이는 런타임도 안에 든 가상 머신도
없습니다.

| 플랫폼 | 실행 파일 | 물리 메모리 | 렌더러 빌드 방식 |
|---|---|---|---|
| macOS (arm64) | **28.76 MB** | **35.1 MB** | Kotlin/Native, Metal 로 그림 |
| Linux (x86-64) | **37.93 MB** | 아직 측정 안 함 | Kotlin/Native, GLX 로 그림 |
| Windows | 아직 측정 안 함 | 아직 측정 안 함 | GraalVM 네이티브 이미지, Direct3D 12 로 그림 |

웹뷰 방식은 브라우저 엔진을 싣고, 동봉한 JVM 은 jlink 를 거쳐도 그것만 80~120 MB 쯤이며, 순수 Rust
툴킷은 10~20 MB 쯤입니다. 이 셋은 자릿수 수준의 대략이고 이 프로젝트의 측정값이 아닙니다.

### 하지 않는 것

- **웹뷰 없음.** WKWebView, WebView2, WebKitGTK, Tauri, wry 를 쓰지 않습니다.
- **동봉된 JVM 없음.** 데스크톱 렌더러는 네이티브 코드입니다.
- **손으로 쓴 경계 글루 없음.** Rust 와 Kotlin 사이의 심은 Rust 스키마 하나에서 생성합니다.
- **UI 는 Rust 로 씁니다.** Kotlin 은 렌더러를 만드는 방법이지 기능을 넣는 곳이 아닙니다.
- **Compose 수준의 텍스트와 IME 를 우회하지 않습니다.** Compose 의 플랫폼 텍스트 입력을 돌아가는 코드
  경로가 없습니다.

---

## 현재 상태

**활발히 개발 중인 초기 프로젝트**입니다. crates.io 의 0.0.0 은 위젯 경로만 있는 초기 스냅숏이고, HTML
경로는 `develop` 브랜치에 있습니다. API 는 바뀝니다.

| 상태 | 항목 |
|---|---|
| 구현 | blitz-dom 으로 Host 에서 하는 HTML 과 CSS 레이아웃: 블록, 인라인, flexbox, grid, 표, `fixed` 를 포함한 위치 지정, overflow 와 스크롤 컨테이너, 테두리, 모서리, 그림자, 불투명도, 그라디언트를 포함한 배경, 앱이 넘기는 리졸버를 거치는 이미지 |
| 구현 | HTML 이벤트와 폼: 클릭, `input` / `change` / `submit`, `select`, 체크박스와 라디오 그룹, 비제어 입력란, 키가 있는 목록, `text-transform` 과 `white-space` |
| 구현 | VS Code 가 잰 텍스트 크기로, 워크벤치 세 구역의 상자 326 개 중 324 개가 VS Code 와 1px 이내(2026-10-03 macOS 에서 측정) |
| 구현 | [`samples/`](https://github.com/DarkPyonix/dioxus-compose/tree/main/samples) 의 HTML 과 CSS 예제 열한 개, 각각 Host 에서 테스트 |
| 구현 | `rsx!` 의 Compose 위젯을 렌더러가 그림: macOS, Android, 웹은 처음부터 끝까지, Windows, Linux, iOS 는 빌드되고 시작됨 |
| 부분 | HTML 화면을 화면에 그리기: 계획은 있고, compose-rust 렌더러(`AbsoluteBox`)로 가는 다리가 없음([#43](https://github.com/DarkPyonix/dioxus-compose/issues/43). [#67](https://github.com/DarkPyonix/dioxus-compose/pull/67)에 작성되어 검토 대기) |
| 부분 | Compose 로 텍스트 재기: 측정 호출을 compose-rust 에서 만드는 중이고, 그때까지는 Parley 가 잼([#43](https://github.com/DarkPyonix/dioxus-compose/issues/43)) |
| 계획 | `translate` 밖의 CSS 변환, CSS transition 과 animation([#46](https://github.com/DarkPyonix/dioxus-compose/issues/46), [#47](https://github.com/DarkPyonix/dioxus-compose/issues/47)) |
| 계획 | VS Code 처럼 HTML 화면 확대 |
| 계획 | 한 화면에 HTML 과 위젯 |
| 계획 | 마크다운 크레이트 `dioxus-compose-markdown`([#24](https://github.com/DarkPyonix/dioxus-compose/issues/24). 초안 [#68](https://github.com/DarkPyonix/dioxus-compose/pull/68)) |

전체 목록은 가이드의 [현황 페이지](http://darkpyonix.dev/dioxus-compose/ko/status.html)에 있습니다.

---

## 화면을 쓰는 두 가지 방법

| 쓰는 것 | import | 지금 |
|---|---|---|
| HTML 요소와 CSS: `div`, `span`, `input`, 스타일시트 | `dioxus_compose::html::prelude::*` | Host 에서 배치하고 계획까지, 아직 그리지 않음 |
| Compose 위젯 이름: `Column`, `Text`, `Button` | `dioxus_compose::prelude::*` | 렌더러가 그림 |

둘 다 늘 크레이트에 있고, 어느 쪽도 기능 플래그로 끄지 않습니다. 두 prelude 가 각자 자기 요소 이름을
들여오므로 `rsx!` 블록 하나는 한쪽 어휘만 씁니다. 한 화면에 섞는 것은 계획 단계입니다.

위젯 경로 앱입니다.
[`dioxus-compose/examples/desktop_demo.rs`](https://github.com/DarkPyonix/dioxus-compose/blob/main/dioxus-compose/examples/desktop_demo.rs)
를 줄였습니다.

```rust
use dioxus_compose::prelude::*;

fn app() -> Element {
    let mut messages = use_signal(Vec::<String>::new);
    let mut draft = use_signal(String::new);

    rsx! {
        Column {
            fill_max_width: true,
            Text { text: "dioxus-compose chat" }
            for message in messages() {
                Text { text: message }
            }
            TextField {
                placeholder: "Write a message",
                multiline: true,
                on_value_change: move |value| draft.set(value),
                on_key_down: move |event: KeyEvent| {
                    if event.key() == Key::Enter && !event.shift_key() {
                        let message = draft().trim().to_owned();
                        if !message.is_empty() {
                            messages.write().push(message);
                            draft.set(String::new());
                        }
                        event.consume();   // the renderer's onKeyEvent returns true
                    }
                }
            }
        }
    }
}

fn main() {
    dioxus_compose::launch(app);
}
```

IME 가 조합하는 동안에는 키 이벤트가 Rust 에 오지 않습니다. 그래서 한글 조합 중의 `Enter` 는 메시지를
보내지 않고 음절을 확정합니다. `event.consume()` 은 웹의 `preventDefault()` 처럼 렌더러에 키를
처리했다고 알립니다.

---

## 시작하기

HTML 경로는 아직 crates.io 릴리스에 없으므로 저장소에 의존합니다.

```bash
cargo add dioxus-compose --git https://github.com/DarkPyonix/dioxus-compose --branch develop
cargo add dioxus-hooks@0.7 dioxus-signals@0.7
```

페이지를 배치하고 무엇을 그리게 될지 봅니다.

```rust
use dioxus_compose::html::prelude::*;

fn main() {
    let mut dom = HtmlDom::with_config(app, HtmlConfig {
        stylesheets: vec![STYLE.to_string()],
        ..HtmlConfig::default()
    });
    let list = dom.layout(800.0, 600.0, 1.0);
    for entry in &list.entries {
        println!("{:<6} {:?}", entry.tag, entry.rect);
    }
}
```

`cargo run` 은 상자를 출력합니다. 렌더러 다리가 생기기 전까지는 HTML 화면을 그리는 것이 없으므로 아직
창은 열리지 않습니다. 위젯 경로 앱은 대신 `dioxus_compose::launch(app)` 을 불러 창을 엽니다. 렌더러는
[compose-rust](https://github.com/DarkPyonix/compose-rust) 의 빌드 스크립트가 대상 플랫폼용으로 내려받는
미리 빌드된 네이티브 라이브러리이고, 직접 빌드하는 방법은 그쪽에 있습니다.

예제의 테스트는 체크아웃에서 cargo 로 돌립니다.

```bash
cargo test -p sample-html-hello
cargo test --workspace
```

[시작하기 페이지](http://darkpyonix.dev/dioxus-compose/ko/getting-started.html)가 클릭과 폼까지 같은
순서로 안내합니다.

---

## 동작 방식

```
your component (rsx! with div, span, CSS)
  -> Dioxus VirtualDom                 runs it
  -> blitz-dom document                Stylo resolves CSS, Taffy lays out
  -> DisplayList                       boxes, colours, text runs, fields, images
  -> Plan, and its diff                drawing elements; only what changed is sent
  -> Compose renderer (compose-rust)   draws (the bridge is not built yet)
```

**렌더러는 CSS 를 보지 않습니다.** 사각형, 색, 텍스트 조각을 받습니다. 그래서 렌더러는 작고 모든 앱에
같으며, CSS 기능은 Rust 의 한 곳에서 더해집니다.

**텍스트는 그리는 엔진이 잽니다.** Host 는 `TextMeasurer` 에 조각마다 크기를 묻습니다. 지금은 Parley 가
재고, 레이아웃 도중의 동기 호출을 거쳐 Compose 가 재게 됩니다. Parley 의 크기는 줄바꿈이 달라질 만큼
Chromium 과 다르기 때문입니다.

**입력란은 비제어입니다.** 사용자가 치는 동안 글자는 렌더러가 갖고 확정된 글자만 Rust 로 오므로,
입력기의 조합은 왕복하지 않습니다.

**경계는 동기이고 한 스레드에서 돕니다.** JSI 가 React Native 의 브리지를 대신한 방식입니다. 원시값,
포인터, 길이만 넘어가고, 레코드는 Rust 스키마 하나에서 생성한 고정 배치이며, 큐에 쌓거나 복사하는 것이
없습니다. 막히는 일은 워커 스레드에서 돌며 시그널을 고치고 프레임을 요청합니다.

**JavaScript 는 실행하지 않고 아무것도 내려받지 않습니다.** `<script>` 는 아무 일도 하지 않고, 이미지
URL 은 앱의 `ImageResolver` 가 정한 것을 그립니다.

---

## 성능

위젯 경로의 목표는 같은 화면을 Kotlin 과 Compose 로 직접 쓴 것과 구별되지 않는 것입니다. Host 쪽
수치이고, 2026-09-20 Mac mini(M1, 16 GiB, macOS 26.5.1)에서 릴리스 빌드로 재서
[`dioxus-compose/benches/baseline.json`](https://github.com/DarkPyonix/dioxus-compose/blob/main/dioxus-compose/benches/baseline.json)
에 기록했습니다.

| 측정 | 결과 |
|---|---|
| 클릭부터 디스패치, diff, 인코딩까지 | **13.3 µs** p99 |
| 변경 100 개 인코딩 | **2.9 µs** p99, 정상 상태 할당 없음 |
| 메시지 10,000 개 대화에 덧붙이기 100 번 | **220 µs** p99 |

HTML 프레임은 Host 에서 레이아웃을 돌리므로 더 듭니다. VS Code 사이드바 픽스처를 바뀐 것 없이 다시
배치하는 데 2026-10-03 에 **3.1 ms** p99 가 들었고, 다른 작업으로 바쁜 기계에서 쟀으므로 상한입니다.
Host 가 상호작용 하나에 쓸 수 있는 0.5 ms 를 넘으며, HTML 경로에서 다음으로 줄일 것이 이것입니다.

---

## 저장소 구조

```
dioxus-compose/
├─ dioxus-compose/        # 크레이트: Dioxus 어댑터와 HTML 경로
│  ├─ src/html.rs         #   dioxus_compose::html: HtmlDom, HtmlConfig, TextMeasurer, ImageResolver
│  ├─ src/dom, layout, paint  # blitz-dom 문서와 이벤트, 레이아웃 단계, 그리기 목록과 계획
│  ├─ examples/           #   desktop_demo, 위젯 경로 데모
│  └─ tests/              #   크레이트 테스트
├─ samples/               # HTML 과 CSS 로 쓴 앱 열한 개, 하나에 크레이트 하나
├─ experiments/           # 남겨 둘 만한 실험
├─ scripts/               # 품질 검사와 저장소 도구
└─ docs/
   ├─ guide/              # 사용자 가이드 사이트(영어와 한국어)
   └─ locales/            # 이 README 의 한국어판
```

렌더러와 그 빌드, 디자인 시스템은 [compose-rust](https://github.com/DarkPyonix/compose-rust) 에
있습니다.

---

## 기여하기

이슈와 풀 리퀘스트를 환영합니다. 작업은 `develop` 에서 합니다.

- 변경에는 그것이 없었다면 잡아냈을 테스트가 함께 오고, 버그 수정은 그 버그를 재현하는 테스트로
  시작합니다.
- `cargo test --workspace` 가 전부를 돌리고, `./scripts/check.sh` 는 포맷, 경고를 오류로 다루는 Clippy,
  벤치마크를 더합니다.
- 커밋 하나에는 논리적 변경 하나를 담고, 제목은 `Feat: Add select to the HTML path` 처럼 씁니다
  (`Feat`, `Fix`, `Refactor`, `Docs`, `Test`, `Chore`).

---

## 문서

**가이드: <http://darkpyonix.dev/dioxus-compose/>**, 영어와 한국어입니다. 시작하기, 레이아웃과 CSS,
텍스트, 폼과 이벤트, 이미지, 스크롤과 오버레이, 테마, 위젯 경로, 그리고 모든 부분의 현황을 다룹니다.

---

## 라이선스

[Apache License 2.0](https://github.com/DarkPyonix/dioxus-compose/blob/main/LICENSE).
