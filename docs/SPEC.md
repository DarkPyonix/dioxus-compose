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

### FR-1 노드 트리 구성 (`Agreed`)
Host는 Mutation 시퀀스로 Renderer의 노드 트리를 생성, 수정, 삭제, 이동할 수 있어야 합니다.
- 수용 기준: `Create`, `SetProp`, `SetModifier`, `Insert`, `Move`, `Remove`로 임의의 트리를 만들 수 있고, 적용 결과가 Renderer의 트리 덤프와 일치합니다.

### FR-2 스키마 기반 렌더링 (`Agreed`)
Renderer는 스키마에 정의된 위젯 타입만 해석해서 해당 Compose 컴포저블로 렌더링합니다.
- 최소 스키마(M0): `Column`, `Row`, `Box`, `Text`, `TextField`, `Button`, `Spacer`, `LazyColumn`(FR-8)
- 디자인 확장: `ScrollColumn`. 값 모델은 FR-13, 테마는 FR-14를 따릅니다
- 수용 기준: 스키마에 없는 타입이나 속성을 받으면 크래시하지 않고 `ProtocolError` 이벤트를 보냅니다.

### FR-3 이벤트 전달 (`Agreed`)
사용자 입력은 `(node_id, handler_id, payload)` 형태로 Host 핸들러를 **동기로 직접 호출**합니다(PR-1). 핸들러는 반환값을 돌려줄 수 있습니다. 클로저는 경계를 넘지 않습니다.
- 수용 기준: Button 클릭이 등록된 Rust 핸들러를 정확히 한 번 호출합니다.

### FR-4 상태 갱신 반영 (`Agreed`)
Host 상태가 변경되면 변경분만 전송하고, Renderer는 해당 노드만 recomposition합니다.
- 수용 기준: Text 하나의 내용을 바꿀 때 전송되는 Mutation은 `SetProp` 1건입니다. 형제 노드는 recomposition되지 않습니다(recomposition 카운터로 확인).

### FR-5 비제어 TextField (`Agreed`)
- TextField의 편집 값과 조합 상태는 Renderer가 소유합니다.
- Renderer는 변경을 알림 이벤트(`TextChanged`, 디바운스 적용)와 확정 이벤트(`TextSubmitted`, `FocusLost`)로 보냅니다.
- Host가 값을 바꿀 때는 명시적 명령 `SetText(node_id, text, selection)`을 씁니다. Renderer는 IME 조합이 진행 중이면 조합이 끝날 때까지 적용을 미룹니다.
- 수용 기준: §6 IME 체크리스트를 통과합니다.

### FR-6 Dioxus 렌더러 (`Agreed`)
`dioxus-core` VirtualDom의 `Mutations`를 프로토콜 Mutation으로 변환하는 렌더러를 제공합니다.
- 사용자 코드는 `rsx!`와 훅만으로 작성하고, 프로토콜을 직접 다루지 않습니다.
- 수용 기준: M0 화면을 `rsx!` 컴포넌트로 재작성했을 때 동일하게 동작합니다.

### FR-7 스키마 코드젠 (`Draft`)
위젯, 속성, Modifier, 이벤트 페이로드는 Rust에서 단일 소스로 정의하고 Kotlin 타입과 코덱을 생성합니다.
- 양쪽 모두 exhaustive match가 적용됩니다(Rust `enum`은 Kotlin `sealed interface`로 생성).
- 핸드셰이크 때 스키마 해시를 비교해서 불일치하면 초기화를 실패시킵니다.
- 수용 기준: Rust 스키마에 속성을 추가하고 Kotlin 인터프리터를 갱신하지 않으면 **빌드가 실패**합니다.

### FR-8 LazyColumn 윈도잉 (`Agreed`)
- Host는 아이템 총 개수와 안정적인 key를 알립니다.
- Renderer는 보이는 범위를 `RangeRequested`로 요청하고, Host는 **요청받은 구간만 정확히** 생성합니다. Host가 구간을 넓히지 않습니다.
- **선읽기 버퍼는 Renderer가 소유합니다.** Renderer가 가시 범위에 버퍼를 더한 값을 `start`, `count`로 보냅니다. 스크롤 위치가 Renderer에 있으므로(D5) 얼마나 미리 읽을지 아는 쪽도 Renderer입니다. Host가 따로 버퍼를 더하면 Renderer는 받은 서브트리가 전체 목록의 몇 번째 자리에 놓이는지 알 수 없습니다. `start`가 곧 첫 아이템의 전역 인덱스라는 것이 이 규칙의 핵심입니다.
- 아이템 식별: Host가 아이템마다 `Box` 래퍼 노드를 만들고 `item_key`(문자열)를 실어 보냅니다. Renderer는 그 값을 Compose `LazyColumn`의 key로 씁니다.
- Renderer는 `item_count`개짜리 실제 Compose `LazyColumn`을 그립니다. 전역 인덱스 `i`는 `i - start`번째 자식으로 그리고, 구간 밖은 빈 자리로 둡니다. 그래서 스크롤 막대와 스크롤 거리가 전체 목록 기준으로 맞습니다.
- 와이어: `RangeRequested`는 이벤트 태그 7(24바이트, `start: u32`, `count: u32`)입니다. Host는 `item_count`, `item_key`, `on_range_requested` 속성으로 선언합니다.
- 수용 기준: 아이템 10,000개 목록에서 생성된 노드 수가 가시 범위와 버퍼에 비례합니다. **(Host 측 통과: 가시 20 + 버퍼 4 요청에 아이템 28개)**

### FR-9 스트리밍 텍스트 (`Agreed`)
긴 텍스트가 점진적으로 늘어나는 경우를 위해 Text 노드에 `AppendText` 명령을 둡니다(태그 8, 16바이트). 전체 문자열이 아니라 늘어난 꼬리만 보냅니다. Host는 추가분을 모아 프레임당 노드별 1건으로 flush하며, flush 지점은 `render_frame`입니다.
- 수용 기준: 초당 100회 추가되는 스트리밍 중에도 스크롤과 입력이 끊기지 않습니다. **(Host 측 통과: 36KB 텍스트에서 배치 64바이트 미만, 스트리밍 프레임 p99 125ns)**

