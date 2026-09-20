# SPEC

이 문서는 dioxus-compose의 요구사항과 수용 기준을 정의합니다. 근거는 [INTENT.md](INTENT.md)에 있습니다.

- 상태 표기: `Draft`(합의 전), `Agreed`(구현 가능), `Done`(수용 기준 통과)
- 요구사항 ID는 한 번 부여하면 재사용하지 않습니다. 폐기할 때는 `Withdrawn`으로 남깁니다.

## 1. 용어

| 용어 | 정의 |
|---|---|
| Host | Rust 프로세스. 사용자 UI 코드(Dioxus 컴포넌트)와 도메인 로직을 소유합니다 |
| Renderer | AOT 컴파일된 Kotlin/Compose 네이티브 라이브러리. 스키마 인터프리터를 포함합니다 |
| Schema | Renderer가 해석할 수 있는 위젯 타입, 속성, Modifier, 이벤트의 닫힌 집합 |
| Mutation | 노드 트리 변경 명령 한 단위 |
| Event | Renderer에서 Host로 올라오는 사용자 입력 알림 |
| Node ID | Host가 발급하는 노드 식별자. 해당 세션 안에서 고유합니다 |
| Handler ID | Host가 발급하는 이벤트 핸들러 식별자 |

## 2. 아키텍처

```
┌──────────────── Host (Rust) ────────────────┐
│ 사용자 컴포넌트 (rsx!, hooks)                  │
│ dioxus-core VirtualDom                       │
│ dioxus-compose renderer: Mutations → bytes   │
└───────────────────┬─────────────────────────┘
                    │  동기 직접 호출 (UI 스레드) + 배치 버퍼
┌───────────────────┴──── Renderer (Kotlin) ───┐
│ 코드젠 심 (@CEntryPoint / @CName / JNI / wasm) │
│ 프로토콜 디코더 → Node 테이블 (snapshot state)  │
│ 스키마 인터프리터 @Composable RenderNode       │
│ Compose Desktop(AWT) / Compose iOS(UIKit)    │
└──────────────────────────────────────────────┘
```

## 3. 기능 요구사항

### FR-1 노드 트리 구성 — `Agreed`
Host는 Mutation 시퀀스로 Renderer의 노드 트리를 생성, 수정, 삭제, 이동할 수 있어야 합니다.
- 수용 기준: `Create`, `SetProp`, `SetModifier`, `Insert`, `Move`, `Remove`로 임의의 트리를 만들 수 있고, 적용 결과가 Renderer의 트리 덤프와 일치합니다.

### FR-2 스키마 기반 렌더링 — `Agreed`
Renderer는 스키마에 정의된 위젯 타입만 해석해서 해당 Compose 컴포저블로 렌더링합니다.
- 최소 스키마(M0): `Column`, `Row`, `Box`, `Text`, `TextField`, `Button`, `Spacer`
- 디자인 확장(M2): `ScrollColumn`, `LazyColumn`. 값 모델은 FR-13, 테마는 FR-14를 따릅니다
- 수용 기준: 스키마에 없는 타입이나 속성을 받으면 크래시하지 않고 `ProtocolError` 이벤트를 보냅니다.

### FR-3 이벤트 전달 — `Agreed`
사용자 입력은 `(node_id, handler_id, payload)` 형태로 Host 핸들러를 **동기로 직접 호출**합니다(PR-1). 핸들러는 반환값을 돌려줄 수 있습니다. 클로저는 경계를 넘지 않습니다.
- 수용 기준: Button 클릭이 등록된 Rust 핸들러를 정확히 한 번 호출합니다.

### FR-4 상태 갱신 반영 — `Agreed`
Host 상태가 변경되면 변경분만 전송하고, Renderer는 해당 노드만 recomposition합니다.
- 수용 기준: Text 하나의 내용을 바꿀 때 전송되는 Mutation은 `SetProp` 1건입니다. 형제 노드는 recomposition되지 않습니다(recomposition 카운터로 확인).

### FR-5 비제어 TextField — `Agreed`
- TextField의 편집 값과 조합 상태는 Renderer가 소유합니다.
- Renderer는 변경을 알림 이벤트(`TextChanged`, 디바운스 적용)와 확정 이벤트(`TextSubmitted`, `FocusLost`)로 보냅니다.
- Host가 값을 바꿀 때는 명시적 명령 `SetText(node_id, text, selection)`을 씁니다. Renderer는 IME 조합이 진행 중이면 조합이 끝날 때까지 적용을 미룹니다.
- 수용 기준: §6 IME 체크리스트를 통과합니다.

### FR-6 Dioxus 렌더러 — `Agreed`
`dioxus-core` VirtualDom의 `Mutations`를 프로토콜 Mutation으로 변환하는 렌더러를 제공합니다.
- 사용자 코드는 `rsx!`와 훅만으로 작성하고, 프로토콜을 직접 다루지 않습니다.
- 수용 기준: M0 화면을 `rsx!` 컴포넌트로 재작성했을 때 동일하게 동작합니다.

### FR-7 스키마 코드젠 — `Draft`
위젯, 속성, Modifier, 이벤트 페이로드는 Rust에서 단일 소스로 정의하고 Kotlin 타입과 코덱을 생성합니다.
- 양쪽 모두 exhaustive match가 적용됩니다(Rust `enum`은 Kotlin `sealed interface`로 생성).
- 핸드셰이크 때 스키마 해시를 비교해서 불일치하면 초기화를 실패시킵니다.
- 수용 기준: Rust 스키마에 속성을 추가하고 Kotlin 인터프리터를 갱신하지 않으면 **빌드가 실패**합니다.

### FR-8 LazyColumn 윈도잉 — `Draft`
- Host는 아이템 총 개수와 안정적인 key를 알립니다.
- Renderer는 보이는 범위를 `RangeRequested`로 요청하고, Host는 그 구간의 서브트리만 생성합니다.
- 수용 기준: 아이템 10,000개 목록에서 생성된 노드 수가 가시 범위와 버퍼에 비례합니다.

### FR-9 스트리밍 텍스트 — `Draft`
LLM 응답 스트리밍을 위해 Text 노드에 `AppendText` 명령을 둡니다. Host는 프레임 주기(약 16ms) 단위로 토큰을 모아서 보냅니다.
- 수용 기준: 초당 100토큰 스트리밍 중에도 스크롤과 입력이 끊기지 않습니다.

### FR-12 이벤트 소비(consume) — `Agreed`
Dioxus 0.7의 이벤트 핸들러는 반환값이 없습니다. 그래서 핸들러가 **이벤트 객체에 소비 표시를 남기고**, 경계가 그 값을 읽어 `MutationBatch.result`로 돌려줍니다. 웹의 `preventDefault()`, Compose의 `PointerInputChange.consume()`과 같은 모델입니다.

```rust
rsx! {
    TextField {
        on_key_down: move |event| {
            if event.key() == Key::Enter && !event.shift_key() {
                submit();
                event.consume();   // Renderer의 onKeyEvent가 true를 반환합니다
            }
        }
    }
}
```

Renderer 측 규칙:
- `Modifier.onKeyEvent`에 연결합니다. `onPreviewKeyEvent`는 전역 단축키처럼 가로채야 하는 경우에만 씁니다.
- **IME 조합 중에는 키 이벤트를 Host로 보내지 않습니다.** 조합 중의 Enter는 제출이 아니라 조합 확정입니다. 이를 어기면 한글 입력 중 Enter에서 조합 중이던 글자가 사라진 채 제출됩니다(FR-5).
- 포인터 이벤트도 같은 방식으로 `PointerInputChange.consume()`에 대응시킵니다.

Host 측 규칙:
- 핸들러 실행 중에 Renderer를 동기로 호출하지 않습니다(재진입 금지). 상태만 바꾸고 프레임 요청으로 넘깁니다.
- 수용 기준: 멀티라인 TextField에서 Enter는 제출되고 줄바꿈이 생기지 않으며, Shift+Enter는 줄바꿈만 생기고 제출되지 않습니다. 한글 조합 중 Enter는 조합만 확정합니다.

### FR-10 Modifier 값 모델 — `Agreed`
Modifier는 값 리스트로 직렬화합니다. 예: `[Padding(16), FillMaxWidth, Background(argb), Clickable(handler_id)]`. Renderer는 이를 `Modifier` 체인으로 재구성합니다.

### FR-13 디자인 프리미티브 — `Draft`
위젯만으로는 디자인을 할 수 없습니다. 스키마에 **값 모델**이 필요합니다. 값은 고정 레이아웃 레코드(PR-4)를 넘어야 하므로, Modifier 한 변형이 쓸 수 있는 공간은 `(tag: u16, first: u64, second: u64)`뿐입니다. 아래 프리미티브는 모두 이 한도 안에 들어갑니다.

원칙: **역할(role)을 우선하고 리터럴은 탈출구로 둡니다.** 역할은 FR-14의 디자인 시스템이 해석하고, 리터럴은 그대로 그립니다.

#### 13.1 색: `Color`와 `ColorRole`, 그리고 `Paint`
- `Color`는 `u32` ARGB 한 개입니다(`#[repr(transparent)]`). 그라데이션과 이미지 브러시는 넣지 않습니다.
- `ColorRole`은 의미 슬롯입니다: `Primary`, `OnPrimary`, `Secondary`, `OnSecondary`, `Surface`, `OnSurface`, `SurfaceVariant`, `OnSurfaceVariant`, `Background`, `OnBackground`, `Outline`, `OutlineVariant`, `Error`, `OnError`.
- `Paint`는 둘 중 하나입니다: `Paint::Role(ColorRole)` 또는 `Paint::Literal(Color)`. `u64` 하나로 인코딩합니다(상위 32비트 = 종류, 하위 32비트 = 값).
- 색을 받는 자리는 전부 `Paint`를 씁니다. 색 표현이 스키마에 두 번 등장하지 않게 하기 위해서입니다.
- 수용 기준: `Modifier::Background(Paint::Role(ColorRole::Surface))`와 `Modifier::Background(Paint::Literal(Color::rgb(0x1B1B1F)))`가 같은 레코드 길이로 왕복하고, 디코드 결과가 입력과 같습니다.

#### 13.2 타이포그래피
- `TypeRole`은 9단 사다리입니다: `Display`, `Headline`, `Title`, `Subtitle`, `Body`, `BodyStrong`, `Label`, `Caption`, `Mono`. 세 디자인 시스템의 타입 스케일이 모두 이 사다리에 대응합니다.
- `Text`의 속성으로 개별 재정의를 둡니다. 각각 독립된 `SetProp`이라서 FR-4의 변경분 전송이 유지됩니다.
  - `type_role`, `font_size`(sp), `font_weight`(100~900), `line_height`, `letter_spacing`, `color`(`Paint`), `text_align`(`Start|Center|End|Justify`), `max_lines`, `overflow`(`Clip|Ellipsis|Visible`)