### FR-12 이벤트 소비(consume) (`Agreed`)
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

### FR-10 Modifier 값 모델 (`Agreed`)
Modifier는 값 리스트로 직렬화합니다. 예: `[Padding(16), FillMaxWidth, Background(argb), Clickable(handler_id)]`. Renderer는 이를 `Modifier` 체인으로 재구성합니다.

### FR-13 디자인 프리미티브 (`Agreed`)
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
- `Modifier::Shape { top_start, top_end, bottom_end, bottom_start }`, f32 4개를 `u64` 2개에 담습니다.
- `Modifier::ShapeRole(ShapeRole)`, `None|ExtraSmall|Small|Medium|Large|Full`. 실제 반지름은 디자인 시스템이 정합니다. Material 3의 12dp, HIG의 연속 곡률 느낌, Fluent의 4dp가 여기서 갈립니다.
- `Modifier::Border { width, paint }`, `first` 하위 32비트에 너비, `second`에 `Paint`.
- 테두리와 모양은 서로 독립입니다. Renderer는 `ShapeRole`/`Shape` 중 마지막에 적용된 것을 클립과 테두리 모두에 씁니다.

#### 13.4 간격과 배치
- `SpaceRole`: `None|Xs|Sm|Md|Lg|Xl|Xxl`. 밀도가 디자인 시스템마다 다른 부분이라 리터럴 dp보다 역할이 먼저입니다.
- `Modifier::PaddingRole(SpaceRole)`, 기존 `Modifier::Padding(f32)`는 유지합니다.
- `Modifier::PaddingEach { start, top, end, bottom }`, f32 4개.
- `Modifier::Weight(f32)`, `RowScope`/`ColumnScope`의 weight입니다.
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

#### 13.8 와이어 태그 배정
프리미티브를 구현하려면 고정 태그가 필요합니다. 태그는 한 번 배정하면 재사용하지 않고 뒤에만 덧붙입니다(PR-4).

역할 enum은 모두 1부터 시작합니다. 0은 "보내지 않음"을 뜻하므로 역할 값으로 쓰지 않습니다.

| enum | 값 |
|---|---|
| `ColorRole` | `Primary=1, OnPrimary=2, Secondary=3, OnSecondary=4, Surface=5, OnSurface=6, SurfaceVariant=7, OnSurfaceVariant=8, Background=9, OnBackground=10, Outline=11, OutlineVariant=12, Error=13, OnError=14` |
| `TypeRole` | `Display=1, Headline=2, Title=3, Subtitle=4, Body=5, BodyStrong=6, Label=7, Caption=8, Mono=9` |
| `ShapeRole` | `None=1, ExtraSmall=2, Small=3, Medium=4, Large=5, Full=6` |
| `SpaceRole` | `None=1, Xs=2, Sm=3, Md=4, Lg=5, Xl=6, Xxl=7` |
| `TextAlign` | `Start=1, Center=2, End=3, Justify=4` |
| `TextOverflow` | `Clip=1, Ellipsis=2, Visible=3` |
| `Arrangement` | `Start=1, Center=2, End=3, SpaceBetween=4, SpaceAround=5, SpaceEvenly=6` |
| `Alignment` | `TopStart=1, TopCenter=2, TopEnd=3, CenterStart=4, Center=5, CenterEnd=6, BottomStart=7, BottomCenter=8, BottomEnd=9` |
| `ButtonVariant` | `Filled=1, Tonal=2, Outlined=3, Text=4` |

`Paint`는 `u64` 하나입니다. 상위 32비트가 종류(`Role=1`, `Literal=2`)이고 하위 32비트가 값(`ColorRole` 태그 또는 ARGB)입니다.

위젯 태그: `ScrollColumn = 9`.

Modifier 태그(기존 `Empty=0`~`Clickable=8` 뒤에 덧붙입니다):

| 태그 | Modifier | `first` | `second` |
|---|---|---|---|
| 9 | `PaddingRole` | `SpaceRole`(u32) | 없음 |
| 10 | `PaddingEach` | `start`\|`top` (f32 2개) | `end`\|`bottom` (f32 2개) |
| 11 | `Weight` | `value`(f32) | 없음 |
| 12 | `Shape` | `top_start`\|`top_end` | `bottom_end`\|`bottom_start` |
| 13 | `ShapeRole` | `ShapeRole`(u32) | 없음 |
| 14 | `Border` | `width`(f32) | `Paint`(u64) |
| 15 | `Elevation` | `dp`(f32) | 없음 |

`f32` 2개를 `u64` 하나에 담을 때는 하위 32비트가 첫 번째 값입니다.

**기존 `Modifier::Background`(태그 7)의 필드를 `argb: u32`에서 `paint: u64`로 바꿉니다.** 13.1의 "색을 받는 자리는 전부 `Paint`"를 지키려면 예외를 둘 수 없습니다. 레코드 길이는 그대로이고 스키마 해시만 바뀝니다. M0가 아직 배포되지 않았으므로 태그를 새로 따지 않고 자리를 바꿉니다.

Property 태그(기존 `OnRangeRequested=12` 뒤에 덧붙입니다): `TypeRole=13, FontSize=14, FontWeight=15, LineHeight=16, LetterSpacing=17, Color=18, TextAlign=19, MaxLines=20, Overflow=21, Arrangement=22, Spacing=23, SpaceRole=24, Alignment=25, Variant=26`.

### FR-14 디자인 시스템과 테마 모드 (`Agreed`)
디자인 시스템은 **토큰 집합 + 컴포넌트 스타일 규칙**의 한 쌍입니다. 속성을 모아 놓은 것이 아닙니다.

지원 대상은 두 단계로 나눕니다.

| 단계 | 디자인 시스템 |
|---|---|
| 1단계 | Material 3, Apple HIG, WinUI/Fluent 2 |
| 2단계 | GNOME 50, KDE Breeze, Deepin |

2단계는 1단계가 동작한 뒤에 추가합니다. 14.1의 추상화가 성립하면 각각 `DesignSystem` 변형 1개와 Renderer 측 테이블 1개, 규칙 구현 1개로 끝나야 하며, 이것이 그 추상화의 실제 검증입니다.

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
| Windows | WinUI/Fluent 2 |
| Linux (GNOME) | GNOME 50 |
| Linux (KDE) | KDE Breeze |
| Linux (그 외, 판별 불가) | Deepin |
| Web | WinUI/Fluent 2 (설정으로 Material 3로 교체 가능) |

- **Linux 데스크톱 환경 판별**: `XDG_CURRENT_DESKTOP`을 먼저 보고, 비어 있으면 `DESKTOP_SESSION`을 봅니다. 값에 `GNOME`이 포함되면 GNOME 50, `KDE`면 Breeze, 그 외와 판별 실패는 Deepin입니다. 판별 결과는 시작 시 한 번만 읽습니다.
- **Web에 플랫폼 룩은 없습니다.** 브라우저는 자기 디자인 언어를 갖지 않으므로 `adaptive`에서도 선택은 임의입니다. 기본을 Fluent 2로 두되, 앱이 설정으로 Material 3를 고를 수 있습니다. 문서에서는 Web에 대해 `unified`를 명시하는 것을 권장합니다.
- 2단계 시스템이 구현되기 전까지 Linux는 `fallback` 인자를 씁니다. 구현 완료 시 위 표가 적용됩니다.
- **GNOME 50 주의**: 버전을 명시한 것은 GNOME의 디자인 언어가 릴리스마다 바뀌기 때문입니다. 구현 전에 해당 릴리스의 HIG를 직접 확인하고, 참조한 문서와 버전을 토큰 테이블 주석에 남깁니다.
- **Deepin 주의**: 토큰값과 스타일 규칙만 참조합니다. 아이콘 세트와 전용 폰트는 별도 라이선스가 걸리므로 가져다 쓰지 않습니다.
- 명암(`ColorScheme`)은 `Light | Dark | FollowSystem`이고 기본은 `FollowSystem`입니다. 시스템 설정 변화는 Renderer가 먼저 알고 스스로 반영합니다. Host는 관여하지 않습니다(D5).

#### 14.4 해석 위치: Renderer
**토큰 해석과 컴포넌트 규칙은 Renderer가 수행합니다.** Host는 역할과 선택만 보냅니다.

근거:
- **PR-3/NFR-9(프레임 예산).** Host는 Renderer UI 스레드에서 돌기 때문에 Host의 작업이 그대로 프레임 예산(§5.1, 상호작용당 ≤ 0.5ms)에서 빠집니다. Host가 토큰을 푼다면 다크모드 전환이나 플랫폼 테마 변경이 트리 전체에 대한 `SetProp` 재전송(O(노드 수))이 됩니다. Renderer가 풀면 같은 변경이 `SetTheme` 1건이고, 나머지는 Compose의 `CompositionLocal` 무효화로 끝납니다.
- **PR-1(동기 경계).** 시스템 명암 전환과 플랫폼 식별은 Renderer 쪽 정보입니다. Host가 해석하려면 Renderer→Host 질의가 필요한데, 경계는 동기 단방향 호출 모델이라 질의를 추가하면 PR-2의 표면이 늘어납니다.
- **D5.** 테마는 UI 로컬 상태입니다. 스크롤 위치·포커스와 같은 부류입니다.
- 비용: Host 쪽 단위 테스트는 "어떤 역할을 보냈는가"까지만 검증할 수 있고, 실제 색·치수는 Renderer 테스트에서 검증합니다. 이 분리를 받아들입니다.

**토큰 테이블의 저작 위치는 Rust이고, 실행 위치는 Renderer입니다.** 14개 `ColorRole` × 2개 명암, 9단 `TypeRole`, `ShapeRole`/`SpaceRole` 치수 같은 값 표는 Rust 스키마에 데이터로 두고, FR-7 코드젠이 `Protocol.gen.kt`에 Kotlin `object`로 내보냅니다. 근거:
- D6(단일 소스는 Rust)를 토큰에도 그대로 적용합니다. Kotlin에 손으로 적으면 세 시스템 × 7개 표가 검증되지 않은 채 남습니다.
- 값 표는 Rust 테스트로 검증할 수 있습니다(대비비, 사다리 단조성, 표가 비어 있지 않은지). 14.4가 포기한 것은 "화면에 그려진 결과"이지 "표의 내용"이 아닙니다.
- 경계는 그대로입니다. 표는 **빌드 시점에** Renderer 바이너리로 들어가고, 런타임에 경계를 넘지 않습니다. 13.7의 "Host가 보내는 토큰 테이블"은 여전히 금지입니다.
- Renderer 구현자가 채우는 것은 값이 아니라 **적용 규칙**(14.6의 5·6·7번과 `CompositionLocal` 배선)입니다.

#### 14.5 와이어 추가분
- Mutation `SetTheme { design_system: u16, fallback: u16, color_scheme: u16, adaptive: u16 }`: **명령 태그 9**, 레코드 길이 12바이트(`tag`, `len`, 뒤이어 u16 4개). 루트(`node_id` 없음)에 적용합니다. `adaptive`는 0 또는 1입니다. Host는 초기 배치의 첫 레코드로 1회 보내고, 앱이 테마를 바꿀 때만 다시 보냅니다.
- `DesignSystem` 태그: `Material3 = 1`, `AppleHig = 2`, `Fluent = 3`.
- `ColorScheme` 태그: `Light = 1`, `Dark = 2`, `FollowSystem = 3`.
- 수용 기준: `Theme::unified(...)`로 띄운 앱의 첫 배치 첫 레코드가 `SetTheme`이고 `adaptive = false`입니다. `Theme::adaptive(...)`이면 `adaptive = true`이며 `fallback`이 인자로 준 시스템입니다.