- `type_role`만 지정한 Text는 디자인 시스템이 정한 크기·굵기·행간·자간을 그대로 씁니다. 재정의 속성이 있으면 그 축만 덮어씁니다.
- 수용 기준: `Text { type_role: Title }` 한 개는 `SetProp` 1건만 보냅니다. `font_size`만 바꾸면 추가 `SetProp` 1건만 전송됩니다.

#### 13.3 모양: `Shape`와 `Border`
- `Modifier::Shape { top_start, top_end, bottom_end, bottom_start }` — f32 4개를 `u64` 2개에 담습니다.
- `Modifier::ShapeRole(ShapeRole)` — `None|ExtraSmall|Small|Medium|Large|Full`. 실제 반지름은 디자인 시스템이 정합니다. Material 3의 12dp, HIG의 연속 곡률 느낌, Fluent의 4dp가 여기서 갈립니다.
- `Modifier::Border { width, paint }` — `first` 하위 32비트에 너비, `second`에 `Paint`.
- 테두리와 모양은 서로 독립입니다. Renderer는 `ShapeRole`/`Shape` 중 마지막에 적용된 것을 클립과 테두리 모두에 씁니다.

#### 13.4 간격과 배치
- `SpaceRole`: `None|Xs|Sm|Md|Lg|Xl|Xxl`. 밀도가 디자인 시스템마다 다른 부분이라 리터럴 dp보다 역할이 먼저입니다.
- `Modifier::PaddingRole(SpaceRole)`, 기존 `Modifier::Padding(f32)`는 유지합니다.
- `Modifier::PaddingEach { start, top, end, bottom }` — f32 4개.
- `Modifier::Weight(f32)` — `RowScope`/`ColumnScope`의 weight입니다.
- `Column`/`Row` 속성: `arrangement`(`Start|Center|End|SpaceBetween|SpaceAround|SpaceEvenly`), `spacing`(f32 dp) 또는 `space_role`, `alignment`(교차축 정렬).
- `Box` 속성: `alignment`(9점 정렬).
- 수용 기준: 위 속성/Modifier가 전부 `(tag, u64, u64)` 안에 들어가고, 인코딩 후 디코딩 결과가 입력과 같습니다.

#### 13.5 고도(elevation)
- `Modifier::Elevation(f32 dp)` 하나만 둡니다. **그림자를 어떻게 그릴지는 디자인 시스템의 규칙입니다.** Material 3는 톤 상승 + 그림자, HIG는 넓고 옅은 그림자, Fluent는 층 그림자 + 가는 스트로크로 같은 값을 다르게 해석합니다.
- 그림자 색·오프셋·블러를 Host가 지정하는 경로는 두지 않습니다. 두면 디자인 시스템이 값만 받는 껍데기가 됩니다.

#### 13.6 스크롤 컨테이너
- `ScrollColumn`: 콘텐츠 전체를 구성하고 세로 스크롤만 붙입니다. 스크롤 위치는 Renderer가 소유합니다(D5).
- `LazyColumn`: FR-8의 윈도잉 프로토콜을 씁니다.
- 가로 스크롤 컨테이너는 넣지 않습니다. 세 샘플 어디에도 필요가 없고, 넣으면 검증되지 않은 위젯이 하나 늘어납니다.

#### 13.7 넣지 않은 것과 이유
| 제외 | 이유 |
|---|---|
| 그라데이션, 이미지 브러시 | `(u64, u64)`에 들어가지 않고, 리소스가 경계를 넘어야 합니다(PR-4 위반) |
| 폰트 패밀리, 커스텀 폰트 | 폰트 리소스는 Renderer 번들에 있습니다. Host가 이름을 보내면 존재 검증이 런타임으로 밀립니다 |
| 아이콘·이미지 위젯 | 에셋 전달 프로토콜이 따로 필요합니다. 별도 요구사항으로 분리합니다 |
| 애니메이션 스펙(duration, easing) | 모션은 디자인 시스템의 규칙입니다(FR-14). Host가 값을 주면 13.5와 같은 이유로 무너집니다 |
| 블러/머티리얼(HIG vibrancy), 리플 설정 | 플랫폼 전용 효과라 세 시스템 공통 축이 아닙니다 |
| Host가 보내는 토큰 테이블 | FR-14에서 해석 위치를 Renderer로 정했습니다. 테이블을 보내면 그 결정이 뒤집힙니다 |

### FR-14 디자인 시스템과 테마 모드 — `Draft`
디자인 시스템은 **토큰 집합 + 컴포넌트 스타일 규칙**의 한 쌍입니다. 속성을 모아 놓은 것이 아닙니다. 1급으로 지원하는 세 가지는 Material 3, Apple HIG, WinUI/Fluent입니다.

#### 14.1 추상화
- 위젯은 **역할만 내보냅니다**(FR-13의 `ColorRole`, `TypeRole`, `ShapeRole`, `SpaceRole`, 그리고 `ButtonVariant` 같은 컴포넌트 변형).
- 디자인 시스템은 Renderer 안에 있는 **토큰 테이블 + 컴포넌트 규칙 구현** 한 쌍입니다.
- 따라서 **네 번째 디자인 시스템을 추가할 때 위젯 코드, 속성, Modifier, 와이어 포맷은 건드리지 않습니다.** Rust `DesignSystem` enum에 변형 1개, Kotlin에 테이블 1개와 규칙 구현 1개를 더하면 끝입니다.
- 수용 기준: `DesignSystem`에 변형을 하나 추가했을 때 `widgets.rs`의 위젯 정의와 Modifier/Property 스키마가 변경되지 않습니다.