#### 14.6 Renderer 구현자가 채워야 할 표
디자인 시스템마다 아래 7개가 필요합니다. 채워지면 위젯 코드는 건드리지 않습니다.

1~4번은 14.4에 따라 Rust 스키마에서 코드젠으로 생성되어 `Protocol.gen.kt`의 `DesignTokens`에 이미 들어 있습니다. Renderer 구현자는 **5~7번과, 1~4번을 Compose에 배선하는 일**을 맡습니다.
1. `ColorRole` 14개 × {Light, Dark} 색값
2. `TypeRole` 9개 → 크기/굵기/행간/자간/폰트
3. `ShapeRole` 6개 → 곡률(HIG는 연속 곡률)
4. `SpaceRole` 7개 → dp
5. `Modifier::Elevation(dp)` → 그림자/톤/스트로크 렌더링 규칙
6. `ButtonVariant` 4개 → 배경·전경·테두리·눌림 표현
7. 모션: 상태 전환 duration과 easing

### FR-11 스키마 확장 (서드파티 위젯) (`Agreed`)
스키마에 없는 Compose 컴포넌트는 **E1 확장 스키마 패키지**로 추가합니다. 확장은 런타임 플러그인이 아니라 Host와 Renderer의 소스 빌드에 함께 들어가는 한 쌍입니다. Rust 쪽 선언이 위젯 태그, 속성 태그, 타입이 붙은 Dioxus 컴포넌트를 소유하고, Kotlin 쪽 구현이 그 태그와 속성을 실제 `@Composable` 호출로 해석합니다.

| 안 | 방식 | 장점 | 단점 |
|---|---|---|---|
| E1 확장 스키마 패키지 | 위젯 정의(Rust)와 인터프리터 구현(Kotlin)을 한 쌍으로 묶은 확장 단위. 빌드 시 코드젠으로 스키마에 병합 | 타입 안전(FR-7 그대로), 오버헤드 없음 | Host와 Renderer 재빌드 필요 |
| E2 Kotlin 어노테이션 기반 역생성 | `@DioxusWidget` 붙인 `@Composable` 함수에서 KSP로 Rust 컴포넌트와 스키마 생성 | Compose 라이브러리 래핑 비용이 최소 | 단일 소스가 Kotlin으로 뒤집힘(D6와 충돌 검토 필요), 파라미터 타입 매핑 한계 |
| E3 동적 Slot 위젯 | `Custom { kind: "id", props: bytes }` 범용 노드, Renderer 측 레지스트리 | 스키마 변경 없이 추가 | 타입 안전이 런타임 검사로 격하 (C3 위반 소지) |

현재 `dioxus-compose/src/codegen.rs`는 Rust의 정적 스키마 표를 순회해 `Protocol.gen.kt`를 만드는 단방향 생성기입니다. 이 구조에서는 E1이 D6와 FR-7을 그대로 보존합니다. 새 태그가 표와 스키마 해시에 들어가고, 기존 `Create`와 `SetProp`의 고정 레이아웃을 그대로 사용하며, 표에 없는 태그는 계속 `ProtocolError`입니다.

E2를 기본 경로로 삼으면 KSP가 Rust 열거형, Dioxus element, 컴포넌트 API까지 역생성해야 합니다. Kotlin 타입 중 Rust와 와이어 타입으로 손실 없이 옮길 수 없는 타입도 별도 규칙이 필요합니다. 이는 현재 생성 방향을 뒤집고 Rust 단일 소스를 깨므로 채택하지 않습니다. 나중에 많은 Compose API를 감쌀 때에도 E2는 Kotlin을 런타임에 등록하는 장치가 아니라, 검토해서 커밋할 E1 Rust 선언 초안을 만드는 오프라인 도구로만 허용합니다. E3는 임의 바이트와 문자열 식별자가 고정 스키마를 우회하므로 채택하지 않습니다.

#### 11.1 저작 단위와 빌드 비용
확장 작성자는 다음 두 소스 파일을 책임집니다.

1. Host의 Rust 확장 파일 하나: 추가 전용 범위의 위젯 태그와 속성 태그, Dioxus element, 타입이 붙은 `#[component]`를 선언합니다. 속성은 기존 `PropertyValue`의 `String`, `bool`, `i64`, `f32` 중 하나로 내립니다.
2. Renderer의 Kotlin 확장 파일 하나: 생성된 `WidgetKind`와 `PropertyKind`를 처리하고 실제 Compose 라이브러리의 `@Composable`을 호출합니다. 외부 라이브러리라면 Renderer 빌드 의존성도 추가합니다.

`cargo run -p dioxus-compose --bin codegen`은 별도 저작 파일을 요구하지 않고 이 Rust 선언을 정규 스키마에 병합해 `Protocol.gen.kt`를 다시 만듭니다. 생성 파일은 커밋합니다. 태그를 추가하거나 속성 계약을 바꿀 때마다 Host 라이브러리와 모든 플랫폼 Renderer를 함께 다시 빌드하고 배포해야 합니다. 앱의 UI 코드만 바뀌고 확장 스키마가 그대로면 Renderer를 다시 빌드할 필요가 없습니다. 이미 배포된 Renderer에는 새 컴포넌트를 로드하거나 등록할 방법이 없습니다.

#### 11.2 태그와 호환성
- 기본 스키마 태그는 그대로 유지합니다. 확장 위젯과 속성은 추가 전용 태그를 쓰며 기존 태그를 재사용하거나 재정렬하지 않습니다.
- 확장 메타데이터는 `WIDGET_SCHEMA`, `PROPERTY_SCHEMA`, `SCHEMA_HASH`에 포함됩니다. Host와 Renderer 중 한쪽만 갱신하면 핸드셰이크가 화면을 만들기 전에 실패합니다.
- 확장도 `Create` 12바이트와 `SetProp` 24바이트 레코드만 사용합니다. 별도 payload, 동적 map, 직렬화 포맷을 만들지 않습니다.
- Renderer는 생성된 열거형의 모든 확장 위젯을 명시적으로 해석해야 합니다. 생성 코드는 Compose 구현을 추론하지 않습니다.