#### 14.2 컴포넌트 변형
컴포넌트 규칙이 붙는 자리는 변형(variant) 속성입니다. 값은 디자인 시스템 중립 이름입니다.
- `Button.variant`: `Filled | Tonal | Outlined | Text`
  - Material 3: Filled/Tonal/Outlined/Text 버튼, 큰 곡률, 리플.
  - HIG: Filled은 강조 버튼(연속 곡률, 그림자 없음), Tonal은 회색 배경, Text는 내용 색만 쓰는 plain 버튼. 리플 대신 하이라이트.
  - Fluent: Accent/Standard/Standard+stroke/Subtle, 4dp 곡률, 위쪽 밝은 테두리.
- 같은 rsx 코드가 시스템에 따라 다른 모양으로 그려지는 것이 정상 동작입니다.

#### 14.3 두 가지 테마 모드
애플리케이션이 **명시적으로** 고릅니다.

```rust
// 모든 플랫폼에서 같은 디자인 시스템
LaunchBuilder::new().with_theme(Theme::unified(DesignSystem::Material3)).launch(app);
// 호스트 플랫폼을 따라감. 대응이 없는 플랫폼에서 쓸 시스템을 반드시 함께 지정합니다
LaunchBuilder::new().with_theme(Theme::adaptive(DesignSystem::Material3)).launch(app);
```

- `adaptive`는 **기본값이 아닙니다.** `with_theme`을 부르지 않으면 `Theme::unified(DesignSystem::Material3)`입니다. 기본값으로 플랫폼마다 다르게 보이는 동작은 두지 않습니다.
- `Theme::adaptive`는 fallback 인자가 **필수**입니다. 그래서 adaptive에 "대응이 애매한 플랫폼"이 남지 않습니다.

| 플랫폼 | `adaptive`가 고르는 시스템 |
|---|---|
| Android | Material 3 |
| macOS, iOS | Apple HIG |
| Windows | WinUI/Fluent |
| Linux | fallback 인자 |
| Web | fallback 인자 |

- Linux에 GNOME/Adwaita를 매핑하지 않는 이유: Adwaita는 1급 지원 대상이 아니고, 셋 중 하나를 임의로 고르면 앱 저자가 의도하지 않은 모양이 됩니다. 고르게 하는 편이 정직합니다.
- 명암(`ColorScheme`)은 `Light | Dark | FollowSystem`이고 기본은 `FollowSystem`입니다. 시스템 설정 변화는 Renderer가 먼저 알고 스스로 반영합니다. Host는 관여하지 않습니다(D5).

#### 14.4 해석 위치: Renderer
**토큰 해석과 컴포넌트 규칙은 Renderer가 수행합니다.** Host는 역할과 선택만 보냅니다.

근거:
- **PR-3/NFR-9(프레임 예산).** Host는 Renderer UI 스레드에서 돌기 때문에 Host의 작업이 그대로 프레임 예산(§5.1, 상호작용당 ≤ 0.5ms)에서 빠집니다. Host가 토큰을 푼다면 다크모드 전환이나 플랫폼 테마 변경이 트리 전체에 대한 `SetProp` 재전송(O(노드 수))이 됩니다. Renderer가 풀면 같은 변경이 `SetTheme` 1건이고, 나머지는 Compose의 `CompositionLocal` 무효화로 끝납니다.
- **PR-1(동기 경계).** 시스템 명암 전환과 플랫폼 식별은 Renderer 쪽 정보입니다. Host가 해석하려면 Renderer→Host 질의가 필요한데, 경계는 동기 단방향 호출 모델이라 질의를 추가하면 PR-2의 표면이 늘어납니다.
- **D5.** 테마는 UI 로컬 상태입니다. 스크롤 위치·포커스와 같은 부류입니다.
- 비용: Host 쪽 단위 테스트는 "어떤 역할을 보냈는가"까지만 검증할 수 있고, 실제 색·치수는 Renderer 테스트에서 검증합니다. 이 분리를 받아들입니다.

#### 14.5 와이어 추가분
- Mutation `SetTheme { design_system: u16, adaptive: bool, fallback: u16, color_scheme: u16 }` — 루트(`node_id` 없음)에 적용합니다. Host는 초기 배치의 첫 레코드로 1회 보내고, 앱이 테마를 바꿀 때만 다시 보냅니다.
- `DesignSystem` 태그: `Material3 = 1`, `AppleHig = 2`, `Fluent = 3`.
- `ColorScheme` 태그: `Light = 1`, `Dark = 2`, `FollowSystem = 3`.
- 수용 기준: `Theme::unified(...)`로 띄운 앱의 첫 배치 첫 레코드가 `SetTheme`이고 `adaptive = false`입니다. `Theme::adaptive(...)`이면 `adaptive = true`이며 `fallback`이 인자로 준 시스템입니다.

#### 14.6 Renderer 구현자가 채워야 할 표
디자인 시스템마다 아래 7개를 채웁니다. 채워지면 위젯 코드는 건드리지 않습니다.
1. `ColorRole` 14개 × {Light, Dark} 색값
2. `TypeRole` 9개 → 크기/굵기/행간/자간/폰트
3. `ShapeRole` 6개 → 곡률(HIG는 연속 곡률)
4. `SpaceRole` 7개 → dp
5. `Modifier::Elevation(dp)` → 그림자/톤/스트로크 렌더링 규칙
6. `ButtonVariant` 4개 → 배경·전경·테두리·눌림 표현
7. 모션: 상태 전환 duration과 easing