#### 11.3 수용 기준
- M0에 없는 실제 Compose 컴포넌트 하나를 Rust의 타입이 붙은 컴포넌트로 선언하고, RSX에서 사용하면 추가 위젯 태그와 속성 태그를 가진 고정 레이아웃 Mutation이 나옵니다.
- 코드젠 결과에 같은 `WidgetKind`, `PropertyKind`, 디코더 분기가 생기며 생성 파일 일치 테스트가 통과합니다.
- 확장을 포함한 스키마 해시는 확장이 없던 스키마 해시와 다르고, 잘못된 핸드셰이크는 초기화에 실패합니다.
- 선언되지 않은 위젯 태그와 속성 태그는 Rust와 Kotlin 디코더에서 `ProtocolError`로 남습니다.
- Renderer 구현에는 생성된 종류별 분기와 Compose 호출이 필요하다는 사실을 문서와 빌드가 숨기지 않습니다.

## 4. 경계 프로토콜

### PR-1 호출 모델: 동기·동일 스레드 직접 호출 (`Agreed`)
옛 React Native 브리지처럼 비동기 큐를 두면 병목이 생깁니다. 비동기 큐는 동기 반환값을 받을 수 없고, 스레드 홉 때문에 최대 1프레임 지연이 생깁니다. 그래서 JSI처럼 **같은 스레드에서 서로를 직접 호출**합니다.

- VirtualDom은 **Renderer의 UI 스레드에서** 돕니다. 이 스레드는 Host의 전용 스레드가 아닙니다.
- 사용자 입력이 들어오면 Renderer가 Host 핸들러를 직접 호출합니다. Host는 그 자리에서 핸들러를 실행하고 diff를 계산한 뒤, 결과 Mutation 배치와 반환값을 돌려줍니다.
- 동기 반환값을 지원합니다. 예: `onKeyEvent`의 "처리됨" 여부. Enter는 제출, Shift+Enter는 줄바꿈으로 나누는 처리가 여기에 해당합니다. 표현 방식은 FR-12를 따릅니다.
- 경계에 비동기 큐를 두지 않습니다. 스레드 간 통신은 PR-3의 wake 신호 하나뿐입니다.

### PR-2 경계 표면 (`Draft`)
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

### PR-3 스레드 규칙 (`Agreed`)
- VirtualDom, 사용자 컴포넌트, 모든 `dioxus_compose_host_*` 호출은 Renderer UI 스레드에서만 실행합니다. 그래서 락이 필요 없습니다.
- **UI 스레드에서 도메인 작업을 금지합니다.** 네트워크, 파일 I/O, 프로세스 관리 같은 작업은 Host 워커 스레드(tokio 등)에서 돌립니다. 워커는 Dioxus signal로 상태를 갱신하고, Host가 내부에서 `request_frame`을 호출합니다. 사용자 코드는 경계 함수를 직접 부르지 않습니다.
- `request_frame`은 여러 번 불러도 다음 프레임에 `render_frame` 1회로 합쳐집니다. Compose frame clock(`withFrameNanos`) 안에서 실행됩니다.
- macOS에서 `dioxus_compose_renderer_run`은 프로세스 메인 스레드에서 호출해야 합니다(AppKit 요구사항).
- Android: Host 워커 스레드는 `request_frame`을 부르기 위해 JavaVM에 **1회 영구 attach**합니다. 호출마다 attach하는 것은 금지합니다. `@FastNative`/`@CriticalNative`는 짧은 호출에만 허용합니다.
- 프레임 예산은 NFR-9를 따릅니다.

### PR-4 배치 버퍼와 인코딩 (`Draft`)
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

### PR-5 Android (`Draft`)
- 호스트 관계: Kotlin Activity가 프로세스와 루프를 소유합니다(`LoopMode::Platform`). Rust는 cdylib입니다. VirtualDom은 PR-3에 따라 UI 스레드에서 돕니다.
- JNI 심: PR-2의 논리 연산에서 jni-rs 기반 심과 Kotlin `external fun` 선언을 코드젠으로 생성합니다. UniFFI(JNA 경유)는 호출당 오버헤드가 커서 쓰지 않습니다.
- 생명주기:
  - Surface 파괴와 config change: Compose만 재구성됩니다. VirtualDom은 프로세스 전역에 유지됩니다. Renderer는 재구성 후 `Resync`를 보내고, Host는 전체 트리 배치를 돌려줍니다.
  - `onStop`/`onStart`: `Lifecycle` 이벤트를 보냅니다. Host는 타이머와 애니메이션을 억제합니다.
  - 프로세스 kill: 메모리 상태는 복원하지 않습니다. 필요하면 `SaveState`로 작은 blob을 `onSaveInstanceState`에 저장합니다.
- android-activity, NativeActivity, GameActivity 진입점은 쓰지 않습니다. ComposeView와 공존한 사례가 없고 IME 충돌 위험이 있습니다. JavaVM은 `JNI_OnLoad`에서 얻습니다.
- 수용 기준(M6):
  1. 일반 JNI와 `@FastNative`의 호출당 비용을 실측합니다. 공개 수치(약 115ns, 약 35ns)와 비교해 기록합니다.
  2. M0 화면을 같은 Rust 소스로 띄우고, 초당 100회 추가되는 스트리밍 중 프레임 끊김이 없음을 Macrobenchmark `FrameTimingMetric`으로 확인합니다.
  3. 화면 회전, 다크모드 전환, 홈→복귀, `am kill` 후 복귀에서 크래시가 없습니다.

### PR-6 Web 경계 (`Agreed`)
Rust(wasm32)와 Kotlin/Wasm 모듈을 연결합니다. `LoopMode::Platform`입니다. 2026-09-20 실측으로 확정했습니다(`experiments/web-interop/`).

- **메모리: Kotlin이 소유합니다.** Kotlin/Wasm 모듈은 항상 자기 메모리를 정의해 export하며, 외부 메모리를 import하는 경로가 없습니다. 따라서 Rust가 `--import-memory`로 그 메모리를 가져다 씁니다. PR-4의 arena는 양쪽이 제자리에서 읽습니다. 복사는 없습니다.
- **함수 호출: 경계 함수마다 고정된 형태의 JS forwarder를 코드젠으로 생성합니다.** 호출당 약 12ns입니다.
- **두 가지를 동시에 가질 수 없습니다.** `WebAssembly.instantiate`는 import를 먼저 요구합니다. Kotlin은 Rust export 없이 인스턴스화할 수 없고, Rust는 Kotlin 메모리 없이 인스턴스화할 수 없으며, Kotlin 메모리는 인스턴스화 전에 존재하지 않습니다. wasm import를 wasm export에 직접 묶으면(1.45ns) 메모리를 공유할 수 없고, 메모리를 공유하면 한쪽 호출 경로에 JS forwarder가 들어옵니다. 단일 모듈 링크(WasmGC와 linear memory 혼합 불가), 제3의 메모리 소유 모듈, component model(브라우저 미지원) 모두 이 순환을 풀지 못합니다.
- **메모리 공유를 택합니다.** 프레임 예산을 지배하는 것은 PR-4의 복사 회피이지 호출 오버헤드가 아닙니다. 프레임당 경계 호출 3회 기준 약 36ns이며, PR-5가 Android에서 이미 수용한 JNI 호출 비용(약 115ns)보다 한 자릿수 작습니다. D8이 거부한 React Native 브리지와는 성격이 다릅니다. 직렬화도, 비동기 큐도, 스레드 홉도, 데이터 복사도 없습니다.
- 구현 시 주의: Kotlin 메모리는 0페이지로 시작하므로 Rust 인스턴스화 전에 JS가 `memory.grow()`를 해야 합니다. Rust의 데이터 세그먼트와 Kotlin `kotlin.wasm.unsafe` 할당자가 같은 주소 공간을 쓰므로 `--global-base`로 영역을 분리합니다.
- 실측(Safari 26.5, Apple silicon): 같은 모듈 호출 0.30ns, wasm 직접 바인딩 1.45ns, JS forwarder 12.05ns, Kotlin에서 메모리 읽기 0.977ns/byte.
- 남은 확인: V8과 SpiderMonkey에서 같은 수치가 나오는지 재측정해야 합니다.

### PR-7 명명 규칙 (`Agreed`)
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

### PR-8 macOS 런타임 요건 (`Draft`)
- 빌드 도구는 Liberica NIK 25 Full입니다(INTENT D9-macOS).
- 배포 레이아웃은 `<root>/lib/` 하나이며 `java.home`은 그 부모입니다. 렌더러는 자기 라이브러리 경로를 dladdr로 얻어 `java.home`, `skiko.library.path`, `skiko.data.path`를 설정합니다.
- `lib/`에 함께 두는 파일: 렌더러 라이브러리, Skia(`libskiko-macos-<arch>.dylib`), `libjawt.dylib` 포워더, `libawt_lwawt.dylib` 자리 채우기.
- `dioxus_compose_renderer_run`은 프로세스 메인 스레드에서 호출해야 합니다. 아니면 `RUN_NOT_MAIN_THREAD`를 반환합니다.
- 빌드 스크립트는 `lib/static/darwin-*/libawt_lwawt.a`가 없는 설치를 이미지 빌드 시작 전에 거부합니다. 순정 GraalVM과 비 Full NIK을 걸러내기 위한 것입니다.
- 수용 기준
  1. C 호스트가 라이브러리를 링크해 `run`을 호출하면 창이 뜨고, 창을 닫으면 `run`이 0을 반환하며 프로세스가 정상 종료됩니다. **(2026-09-20 통과)**
  2. 잘못된 툴체인(`GRAALVM_HOME` 미설정/없는 경로/`native-image` 없음/정적 AWT 아카이브 없음)에서 빌드가 즉시 실패하고 조치 방법을 출력합니다. **(2026-09-20 통과)**

## 5. 비기능 요구사항

| ID | 요구사항 | 수용 기준 | 상태 |
|---|---|---|---|
| NFR-1 | JVM 불필요 | 배포물에 JRE가 없고, `java`가 없는 머신에서 실행됨 | Agreed |
| NFR-2 | 웹뷰 불필요 | WKWebView, WebView2, WebKitGTK에 링크하지 않음 | Agreed |
| NFR-3 | 데스크톱 무게 | 빈 창 physical footprint < 45MB, 배포 용량 < 100MB. 측정 기준과 현재값은 §5.2 | Draft |
| NFR-4 | 플랫폼 | macOS, Windows, Linux 데스크톱, iOS, Android, Web(wasm). Android와 Web의 경계는 PR-5, PR-6 참조 | Agreed |
| NFR-5 | 개발 경험 | Renderer는 JVM 개발 셸에서 hot reload와 `@Preview`로 작업 가능. native-image 빌드는 개발 루프에 필요 없음. 새 머신의 준비 상태를 `scripts/setup-check.sh` 한 번으로 확인 가능 | Agreed |
| NFR-6 | 안정 API만 사용 | `@InternalComposeUiApi`, `@ExperimentalComposeUiApi` 의존을 금지하거나, 쓰더라도 어댑터 한 파일에 격리하고 버전 핀을 둠 | Agreed |
| NFR-7 | 크래시 격리 | 프로토콜 오류로 프로세스가 종료되지 않고 `ProtocolError` 이벤트를 보냄 | Agreed |
| NFR-8 | 데스크톱 접근성 | VoiceOver/Narrator 기본 동작 (§7 실험 결과로 확정) | Draft |
| NFR-9 | 네이티브 수준 프레임 성능 | §5.1 기준 충족 | Agreed |
| NFR-10 | 렌더러 탐색 경로 | `DIOXUS_COMPOSE_RENDERER_DIR` → 워크스페이스 빌드 결과물 순서로 찾음 | Agreed |
| NFR-11 | 배포 | 크레이트는 crates.io, 렌더러는 플랫폼별 체크섬 릴리스 아티팩트. 아티팩트 규격과 소비자 측 해석, 버전 계약은 §5.3 (INTENT D10) | Draft |

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
| 프레임 드랍 | 초당 100회 추가되는 스트리밍 + 스크롤 중 드랍 0 (아이템 1만 개 목록) |