### FR-11 스키마 확장 (서드파티 위젯) — `Draft`
Q2: 스키마에 없는 Compose 컴포넌트를 쓰는 방식입니다. 후보는 다음과 같습니다.

| 안 | 방식 | 장점 | 단점 |
|---|---|---|---|
| E1 확장 스키마 패키지 | 위젯 정의(Rust)와 인터프리터 구현(Kotlin)을 한 쌍으로 묶은 확장 단위. 빌드 시 코드젠으로 스키마에 병합 | 타입 안전(FR-7 그대로), 오버헤드 없음 | Renderer 재빌드 필요 |
| E2 Kotlin 어노테이션 기반 역생성 | `@DioxusWidget` 붙인 `@Composable` 함수에서 KSP로 Rust 컴포넌트와 스키마 생성 | Compose 라이브러리 래핑 비용이 최소 | 단일 소스가 Kotlin으로 뒤집힘(D6와 충돌 검토 필요), 파라미터 타입 매핑 한계 |
| E3 동적 Slot 위젯 | `Custom { kind: "id", props: bytes }` 범용 노드, Renderer 측 레지스트리 | 스키마 변경 없이 추가 | 타입 안전이 런타임 검사로 격하 (C3 위반 소지) |

잠정 권장: **E1을 기본**으로, 대량 래핑에 한해 E2를 E1 산출물을 만드는 보조 도구로 씁니다. E3는 채택하지 않습니다. AOT 특성상 어느 쪽이든 Renderer 재빌드는 필수입니다.

## 4. 경계 프로토콜

### PR-1 호출 모델: 동기·동일 스레드 직접 호출 — `Agreed`
옛 React Native 브리지처럼 비동기 큐를 두면 병목이 생깁니다. 비동기 큐는 동기 반환값을 받을 수 없고, 스레드 홉 때문에 최대 1프레임 지연이 생깁니다. 그래서 JSI처럼 **같은 스레드에서 서로를 직접 호출**합니다.

- VirtualDom은 **Renderer의 UI 스레드에서** 돕니다. 이 스레드는 Host의 전용 스레드가 아닙니다.
- 사용자 입력이 들어오면 Renderer가 Host 핸들러를 직접 호출합니다. Host는 그 자리에서 핸들러를 실행하고 diff를 계산한 뒤, 결과 Mutation 배치와 반환값을 돌려줍니다.
- 동기 반환값을 지원합니다. 예: `onKeyEvent`의 "처리됨" 여부. Enter는 제출, Shift+Enter는 줄바꿈으로 나누는 처리가 여기에 해당합니다. 표현 방식은 FR-12를 따릅니다.
- 경계에 비동기 큐를 두지 않습니다. 스레드 간 통신은 PR-3의 wake 신호 하나뿐입니다.

### PR-2 경계 표면 — `Draft`
경계는 primitive, 포인터, 길이만 씁니다(GraalVM `@CEntryPoint` 제약). 함수는 호출 방향에 중립적인 **논리 연산**으로 정의하고, 플랫폼별 심은 코드젠(FR-7)이 생성합니다. 사람이 JNI나 cinterop 코드를 직접 쓰지 않습니다.

C 심볼은 Rust 쪽 관례(snake_case, 크레이트 이름 접두사)를 따릅니다. Kotlin 쪽 선언은 코드젠이 Compose 관례에 맞춰 생성합니다. 명명 규칙 전체는 PR-7에 있습니다.

Renderer → Host (Rust가 export):
```c
// 핸드셰이크: 스키마 해시, 버전, LoopMode를 교환하고 초기 트리 배치를 받습니다
int32_t dioxus_compose_host_init(const uint8_t* handshake, uint32_t len, MutationBatch* out);
// 이벤트 1건 처리: 핸들러를 실행한 뒤 diff를 계산하고, 결과 배치와 반환값을 돌려줍니다
int32_t dioxus_compose_host_dispatch_event(const uint8_t* event, uint32_t len, MutationBatch* out);
// 프레임 요청 이후 호출: 워커에서 온 상태 변경을 반영해 diff를 계산합니다
int32_t dioxus_compose_host_render_frame(uint64_t frame_time_nanos, MutationBatch* out);
void    dioxus_compose_host_release_batch(MutationBatch* batch);
void    dioxus_compose_host_shutdown(void);
```

Host → Renderer (Kotlin이 export):
```c
int32_t dioxus_compose_renderer_run(void);            // LoopMode::Renderer일 때만. 블로킹
void    dioxus_compose_renderer_request_frame(void);  // 스레드 안전. 다음 프레임에 render_frame 예약
```

- `LoopMode`
  - `LoopMode::Renderer`: Desktop, iOS. Rust `main`에서 `dioxus_compose::launch(app)`가 `dioxus_compose_renderer_run`을 호출합니다.
  - `LoopMode::Platform`: Android, Web. 플랫폼이 루프를 소유하고, Renderer가 먼저 `dioxus_compose_host_init`을 호출합니다.
  - 사용자 코드는 `fn app() -> Element`뿐이라 두 모드에서 동일합니다.
- GraalVM에서는 진입점마다 `IsolateThread*`가 붙습니다. 코드젠이 이를 숨깁니다. isolate는 프로세스당 1개입니다.
- `dispatch_event`와 `render_frame`이 반환한 배치는 Renderer가 **같은 호출 스택 안에서** 적용하고 즉시 `release_batch`합니다. 배치를 쌓아 두는 큐는 없습니다.
- 배치 하나는 단일 스냅샷 트랜잭션(`Snapshot.withMutableSnapshot`)으로 적용합니다. 중간 상태가 화면에 그려져서는 안 됩니다.

### PR-3 스레드 규칙 — `Agreed`
- VirtualDom, 사용자 컴포넌트, 모든 `dioxus_compose_host_*` 호출은 Renderer UI 스레드에서만 실행합니다. 그래서 락이 필요 없습니다.
- **UI 스레드에서 도메인 작업을 금지합니다.** PTY, 네트워크, LLM 스트리밍, 파일 I/O 같은 작업은 Host 워커 스레드(tokio 등)에서 돌립니다. 워커는 Dioxus signal로 상태를 갱신하고, Host가 내부에서 `request_frame`을 호출합니다. 사용자 코드는 경계 함수를 직접 부르지 않습니다.
- `request_frame`은 여러 번 불러도 다음 프레임에 `render_frame` 1회로 합쳐집니다. Compose frame clock(`withFrameNanos`) 안에서 실행됩니다.
- macOS에서 `dioxus_compose_renderer_run`은 프로세스 메인 스레드에서 호출해야 합니다(AppKit 요구사항).
- Android: Host 워커 스레드는 `request_frame`을 부르기 위해 JavaVM에 **1회 영구 attach**합니다. 호출마다 attach하는 것은 금지합니다. `@FastNative`/`@CriticalNative`는 짧은 호출에만 허용합니다.
- 프레임 예산은 NFR-9를 따릅니다.

### PR-4 배치 버퍼와 인코딩 — `Draft`
원칙: **같은 프로세스 안이므로 직렬화, 복사, 경계 호출 횟수를 최소화합니다.** 버퍼는 큐가 아니라 **한 번의 호출에서 Mutation 여러 개를 넘기는 인자**입니다.

- `#[repr(C)] struct MutationBatch { ptr: *const u8, len: u32, result: i64 }`
  - Host가 소유하는 arena를 가리키며, 프레임마다 재사용합니다.
  - `result`에는 핸들러의 동기 반환값이 들어갑니다.
- **인코딩**: 고정 레이아웃 바이너리로, 제자리에서 읽습니다(zero-copy). 디코드 단계를 두지 않습니다.
  - 레코드: `tag: u16`, `len: u16`, 뒤이어 고정 필드(`node_id: u32` 등)가 오며, 리틀 엔디언이고 4바이트 정렬입니다.
  - 문자열: 같은 arena에 두고 `(offset: u32, len: u32)`로 참조합니다. UTF-8입니다. Renderer는 Compose에 넘기는 시점에만 `String`으로 변환합니다.
  - postcard, bincode, FlatBuffers는 쓰지 않습니다. 레코드 레이아웃과 접근자는 FR-7 코드젠이 생성합니다.
- **이벤트 태그** (Renderer→Host): 입력 이벤트(FR-3), `TextChanged`/`TextSubmitted`/`FocusLost`(FR-5), `RangeRequested`(FR-8), `Lifecycle(Resumed|Paused|Destroyed)`, `Resync`, `SaveState`/`RestoreState`(선택), `ProtocolError`
- **플랫폼별 메모리 접근**
  - Desktop(GraalVM): `Pointer`로 직접 읽습니다.
  - iOS(Kotlin/Native): `CPointer`로 읽습니다.
  - Android: `NewDirectByteBuffer`로 arena를 감싸서 읽습니다.
  - Web: 공유 linear memory(PR-6)
- 수용 기준: 텍스트 하나를 바꾸는 이벤트 처리에서 경계 호출 2회(`dispatch_event`, `release_batch`), 힙 할당은 Compose `String` 생성 1회 이하

### PR-5 Android — `Draft`
- 호스트 관계: Kotlin Activity가 프로세스와 루프를 소유합니다(`LoopMode::Platform`). Rust는 cdylib입니다. VirtualDom은 PR-3에 따라 UI 스레드에서 돕니다.
- JNI 심: PR-2의 논리 연산에서 jni-rs 기반 심과 Kotlin `external fun` 선언을 코드젠으로 생성합니다. UniFFI(JNA 경유)는 호출당 오버헤드가 커서 쓰지 않습니다.
- 생명주기:
  - Surface 파괴와 config change: Compose만 재구성됩니다. VirtualDom은 프로세스 전역에 유지됩니다. Renderer는 재구성 후 `Resync`를 보내고, Host는 전체 트리 배치를 돌려줍니다.
  - `onStop`/`onStart`: `Lifecycle` 이벤트를 보냅니다. Host는 타이머와 애니메이션을 억제합니다.
  - 프로세스 kill: 메모리 상태는 복원하지 않습니다. 필요하면 `SaveState`로 작은 blob을 `onSaveInstanceState`에 저장합니다.
- android-activity, NativeActivity, GameActivity 진입점은 쓰지 않습니다. ComposeView와 공존한 사례가 없고 IME 충돌 위험이 있습니다. JavaVM은 `JNI_OnLoad`에서 얻습니다.
- 수용 기준(M6):
  1. 일반 JNI와 `@FastNative`의 호출당 비용을 실측합니다. 공개 수치(약 115ns, 약 35ns)와 비교해 기록합니다.
  2. M0 화면을 같은 Rust 소스로 띄우고, 초당 100토큰 스트리밍 중 프레임 끊김이 없음을 Macrobenchmark `FrameTimingMetric`으로 확인합니다.
  3. 화면 회전, 다크모드 전환, 홈→복귀, `am kill` 후 복귀에서 크래시가 없습니다.