- 모든 수치는 실측으로 확인하고, 측정 환경(기기, OS, 빌드 설정)과 함께 기록합니다. 측정 결과는 `dioxus-compose/benches/baseline.json`에 있습니다.
- 할당은 횟수 자체보다 **증가하지 않는지**가 기준입니다. Dioxus는 diff와 이벤트 처리 과정에서 내부적으로 할당하며(2026-09-20 측정: 클릭당 99회), 이를 0으로 만들려면 Dioxus를 포크해야 해서 D2와 충돌합니다. Rust에는 GC가 없어 이 할당이 프레임 멈춤으로 이어지지 않습니다. 반복 상호작용에서 할당 수가 늘어나면 누수나 캐시 미작동으로 보고 조사합니다.
- 벤치마크 하네스는 M0에서 함께 만들고, CI에서 회귀를 감시합니다. 기준 초과는 빌드 실패로 처리합니다.
- 개발 빌드에서는 Host 처리가 1ms를 넘는 프레임을 경고로 남깁니다.
- native-image의 GC pause도 프레임 드랍 요인으로 측정합니다. 기준을 넘으면 GC 설정(Serial/Epsilon, 힙 크기) 조정을 SPEC에 기록합니다.

### 5.2 메모리 (NFR-3)

**측정 기준은 macOS의 physical footprint입니다.** RSS는 이미지에 매핑된 깨끗한 페이지까지 세기 때문에 실제 점유량을 과장합니다. 같은 프로세스가 RSS 127MB, footprint 62MB로 두 배 넘게 차이납니다. Activity Monitor의 "메모리" 열이 footprint입니다.

**현재값 (2026-09-20, macOS 26.5.1, Apple M1, 빈 창 + TextField 2개)**

| 항목 | 값 |
|---|---|
| physical footprint | 61.6MB (최대 67.0MB) |
| MALLOC_SMALL (SubstrateVM 힙 + Skia) | 16MB |
| 그래픽 (IOSurface, IOAccelerator, 소유 물리 페이지) | 약 25MB |
| `__DATA` dirty | 약 7.7MB |

**목표는 45MB 미만입니다.** 비교 기준으로 macOS 네이티브 앱(AppKit과 시스템 텍스트 스택을 공유하는)은 창 하나에 20~25MB, 창 두 개에 40MB 수준입니다. 우리는 Skia와 Compose 런타임, GC 힙을 프로세스 안에 갖고 있으므로 그 수치를 그대로 따라갈 수는 없지만, 현재의 62MB는 튜닝 여지가 큽니다.

**줄일 수 있는 항목**

1. SubstrateVM 힙: 기본 최대 힙이 RAM의 80%입니다. 상한을 고정하고(`-R:MaxHeapSize`) 초기 힙을 줄이면 MALLOC 영역이 직접 줄어듭니다.
2. GC 선택: Serial GC의 영역 크기와 수집 정책을 UI 작업량에 맞춰 조정합니다(§5.1의 프레임 멈춤 기준과 함께 판단).
3. 그래픽 서페이스: 창 크기에 비례합니다. Skia 래스터 캐시 상한과 Metal 서페이스 개수를 확인합니다.
4. 폰트와 ICU 데이터: 사용하지 않는 로케일 데이터를 이미지에서 제외합니다.
5. 이미지 자체: `-Os`는 적용 중입니다. 도달 가능 코드 축소가 dirty `__DATA`에도 영향을 줍니다.

수용 기준: 위 항목을 적용한 뒤 빈 창 footprint를 재측정하고, 45MB를 넘으면 무엇이 막는지 항목별 수치와 함께 기록합니다.

### 5.3 배포 경로 (NFR-10, NFR-11)

크레이트(`dioxus-compose`)는 평범한 crates.io 크레이트로 배포하고, 렌더러는 태그마다 플랫폼별 릴리스 아티팩트로 배포합니다(INTENT D10). 아래가 그 계약입니다.

**아티팩트 규격**

| 항목 | 값 |
|---|---|
| 태그 | `v<크레이트 버전>` (예: `v0.1.0`) |
| 타깃 이름 | `macos-aarch64` 형식 (`<os>-<arch>`) |
| 파일 이름 | `dioxus-compose-renderer-v<버전>-<타깃>.tar.gz` |
| 체크섬 | 같은 이름에 `.sha256`을 붙인 파일. `shasum -a 256` 출력 형식 그대로이며, 릴리스에는 모든 타깃을 모은 `SHA256SUMS`도 함께 올립니다 |
| 내용 | `dist/`의 내용물을 그대로 푼 것: `lib/`(렌더러, Skia, AWT 보조 라이브러리), `include/`, 그리고 버전 파일 |
| 버전 파일 | `dioxus-compose-renderer.version`. 아티팩트 루트에 있으며 한 줄에 크레이트 버전만 적습니다 |

아티팩트를 만드는 주체는 `scripts/package-renderer.sh` 하나뿐입니다. CI(`.github/workflows/release.yml`)도 같은 스크립트를 호출하므로, CI가 올리는 것과 사람이 로컬에서 만드는 것이 같음이 보장됩니다.

**소비자 측 해석 (`dioxus-compose/build.rs`)**