### PR-6 Web 직결 — `Draft`
Rust(wasm32)와 Kotlin/Wasm 모듈을 **JS 글루를 거치지 않고** 연결합니다. `LoopMode::Platform`입니다.

- 함수: 한쪽 모듈의 wasm export를 다른 쪽 모듈의 wasm import로 직접 연결합니다. JS는 인스턴스화 시점의 배선에만 쓰고, 호출 경로에는 JS 프레임이 없습니다.
- 데이터: PR-4의 arena를 `WebAssembly.Memory` 하나에 두고 양쪽이 공유합니다.
- **검증 필요** (Q3)
  - Kotlin/Wasm은 WasmGC 기반입니다. 외부 linear memory를 import해서 직접 읽는 경로(`kotlin.wasm.unsafe` 등)가 어디까지 지원되는지 확인해야 합니다.
  - 브라우저 엔진이 wasm↔wasm import 호출을 JS 트램폴린 없이 처리하는지 벤치마크로 확인해야 합니다.
- 불가능하면 대안(단일 모듈 링크 등)을 조사한 뒤 INTENT를 갱신합니다. JS 브리지로 되돌아가는 대안은 허용하지 않습니다.

### PR-7 명명 규칙 — `Agreed`
각 언어 생태계의 관례를 따릅니다. 한쪽 관례를 다른 쪽에 억지로 맞추지 않습니다.

| 영역 | 관례 | 예 |
|---|---|---|
| C ABI 심볼 | snake_case, `dioxus_compose_{host,renderer}_` 접두사, 동사구 | `dioxus_compose_host_dispatch_event` |
| Rust 공개 API | Dioxus 관례를 따릅니다: `launch`, `LaunchBuilder`, `use_*` 훅, PascalCase 컴포넌트 | `dioxus_compose::launch(app)`, `use_text_field()` |
| Rust 타입 | PascalCase, `#[repr(C)]` 경계 타입은 역할 이름 | `MutationBatch`, `LoopMode`, `HostEvent` |
| rsx 위젯 | Compose 이름을 그대로 씁니다 | `Column`, `Row`, `LazyColumn`, `TextField` |
| rsx 속성과 Modifier | snake_case, Compose 이름을 옮긴 것 | `fill_max_width`, `padding`, `on_click`, `on_value_change` |
| Kotlin 선언 | camelCase 함수, PascalCase 타입, 컴포저블은 PascalCase 명사 | `DioxusContent(host)`, `rememberDioxusHost()`, `HostBridge.dispatchEvent()` |
| Kotlin 생성 코드 | `generated` 패키지, 파일 이름 접미사 `.gen.kt` | `org.thisisthepy.dioxus.compose.generated` |
| 이벤트/Mutation 태그 | Rust `enum` 변형은 PascalCase, Kotlin `sealed interface` 하위 타입은 같은 이름 | `SetText`, `TextSubmitted` |

- Compose에 같은 개념이 있으면 그 이름을 씁니다(`Modifier`, `Recomposition`, `requestFrame`). 새 이름을 만들지 않습니다.
- Dioxus에 같은 개념이 있으면 Rust 쪽은 Dioxus 이름을 씁니다(`VirtualDom`, `Mutations`, `ElementId`).
- 두 이름이 충돌하면 Rust 쪽은 Dioxus 이름을, Kotlin 쪽은 Compose 이름을 쓰고, 대응 관계를 코드젠 스키마에 기록합니다.

### PR-8 macOS 런타임 요건 — `Draft`
- 빌드 도구는 Liberica NIK 25 Full입니다(INTENT D9-macOS).
- 배포 레이아웃은 `<root>/lib/` 하나이며 `java.home`은 그 부모입니다. 렌더러는 자기 라이브러리 경로를 dladdr로 얻어 `java.home`, `skiko.library.path`, `skiko.data.path`를 설정합니다.
- `lib/`에 함께 두는 파일: 렌더러 라이브러리, Skia(`libskiko-macos-<arch>.dylib`), `libjawt.dylib` 포워더, `libawt_lwawt.dylib` 자리 채우기.
- `dioxus_compose_renderer_run`은 프로세스 메인 스레드에서 호출해야 합니다. 아니면 `RUN_NOT_MAIN_THREAD`를 반환합니다.
- 수용 기준: C 호스트가 라이브러리를 링크해 `run`을 호출하면 창이 뜨고, 창을 닫으면 `run`이 0을 반환하며 프로세스가 정상 종료됩니다. **(2026-09-20 통과)**

## 5. 비기능 요구사항