`native-renderer` 기능을 켠 소비자는 `DIOXUS_COMPOSE_RENDERER_DIR`로 렌더러 위치를 알려줍니다. 탐색 순서는 NFR-10 그대로입니다.

1. `DIOXUS_COMPOSE_RENDERER_DIR`: 아티팩트를 푼 루트를 가리켜도 되고, 그 안의 `lib` 디렉터리를 직접 가리켜도 됩니다. 루트로 판단되면 `lib`를 붙여 씁니다.
2. 워크스페이스 빌드 결과물 (`dioxus-compose-renderer/build/native-image/dist/lib`).

**결정: 링커 오류를 사용자의 첫 신호로 두지 않습니다.** 게시된 크레이트에는 폴백할 워크스페이스가 없으므로, 예전 동작은 존재하지 않는 경로를 링커에 넘겨 빌드 후반에 원시 링커 오류를 내게 됩니다. 대신 빌드 스크립트가 링크 지시를 내보내기 **전에** 라이브러리 파일의 존재를 확인하고, 없으면 빌드를 즉시 실패시킵니다. 실패 메시지는 (1) 무엇이 없는지, (2) 이 크레이트 버전에 맞는 아티팩트 파일 이름, (3) 내려받아 검증해 푸는 방법(`scripts/fetch-renderer.sh` 또는 동등한 수동 절차), (4) 설정해야 할 환경 변수를 모두 담습니다.

**다운로드는 빌드 스크립트가 하지 않습니다.** 빌드 중 네트워크 접근은 오프라인·샌드박스·벤더링 빌드를 깨뜨리고 감사도 어렵게 합니다. 내려받기는 사람이 한 번 실행하는 옵트인 스크립트(`scripts/fetch-renderer.sh`)로 분리하며, 이 스크립트가 `.sha256`으로 무결성을 검증한 뒤 풀고 설정할 환경 변수를 출력합니다. 크레이트에는 다운로드 기능 플래그를 두지 않습니다.

**버전 계약.** 아티팩트 루트의 `dioxus-compose-renderer.version`이 크레이트 버전과 다르면 빌드를 실패시킵니다. 이 파일이 없으면 워크스페이스에서 직접 빌드한 렌더러로 보고 통과시킵니다(로컬 개발 루프를 막지 않기 위함). 프로토콜 스키마 해시는 이 용도에 맞지 않습니다. 스키마 해시는 실행 시점의 프로토콜 표류를 잡는 장치이고 링크 시점에는 아티팩트 쪽 값을 신뢰할 수 있게 읽을 방법이 없으므로, 배포 계약은 버전 문자열로 고정하고 스키마 해시는 지금처럼 런타임 핸드셰이크에 둡니다.

- 수용 기준
  1. `native-renderer`를 켜고 `DIOXUS_COMPOSE_RENDERER_DIR` 없이, 워크스페이스 밖에서 푼 크레이트를 빌드하면 위 네 가지를 담은 빌드 스크립트 오류로 실패합니다(링커 오류가 아닙니다).
  2. 같은 크레이트를 아티팩트 레이아웃으로 스테이징한 디렉터리를 가리켜 빌드하면 성공합니다.
  3. 버전 파일이 크레이트 버전과 다르면 빌드가 실패하고 두 값을 모두 출력합니다.
  4. `scripts/package-renderer.sh`가 만든 파일 이름과 체크섬이 위 표와 일치하고 `shasum -a 256 -c`로 검증됩니다.

## 6. IME 수용 체크리스트 (FR-5, M1)

native-image 빌드에서 macOS와 Windows 각각 수동으로 확인합니다.

**macOS arm64 결과 (2026-09-20, Liberica NIK 25)**: 한국어 입력기로 전환하고 입력창에 한글을 입력하는 기본 경로가 동작합니다. 나머지 항목은 아직 확인 전입니다.

여기서 발견한 실패 양상을 남겨 둡니다. 등록되지 않은 입력 경로는 빌드도 렌더링도 멀쩡히 통과한 뒤, 입력기가 텍스트 필드를 건드리는 순간 Objective-C 예외로 프로세스를 abort시킵니다. Java 스택 트레이스 없이 창이 그냥 사라지므로, 이 증상이 보이면 실행 로그에서 `JNI Lookup Exception`과 그 앞의 `NoSuchMethodError`를 먼저 찾으십시오. 근본 대응은 `ImeReachabilityFeature`가 패키지 단위로 등록하는 것입니다(INTENT D9-macOS).

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

**입력 경로 등록은 반드시 빌드 시점 Feature(`ImeReachabilityFeature`)로 합니다.** 메타데이터에 `allDeclaredMethods`를 쓰면 안 됩니다. JDK가 선언만 하고 라이브러리에 넣지 않은 네이티브 메서드(`CInputMethod.nativeHandleEvent`)까지 링크 대상이 되어, Java 스택 트레이스 없이 dyld 단계에서 라이브러리 로드가 실패합니다. 반대로 등록이 부족하면 렌더링까지 정상 동작한 뒤 입력기가 텍스트 필드를 건드리는 순간 프로세스가 abort합니다.

## 7. 접근성 실험 (Q1, M1)

native-image 빌드에서 AWT 접근성 브리지가 유지되는지 확인합니다. 결과에 따라 NFR-8을 확정합니다.

- [ ] macOS VoiceOver가 Text와 Button 라벨을 읽음
- [ ] Windows Narrator가 같은 화면을 읽음
- [ ] Tab 키 포커스 순회
- [ ] 같은 화면을 JVM 실행과 비교해 차이 기록

실패하면 원인을 native-image 설정 누락(JNI/리플렉션 config, `javax.accessibility` 서비스)과 Substrate 미지원으로 구분해 기록합니다.

## 8. 열린 질문 추적

[PROJECT.md](../PROJECT.md#열린-질문)의 열린 질문 표를 참고하세요. 결정이 나면 해당 SPEC 항목을 `Agreed`로 올리고 INTENT에 결정을 기록합니다.