| ID | 요구사항 | 수용 기준 | 상태 |
|---|---|---|---|
| NFR-1 | JVM 불필요 | 배포물에 JRE가 없고, `java`가 없는 머신에서 실행됨 | Agreed |
| NFR-2 | 웹뷰 불필요 | WKWebView, WebView2, WebKitGTK에 링크하지 않음 | Agreed |
| NFR-3 | 데스크톱 무게 | 빈 창 RSS < 100MB, 배포 용량 < 100MB (목표치, M1에서 측정 후 확정) | Draft |
| NFR-4 | 플랫폼 | macOS, Windows, Linux 데스크톱, iOS, Android, Web(wasm). Android와 Web은 PR-5, PR-6 참조 | Agreed |
| NFR-5 | 개발 경험 | Renderer는 JVM 개발 셸에서 hot reload와 `@Preview`로 작업 가능. native-image 빌드는 개발 루프에 필요 없음 | Agreed |
| NFR-6 | 안정 API만 사용 | `@InternalComposeUiApi`, `@ExperimentalComposeUiApi` 의존을 금지하거나, 쓰더라도 어댑터 한 파일에 격리하고 버전 핀을 둠 | Agreed |
| NFR-7 | 크래시 격리 | 프로토콜 오류로 프로세스가 종료되지 않고 `ProtocolError` 이벤트를 보냄 | Agreed |
| NFR-8 | 데스크톱 접근성 | VoiceOver/Narrator 기본 동작 (§7 실험 결과로 확정) | Draft |
| NFR-9 | 네이티브 수준 프레임 성능 | §5.1 기준 충족 | Agreed |

### 5.1 프레임 예산 (NFR-9)

목표는 **같은 화면을 Kotlin/Compose로 직접 작성한 것과 체감 차이가 없는 수준**입니다. 기준은 120Hz 디스플레이(프레임당 8.33ms)입니다.

| 항목 | 기준 (p99, 릴리스 빌드) |
|---|---|
| 기준선 대비 오버헤드 | 같은 화면을 순수 Compose로 만든 기준선 대비 프레임 시간 증가 ≤ 10% |
| Host 처리 (핸들러 + diff + 배치 인코딩) | 일반 상호작용 ≤ 0.5ms, 스트리밍 프레임 ≤ 1ms |
| 경계 호출 1회 비용 | Desktop/iOS ≤ 100ns, Android ≤ 200ns(`@FastNative` 적용 시) |
| 배치 적용 (Renderer 디코드 + 스냅샷 적용) | Mutation 100건당 ≤ 0.3ms |
| 입력 → 화면 반영 | 기준선과 같은 프레임 수. 추가 프레임 지연 0 |
| 정상 상태 할당 | 경계 인코딩(arena 재사용) 0회. Host 전체 경로는 프레임당 200회 이하이고, 같은 상호작용을 반복해도 증가하지 않을 것. Renderer: 변경된 문자열의 `String` 생성 외 할당 0회 |
| 프레임 드랍 | 초당 100토큰 스트리밍 + 스크롤 중 드랍 0 (1만 개 메시지 대화) |

- 모든 수치는 실측으로 확인하고, 측정 환경(기기, OS, 빌드 설정)과 함께 기록합니다.
- 할당은 횟수 자체보다 **증가하지 않는지**가 기준입니다. Dioxus는 diff와 이벤트 처리 과정에서 내부적으로 할당하며(2026-09-20 측정: 클릭당 99회), 이를 0으로 만들려면 Dioxus를 포크해야 해서 D2와 충돌합니다. Rust에는 GC가 없어 이 할당이 프레임 멈춤으로 이어지지 않습니다. 반복 상호작용에서 할당 수가 늘어나면 누수나 캐시 미작동으로 보고 조사합니다.
- 벤치마크 하네스는 M0에서 함께 만들고, CI에서 회귀를 감시합니다. 기준 초과는 빌드 실패로 처리합니다.
- 개발 빌드에서는 Host 처리가 1ms를 넘는 프레임을 경고로 남깁니다.
- native-image의 GC pause도 프레임 드랍 요인으로 측정합니다. 기준을 넘으면 GC 설정(Serial/Epsilon, 힙 크기) 조정을 SPEC에 기록합니다.

## 6. IME 수용 체크리스트 (FR-5, M1)

native-image 빌드에서 macOS와 Windows 각각 수동으로 확인합니다.

- [ ] "안녕하세요" 입력 시 조합 과정이 정상 표시됨
- [ ] 조합 중 백스페이스로 자모 단위 삭제
- [ ] 조합 중 화살표로 커서 이동 시 조합 확정 후 이동
- [ ] 문장 중간에 커서를 두고 한글 삽입
- [ ] 멀티라인 필드에서 조합 중 Enter 처리 (조합 확정과 줄바꿈/제출 구분)
- [ ] 한글이 섞인 긴 텍스트 붙여넣기
- [ ] 일본어/중국어 IME 후보창이 커서 위치에 뜸
- [ ] Host가 `TextChanged`를 받는 동안 조합이 리셋되지 않음
- [ ] 한글 폰트 폴백 (두부 문자 없음)

실패하면 INTENT D4에 따라 native-image 설정(ServiceLoader, JNI/리플렉션 config, 로케일/문자셋)부터 점검합니다.

## 7. 접근성 실험 (Q1, M1)

native-image 빌드에서 AWT 접근성 브리지가 유지되는지 확인합니다. 결과에 따라 NFR-8을 확정합니다.

- [ ] macOS VoiceOver가 Text와 Button 라벨을 읽음
- [ ] Windows Narrator가 같은 화면을 읽음
- [ ] Tab 키 포커스 순회
- [ ] 같은 화면을 JVM 실행과 비교해 차이 기록

실패하면 원인을 native-image 설정 누락(JNI/리플렉션 config, `javax.accessibility` 서비스)과 Substrate 미지원으로 구분해 기록합니다.

## 8. 열린 질문 추적

[PROJECT.md](../PROJECT.md#열린-질문)의 열린 질문 표를 참고하세요. 결정이 나면 해당 SPEC 항목을 `Agreed`로 올리고 INTENT에 결정을 기록합니다.
