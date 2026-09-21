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
- 빈 분기(`if false`)와 빈 반복은 Dioxus 플레이스홀더를 남깁니다. 플레이스홀더는 Compose 트리의 노드가 아니라서 node id 0을 싣고, Host는 그 자리에 `Insert`를 보내지 않습니다. 대신 자리(부모와 인덱스)를 **엘리먼트별로** 기억하고, 그 분기가 채워질 때 보내는 `Insert`가 그 자리를 싣습니다. node id가 같으므로, 한 화면의 플레이스홀더 둘을 node id로 구분하면 서로의 자리를 덮어씁니다.
- 자기 자신이나 자기 자손 아래로 넣는 `Insert`/`Move`는 부모 사슬에 뿌리가 없게 만듭니다. Renderer는 붙이기 전에 그런 `Insert`를 거부하고 `ProtocolError`로 알리며 가지고 있던 트리를 그대로 둡니다(NFR-7).
- 수용 기준: `Create`, `SetProp`, `SetModifier`, `Insert`, `Move`, `Remove`로 임의의 트리를 만들 수 있고, 적용 결과가 Renderer의 트리 덤프와 일치합니다.
- 수용 기준: 부모가 다른 빈 분기 둘이 동시에 채워질 때 각 분기의 내용이 자기 부모 아래에 붙고, Host가 보내는 부모 사슬은 순환하지 않습니다. 순환하는 `Insert`를 받은 Renderer는 프로세스를 중단하지 않고 `ProtocolError`를 보냅니다. **(2026-09-21 통과)**

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
- 수용 기준: §6 IME 체크리스트를 통과합니다. **핵심 5개 항목은 2026-09-21 native-image 빌드에서 확인했습니다.** 나머지 4개(멀티라인 Enter 처리, 한글 혼합 붙여넣기, 일본어/중국어 후보창 위치, TextChanged 중 조합 유지)는 미확인이므로 `Done`이 아닙니다.

### FR-6 Dioxus 렌더러 (`Agreed`)
`dioxus-core` VirtualDom의 `Mutations`를 프로토콜 Mutation으로 변환하는 렌더러를 제공합니다.
- 사용자 코드는 `rsx!`와 훅만으로 작성하고, 프로토콜을 직접 다루지 않습니다.
- 수용 기준: M0 화면을 `rsx!` 컴포넌트로 재작성했을 때 동일하게 동작합니다.

### FR-7 스키마 코드젠 (`Draft`)
위젯, 속성, Modifier, 이벤트 페이로드는 Rust에서 단일 소스로 정의하고 Kotlin 타입과 코덱을 생성합니다.
- 양쪽 모두 exhaustive match가 적용됩니다(Rust `enum`은 Kotlin `sealed interface`로 생성).
- 핸드셰이크 때 스키마 해시를 비교해서 불일치하면 초기화를 실패시킵니다.
- 수용 기준: Rust 스키마에 속성을 추가하고 Kotlin 인터프리터를 갱신하지 않으면 **빌드가 실패**합니다.
- codegen은 **자기가 컴파일된 크레이트 디렉터리에만 씁니다.** cargo가 실행 시점에 알려 준 크레이트 디렉터리가 그것과 다르면 파일을 하나도 만들지 않고 두 경로를 찍으며 0이 아닌 코드로 끝납니다. 규격과 근거는 §5.4.
- 수용 기준: `CARGO_MANIFEST_DIR`을 다른 디렉터리로 두고 codegen 바이너리를 실행하면 그 디렉터리는 비어 있는 채로 남고, 종료 코드가 0이 아니며, 메시지에 컴파일된 경로와 실행된 경로가 모두 나옵니다.

### FR-8 LazyColumn 윈도잉 (`Agreed`)
- Host는 아이템 총 개수와 안정적인 key를 알립니다.
- Renderer는 보이는 범위를 `RangeRequested`로 요청하고, Host는 **요청받은 구간만 정확히** 생성합니다. Host가 구간을 넓히지 않습니다.
- **선읽기 버퍼는 Renderer가 소유합니다.** Renderer가 가시 범위에 버퍼를 더한 값을 `start`, `count`로 보냅니다. 스크롤 위치가 Renderer에 있으므로(D5) 얼마나 미리 읽을지 아는 쪽도 Renderer입니다. Host가 따로 버퍼를 더하면 Renderer는 받은 서브트리가 전체 목록의 몇 번째 자리에 놓이는지 알 수 없습니다. `start`가 곧 첫 아이템의 전역 인덱스라는 것이 이 규칙의 핵심입니다.
- 아이템 식별: Host가 아이템마다 `Box` 래퍼 노드를 만들고 `item_key`(문자열)를 실어 보냅니다. Renderer는 그 값을 Compose `LazyColumn`의 key로 씁니다.
- Renderer는 `item_count`개짜리 실제 Compose `LazyColumn`을 그립니다. 전역 인덱스 `i`는 `i - start`번째 자식으로 그리고, 구간 밖은 빈 자리로 둡니다. 그래서 스크롤 막대와 스크롤 거리가 전체 목록 기준으로 맞습니다.
- 와이어: `RangeRequested`는 이벤트 태그 7(24바이트, `start: u32`, `count: u32`)입니다. Host는 `item_count`, `item_key`, `on_range_requested` 속성으로 선언합니다.
- **목록은 뷰포트입니다.** Host가 높이를 정해주지 않았다면(`height`, `size`, `fill_max_height`, 그리고 세로로 쌓는 부모 아래의 `weight`) Renderer는 목록에 주어진 높이를 채웁니다. 자기 아이템 높이로 줄어든 목록은 다시 커질 수 없습니다. 요청하는 구간이 지금 높이로 결정되기 때문입니다. `Row` 아래의 `weight`는 너비의 몫이므로 높이를 정하지 않습니다(13.4).
- **크기가 0인 아이템은 목록의 끝이 아니라 빈 자리입니다.** 보이는 자리는 실제로 자리를 차지하는 것만 셉니다. 화면에 있는 윈도우가 아무것도 그리지 않으면 그 윈도우를 그대로 둡니다. 그러지 않으면 크기 0 아이템이 모두 보이는 것으로 보고되어 컬렉션 전체 크기의 구간을 요청하고, 두 윈도우가 프레임마다 서로를 대체하면서 아무것도 그려지지 않습니다.
- 수용 기준: 아이템 10,000개 목록에서 생성된 노드 수가 가시 범위와 버퍼에 비례합니다. **(Host 측 통과: 가시 20 + 버퍼 4 요청에 아이템 28개)**
- 수용 기준: 화면 높이를 채우는 `Row` 안의 `LazyColumn`은 `weight`만 받은 경우에도 Row가 주는 높이를 채웁니다. 아이템이 모두 크기 0인 목록은 한 윈도우에 정착하고 경계 호출을 되풀이하지 않습니다. **(2026-09-21 통과)**

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
- `Modifier::Weight(f32)`, `RowScope`/`ColumnScope`의 weight입니다. 부모 데이터이므로 자식을 쌓는 레이아웃만 적용할 수 있고, 자식을 세로로 쌓는 컨테이너(`Card`, `Surface`, `ScrollColumn`)도 `Column`과 같게 적용합니다. 몫이 되는 축은 부모가 쌓는 축입니다: `Column` 아래에서는 높이, `Row` 아래에서는 너비이며, `Box`처럼 쌓지 않는 부모 아래에서는 어느 축도 정하지 않습니다.
- 수용 기준: 높이가 정해진 `Surface` 안에서 `weight` 1과 3을 받은 자식 둘이 높이를 1:3으로 나눕니다. **(2026-09-21 통과)**
- `Column`/`Row` 속성: `arrangement`(`Start|Center|End|SpaceBetween|SpaceAround|SpaceEvenly`), `spacing`(f32 dp) 또는 `space_role`, `alignment`(교차축 정렬).
- `Box` 속성: `alignment`(9점 정렬).
- 수용 기준: 위 속성/Modifier가 전부 `(tag, u64, u64)` 안에 들어가고, 인코딩 후 디코딩 결과가 입력과 같습니다.

#### 13.5 고도(elevation)
- `Modifier::Elevation(f32 dp)` 하나만 둡니다. **그림자를 어떻게 그릴지는 디자인 시스템의 규칙입니다.** Material 3는 톤 상승 + 그림자, HIG는 넓고 옅은 그림자, Fluent는 층 그림자 + 가는 스트로크로 같은 값을 다르게 해석합니다.
- 그림자 색·오프셋·블러를 Host가 지정하는 경로는 두지 않습니다. 두면 디자인 시스템이 값만 받는 껍데기가 됩니다.

#### 13.6 스크롤 컨테이너
- `ScrollColumn`: 콘텐츠 전체를 구성하고 세로 스크롤만 붙입니다. 스크롤 위치는 Renderer가 소유합니다(D5).
- `LazyColumn`: FR-8의 윈도잉 프로토콜을 씁니다.
- `LazyRow`: FR-8의 윈도잉 프로토콜을 가로 축에 그대로 씁니다. 프로토콜은 축과 무관하므로 두 번째 윈도잉 규약을 만들지 않습니다.
- 지연되지 않는 가로 스크롤 컨테이너(`ScrollRow`)는 여전히 넣지 않습니다. 세 샘플 어디에도 필요가 없고, 넣으면 검증되지 않은 위젯이 하나 늘어납니다.

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

위젯 태그: 1-9는 13.8 시점의 어휘이고, 10-25는 FR-15.2가 배정합니다.

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

#### 13.9 파괴적 동작과 구분선
목록과 폼을 실제로 그려 보면 13.1~13.8 어휘로는 두 가지를 말할 수 없습니다. 둘 다 새 위젯이나 새 태그 없이 해결합니다.

- **파괴적 동작**: 삭제 버튼은 세 시스템 모두에서 "평범한 버튼인데 라벨만 경고색"입니다. `ButtonVariant`를 하나 더 늘리는 대신, `Button`이 이미 있는 `Color`(태그 18) 속성을 받습니다. 노드가 색을 지정하면 그 값이 variant가 정한 라벨 색을 덮고, 지정하지 않으면 지금까지와 같습니다. 보내는 것은 여전히 `Paint::Role(ColorRole::Error)`이므로 실제 색조는 디자인 시스템이 정합니다.
  - 수용 기준: `fr13_button_label_colour_is_sent_as_a_role`.
- **구분선**: 묶인 목록은 행과 행 사이를 하이라인으로 나눕니다(HIG의 그룹 목록, Material의 Divider, Fluent의 층 스트로크). 애플리케이션 코드가 두께와 색을 들지 않는다는 것이 이 항목의 요점이고, 그 부분은 그대로입니다. 이름도 `Separator` 그대로입니다.

  **2026-09-21 정정**: 이 항목은 원래 "두께는 역할로 표현할 수 없는 상수이므로 라이브러리가 1dp를 들고, `Box` 하나로 그려서 위젯 태그를 늘리지 않는다"고 적혀 있었습니다. 15.2.4가 `Divider`를 태그 17로 넣으면서 그 근거가 둘 다 없어졌습니다. 태그는 이미 늘었고, 두께가 상수라는 전제도 틀렸습니다. Material의 divider와 Fluent의 층 스트로크와 HIG의 그룹 구분선은 두께도 색도 들여쓰기도 서로 다르며, 그것을 정하는 것이 디자인 시스템이 하는 일입니다(FR-14.1). 1dp를 라이브러리에 박아 두면 세 시스템 중 둘이 틀린 굵기로 그려집니다.

  따라서 `Separator`는 `Divider` 하나를 렌더링합니다. 같은 개념에 이름이 둘 있는 상태를 남기지 않되, 애플리케이션 코드가 이미 쓰고 있는 이름은 유지합니다.
  - 수용 기준: `fr13_separator_is_one_divider_so_the_design_system_sets_its_weight`.

#### 13.10 비활성 상태는 보여야 합니다

`Enabled`(태그 3)는 M0부터 있었지만 Renderer에서 하는 일이 클릭을 막는 것뿐이었습니다. 그래서 눌리지 않는 버튼이 눌리는 버튼과 픽셀 단위로 같게 그려졌고, 사용자는 눌러 보고 나서야 알 수 있었습니다. 선택 컨트롤 쪽은 `ControlsStyle.disabledAlpha`로 이미 흐려지고 있었으므로, 같은 개념이 위젯에 따라 있다가 없다가 하는 상태이기도 했습니다.

- **`Enabled = false`는 입력을 막고 동시에 흐려집니다.** 여섯 시스템 모두 비활성 컨트롤을 흐리게 그립니다. 얼마나 흐린지는 디자인 시스템의 값이고(FR-14.1), `ComponentRules`가 컨트롤에 대해 이미 답하고 있는 것과 같은 이름으로 버튼에 대해서도 답합니다. Host가 지정할 속성은 없습니다.
- **새 태그도 새 이벤트도 없습니다.** 이미 보내고 있는 속성을 Renderer가 마저 읽는 것입니다.
- 흐려지는 것은 컨테이너와 라벨 전체입니다. variant마다 비활성 색을 따로 두면 디자인 시스템 하나가 답해야 할 값이 네 개로 늘고, 일곱 번째 시스템이 들어올 때 그만큼 늘어납니다.
  - 수용 기준: `fr13_a_disabled_button_is_drawn_faded_in_every_design_system`.

### FR-14 디자인 시스템과 테마 모드 (`Agreed`)
디자인 시스템은 **토큰 집합 + 컴포넌트 스타일 규칙**의 한 쌍입니다. 속성을 모아 놓은 것이 아닙니다.

지원 대상은 두 단계로 나눕니다.

| 단계 | 디자인 시스템 |
|---|---|
| 1단계 | Material 3, Cupertino, WinUI/Fluent 2 |
| 2단계 | GNOME 50, KDE Breeze, Deepin |

2단계는 1단계가 동작한 뒤에 추가합니다. 14.1의 추상화가 성립하면 각각 `DesignSystem` 변형 1개와 Renderer 측 테이블 1개, 규칙 구현 1개로 끝나야 하며, 이것이 그 추상화의 실제 검증입니다.

2단계 세 시스템은 구현되어 실행 경로 위에 있습니다. 세 개 모두 `DesignSystem` 변형 1개(태그 4, 5, 6 추가, 기존 태그는 그대로), `tokens.rs`의 토큰 테이블 1개, Renderer의 `ComponentRules` 구현 1개로 끝났고 위젯·속성·Modifier·와이어 포맷은 움직이지 않았습니다. 값은 `dioxus-design-systems/`의 Kotlin 구현에서 그대로 옮겨 왔습니다.

사본이 둘인데 아무것도 비교하지 않으면 조용히 갈라집니다. 실제로 두 행이 갈라졌습니다. GNOME의 어두운 보조 강조색 위 글자색은 한쪽이 흰색, 다른 쪽이 거의 검정이었고, Breeze의 어두운 패널 색은 한쪽이 뷰 색, 다른 쪽이 `SurfaceVariant` 회색이었습니다. 이제 Rust 쪽 테스트가 `tokens.rs`의 테이블과 `dioxus-design-systems/`의 Kotlin 리터럴 테이블을 직접 비교합니다. 색, 반경, 간격이 하나라도 다르면 실패합니다.

#### 14.1 추상화
- 위젯은 **역할만 내보냅니다**(FR-13의 `ColorRole`, `TypeRole`, `ShapeRole`, `SpaceRole`, 그리고 `ButtonVariant` 같은 컴포넌트 변형).
- 디자인 시스템은 Renderer 안에 있는 **토큰 테이블 + 컴포넌트 규칙 구현** 한 쌍입니다.
- 따라서 **네 번째 디자인 시스템을 추가할 때 위젯 코드, 속성, Modifier, 와이어 포맷은 건드리지 않습니다.** Rust `DesignSystem` enum에 변형 1개, Kotlin에 테이블 1개와 규칙 구현 1개를 더하면 끝입니다.
- 수용 기준: `DesignSystem`에 변형을 하나 추가했을 때 `widgets.rs`의 위젯 정의와 Modifier/Property 스키마가 변경되지 않습니다.

#### 14.1-2 Apple 디자인 시스템의 이름과 현재 언어

**이름은 `Cupertino`입니다.** HIG(Human Interface Guidelines)는 지침 문서이지 디자인 시스템의 이름이 아닙니다. Apple은 자사 디자인 언어에 공개된 제품명을 붙이지 않으므로, 크로스플랫폼 툴킷에서 Apple 스타일 위젯 집합을 가리키는 관례적 이름인 `Cupertino`를 씁니다.

**현재 언어는 Liquid Glass입니다.** macOS 26과 iOS 26부터 Apple의 디자인 언어가 바뀌었고, 이 프로젝트가 대상으로 하는 macOS가 그 버전입니다. 평면 채움과 단색 배경을 전제한 이전 스타일로는 플랫폼을 따라간다고 할 수 없습니다. Cupertino 토큰과 컴포넌트 규칙은 다음을 표현해야 합니다.

- **재질(material)**: 컨트롤과 표면이 뒤 배경을 비춥니다. 불투명 채움이 아니라 반투명 레이어와 흐림입니다.
- **가장자리 하이라이트**: 광원을 받은 유리처럼 테두리 상단이 밝고 하단이 어둡습니다. 단색 1px 테두리와는 다릅니다.
- **동심 곡률**: 안쪽 요소의 모서리 반경이 바깥 컨테이너와 동심을 이루도록 계산됩니다. 고정된 반경 값 하나로는 표현되지 않습니다.
- **깊이**: 그림자보다 레이어의 겹침과 굴절로 깊이를 나타냅니다.

- 수용 기준
  1. `DesignSystem::Cupertino`로 그린 화면이 재질, 가장자리 하이라이트, 동심 곡률에서 Material 3 및 Fluent와 눈으로 구분됩니다.
  2. 배경이 바뀌면 그 위의 컨트롤 색이 따라 바뀝니다. 고정 색을 칠하고 끝내지 않습니다.
  3. 흐림을 지원하지 않는 환경에서도 읽을 수 있는 대체 표현이 있습니다(대비를 지키는 불투명 재질).
  4. 접근성 설정의 투명도 감소를 존중합니다.

**Renderer 구현 제약 (조사 완료, 2026-09-21)**

Compose Multiplatform은 Liquid Glass를 그릴 수 없습니다. JetBrains가 명시합니다([ios-liquid-glass](https://kotlinlang.org/docs/multiplatform/ios-liquid-glass.html)). 효과는 **시스템이** 네이티브 SwiftUI `TabView`, `NavigationStack`, 툴바 API를 통해 그리며, Compose 앱이 채택하려면 Compose 콘텐츠를 **네이티브 SwiftUI 셸로 감싸야** 합니다. iOS 26 + Xcode 26 전용이고, Compose 쪽 API는 없습니다. 이 문서는 iOS만 다루므로 데스크톱에 대해서는 아무 말도 하지 않습니다.

따라서 요구사항을 둘로 나눕니다. 경계는 **흐리게 할 대상이 우리가 그린 것이냐**입니다.

1. **우리가 그릴 수 있는 것 (1.0 범위).** 뒤에 있는 것이 우리 앱 콘텐츠인 경우입니다. 동심 곡률, 가장자리 하이라이트(상단 밝고 하단 어두움), 레이어 겹침으로 표현하는 깊이, 앱 콘텐츠 위의 반투명 컨트롤 표면, 그리고 우리가 그린 콘텐츠에 대한 Compose 흐림. 이것들은 흉내가 아니라 실제 구현이며, iOS 26 화면처럼 보이게 하는 요소의 대부분입니다.
2. **우리가 그릴 수 없는 것.** 창 뒤의 **시스템 배경**을 비추는 재질입니다. macOS에서는 `NSVisualEffectView`가 필요하고 그것은 Compose 표면 바깥의 플랫폼 경로입니다. 우리가 `NSApplication`을 직접 만들므로(INTENT D9-macOS) 이론적으로 접근 가능성은 있으나, 1.0 범위 밖입니다. iOS에서 진짜 Liquid Glass는 SwiftUI 셸의 내비게이션 크롬에만 적용되며, 그 크롬은 Rust가 작성하는 UI가 아닙니다.

수용 기준 2의 "배경"은 **컨트롤 뒤의 앱 콘텐츠**를 뜻합니다. 창 뒤의 바탕화면이 아닙니다.

**흉내만 낸 반투명으로 "구현했다"고 표시하지 않습니다.** 1번 항목은 실제로 그리는 것이므로 이 규칙에 걸리지 않지만, 2번을 했다고 주장해서는 안 됩니다.

#### 14.2 컴포넌트 변형
컴포넌트 규칙이 붙는 자리는 변형(variant) 속성입니다. 값은 디자인 시스템 중립 이름입니다.
- `Button.variant`: `Filled | Tonal | Outlined | Text`
  - Material 3: Filled/Tonal/Outlined/Text 버튼, 큰 곡률, 리플.
  - Cupertino: Filled은 강조 버튼(연속 곡률, 그림자 없음), Tonal은 회색 배경, Text는 내용 색만 쓰는 plain 버튼. 리플 대신 하이라이트.
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

- **기본값은 `Theme::adaptive(DesignSystem::Material3)`입니다.** `with_theme`을 부르지 않으면 호스트 플랫폼을 따라갑니다.
- 이 값은 원래 `unified(Material3)`였습니다. 기본값이 플랫폼마다 다르게 보이는 것이 예측 가능성을 해친다고 봤기 때문입니다. 실제로 써 보니 판단이 틀렸습니다. macOS에서 아무 설정 없이 실행하면 Material 3 화면이 나오는데, 네이티브 데스크톱 UI를 표방하는 툴킷의 첫인상으로 맞지 않고, 무엇보다 **플랫폼 적응이 동작하는지 확인할 방법이 기본 경로에 없었습니다.** 깨져 있어도 알 수 없는 기본값은 예측 가능성이 아닙니다.
- 플랫폼과 무관하게 같은 화면을 원하면 `Theme::unified(...)`를 명시합니다. 한 줄이고, 그렇게 쓰는 쪽이 의도를 드러냅니다.
- `Theme::adaptive`는 fallback 인자가 **필수**입니다. 그래서 adaptive에 "대응이 애매한 플랫폼"이 남지 않습니다. 기본값의 fallback은 Material 3입니다.

| 플랫폼 | `adaptive`가 고르는 시스템 |
|---|---|
| Android | Material 3 |
| macOS, iOS | Cupertino |
| Windows | WinUI/Fluent 2 |
| Linux (GNOME) | GNOME 50 |
| Linux (KDE) | KDE Breeze |
| Linux (그 외, 판별 불가) | Deepin |
| Web | WinUI/Fluent 2 (설정으로 Material 3로 교체 가능) |

- **Linux 데스크톱 환경 판별**: `XDG_CURRENT_DESKTOP`을 먼저 보고, 비어 있으면 `DESKTOP_SESSION`을 봅니다. 값에 `GNOME`이 포함되면 GNOME 50, `KDE`면 Breeze, 그 외와 판별 실패는 Deepin입니다. 판별 결과는 시작 시 한 번만 읽습니다.
- **Web에 플랫폼 룩은 없습니다.** 브라우저는 자기 디자인 언어를 갖지 않으므로 `adaptive`에서도 선택은 임의입니다. 기본을 Fluent 2로 두되, 앱이 설정으로 Material 3를 고를 수 있습니다. 문서에서는 Web에 대해 `unified`를 명시하는 것을 권장합니다.
- 위 표가 적용됩니다. `fallback` 인자는 자기 디자인 언어가 없는 플랫폼(판별 불가 포함)이 쓰므로 여전히 필수입니다.
- **GNOME 50 주의**: 버전을 명시한 것은 GNOME의 디자인 언어가 릴리스마다 바뀌기 때문입니다. 참조한 문서와 버전은 토큰 테이블의 `reference` 문자열에 남아 있습니다.
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
- `DesignSystem` 태그: `Material3 = 1`, `Cupertino = 2`, `Fluent = 3`. 태그 값은 바뀌지 않습니다. 이전 이름은 `AppleHig`였습니다.
- `ColorScheme` 태그: `Light = 1`, `Dark = 2`, `FollowSystem = 3`.
- 수용 기준: `Theme::unified(...)`로 띄운 앱의 첫 배치 첫 레코드가 `SetTheme`이고 `adaptive = false`입니다. `Theme::adaptive(...)`이면 `adaptive = true`이며 `fallback`이 인자로 준 시스템입니다.

#### 14.6 Renderer 구현자가 채워야 할 표
디자인 시스템마다 아래 8개가 필요합니다. 채워지면 위젯 코드는 건드리지 않습니다.

1~4번은 14.4에 따라 Rust 스키마에서 코드젠으로 생성되어 `Protocol.gen.kt`의 `DesignTokens`에 이미 들어 있습니다. Renderer 구현자는 **5~8번과, 1~4번을 Compose에 배선하는 일**을 맡습니다.
1. `ColorRole` 14개 × {Light, Dark} 색값
2. `TypeRole` 9개 → 크기/굵기/행간/자간/폰트
3. `ShapeRole` 6개 → 곡률(HIG는 연속 곡률)
4. `SpaceRole` 7개 → dp
5. `Modifier::Elevation(dp)` → 그림자/톤/스트로크 렌더링 규칙
6. `ButtonVariant` 4개 → 배경·전경·테두리·눌림 표현
7. 모션: 상태 전환 duration과 easing
8. 입력 필드의 틀(14.7)

#### 14.7 입력 필드의 틀 (`Done`)

`TextField`는 지금까지 아무 틀 없이 그려졌습니다. 배경도, 테두리도, 안쪽 여백도, 포커스 표시도 없는 맨 편집 영역 하나입니다. 그래서 여섯 시스템의 입력 필드가 전부 똑같이 보였고, 틀을 원하는 화면은 `Background`와 `Border`와 `PaddingRole`을 직접 붙여 왔습니다. 둘 다 14.1에 어긋납니다.

필드의 틀은 여섯 시스템이 가장 눈에 띄게 갈라지는 자리입니다. Material 3는 채운 상자에 밑줄을 긋고 포커스에서 밑줄이 두꺼워집니다. Cupertino는 둥근 사각형에 옅은 채움이고 테두리를 거의 쓰지 않습니다. Fluent 2는 사각형에 가까운 상자에 아래쪽 강조선을 두고 그 선만 포커스에서 굵어집니다. Adwaita는 6px 둥근 채움에 포커스에서 강조색 테두리가 생깁니다. Breeze는 한 겹 하이라이트 테두리로 포커스를 표시합니다. Deepin은 큰 반경의 채움에 테두리 없이 포커스에서만 선이 나타납니다. 그것을 Host가 칠한다는 것은 디자인 시스템이 정할 것을 애플리케이션이 정하고 있다는 뜻입니다.

- **위젯은 역할만 내보냅니다**(14.1). 애플리케이션은 필드가 있다고 말할 뿐이고, 틀은 디자인 시스템이 그립니다. 모든 위젯이 공통으로 갖는 Modifier(배경·테두리·반경·여백)는 `TextField`에도 그대로 남지만 그것은 어느 위젯에나 있는 탈출구이지 필드의 틀을 그리는 수단이 아닙니다. `Button`이 공통 Modifier를 가지면서도 자기 컨테이너를 스스로 그리는 것과 같습니다. 포커스 표시를 지정하는 속성은 두지 않습니다.
- **Renderer 쪽 확장은 `ComponentRules`에 `field()` 하나를 더하는 것으로 끝납니다.** 일곱 번째 디자인 시스템은 여전히 구현 하나입니다.
- **포커스는 Renderer의 상태입니다**(D5). 포커스가 들고 나면서 틀이 바뀌는 것은 경계를 넘지 않고, Host는 그 전환을 알지 못합니다.
- 규칙이 답하는 값: 채움색, 포커스 시 채움색, 테두리색과 두께, 포커스 시 테두리색과 두께, 밑줄만 그리는지, 모서리, 안쪽 여백, 커서색, 최소 높이.
- 수용 기준
  1. Modifier를 하나도 붙이지 않은 `TextField`가 여섯 시스템에서 서로 다른 틀로 그려지고, 어느 것도 틀 없는 맨 편집 영역이 아닙니다.
  2. 필드에 포커스가 들어가면 그 시스템이 정한 대로 틀이 바뀝니다.
  3. 같은 `TextField` 선언이 디자인 시스템을 바꾸면 다른 모양이 됩니다.

### FR-15 위젯 어휘 (`Agreed`)

M0의 위젯 9개는 데모를 굴리는 데 필요했던 만큼이지 설계된 범위가 아니었습니다. 1.0의 목표 어휘를 여기에 고정합니다.

#### 15.1 기준: 세 시스템 공통 축

위젯은 **Material 3, Cupertino, Fluent 2 모두에 대응물이 있는 것만** 코어 스키마에 넣습니다. 세 곳에 다 있어야 FR-14의 역할 기반 설계가 성립합니다. 위젯이 역할만 내보내고 그리는 방법은 디자인 시스템이 정하려면, 애초에 세 시스템이 그 개념을 공유해야 합니다.

`androidx.compose.material3`를 1:1로 미러링하지 않는 이유가 이것입니다. `AssistChip`, `NavigationRail`, `ExtendedFloatingActionButton`은 Material 고유 어휘이고 Cupertino에 대응물이 없습니다. 코어에 넣으면 Cupertino와 Fluent는 Material을 흉내 내는 스킨이 되고, FR-14.2의 "네 번째 디자인 시스템을 추가할 때 위젯 코드를 건드리지 않는다"가 무너집니다. 그런 위젯은 FR-11 확장 패키지가 맡습니다.

#### 15.2 코어 어휘

기존(태그 1-9): `Column=1, Row=2, Box=3, Text=4, TextField=5, Button=6, Spacer=7, LazyColumn=8, ScrollColumn=9`

추가(태그 10-25):

| 태그 | 위젯 | 세 시스템 대응 |
|---|---|---|
| 10 | `Image` | 공통. FR-16 에셋 핸들을 받습니다 |
| 11 | `Icon` | 공통. FR-16 에셋 핸들과 `ColorRole` 틴트 |
| 12 | `Checkbox` | Material Checkbox / UIKit 체크 / Fluent CheckBox |
| 13 | `RadioButton` | 공통 |
| 14 | `Switch` | Material Switch / UISwitch / Fluent Toggle |
| 15 | `Slider` | 공통 |
| 16 | `ProgressIndicator` | 공통. `determinate: bool`, `circular: bool` |
| 17 | `Divider` | 공통. `vertical: bool` |
| 18 | `Card` | Material Card / 그룹 박스 / Fluent Card |
| 19 | `Surface` | 배경과 고도를 갖는 컨테이너. FR-13.5 Elevation을 씁니다 |
| 20 | `Dialog` | 공통. 모달. 위치와 애니메이션은 디자인 시스템 규칙 |
| 21 | `Menu` | 공통. 앵커 노드에 붙는 팝업 |
| 22 | `Tabs` | Material TabRow / UISegmentedControl / Fluent Pivot |
| 23 | `TopAppBar` | Material TopAppBar / UINavigationBar / Fluent CommandBar |
| 24 | `LazyRow` | FR-8 윈도잉의 가로 축 |
| 25 | `Tooltip` | 공통. 데스크톱 전용 동작이 아니라 접근성 설명으로도 쓰입니다 |
| 26 | `Canvas` | FR-17의 커스텀 드로잉 |
| 27 | `DatePicker` | Material DatePicker / `UIDatePicker` / Fluent CalendarDatePicker |
| 28 | `TimePicker` | Material TimePicker / `UIDatePicker`(시간 모드) / Fluent TimePicker |
| 29 | `Dropdown` | 목록에서 하나 고르기. Material ExposedDropdownMenu / Cupertino Picker / Fluent ComboBox |

#### 15.2.1 선택기(picker)에 대한 주의

날짜와 시간 선택기는 세 시스템에서 **겉모습만 다른 것이 아니라 상호작용 자체가 다릅니다.** Material은 달력 격자와 다이얼, Cupertino는 휠, Fluent는 달력 플라이아웃입니다. FR-14의 역할 기반 설계가 특히 중요한 자리입니다. 위젯은 **값과 범위와 변경 이벤트만** 내보내고, 어떤 방식으로 고르게 할지는 전적으로 디자인 시스템이 정합니다. Host가 "휠로 고르게 하라"고 지시할 수 있는 속성을 두면 안 됩니다.

- 값은 에폭 기준 정수로 주고받습니다. 날짜는 `days: i64`(1970-01-01 기준), 시간은 `minutes: u32`(자정 기준). 고정 레이아웃을 유지하고 타임존 해석이 경계를 넘지 않습니다.
- **타임존과 로케일은 Renderer가 소유합니다(D5).** 표시 형식, 주 시작 요일, 12/24시간제는 플랫폼 설정을 따릅니다. Host가 형식 문자열을 보내는 경로는 두지 않습니다. 보내면 플랫폼을 따라간다는 말이 거짓이 됩니다.
- 범위 제한은 `min`과 `max`로 같은 단위로 보냅니다.

여기까지가 15.2의 어휘 29개이고, FR-21이 탐색과 시트로 32개까지 넓힙니다.

#### 15.2.2 속성 태그 블록

FR-13.8이 코어 속성 태그를 26까지 썼고, FR-11 확장 예제가 27을 가져갔습니다. 15.2의 새 위젯들은 서로 다른 시점에 구현되므로, 태그가 겹치지 않도록 블록을 미리 나눠 둡니다. 블록 안에서는 앞에서부터 채우고, 남는 자리는 비워 둡니다.

| 위젯 태그 | 속성 태그 블록 |
|---|---|
| 10-17 (`Image` ~ `Divider`) | 28-39 |
| 18-25 (`Card` ~ `Tooltip`) | 40-49 |
| 26-29 (`Canvas` ~ `Dropdown`) | 50-59 |
| 30-32 (`Navigation` ~ `Sheet`) | 60-69 (FR-21) |

#### 15.2.3 컨테이너, 오버레이, 탭(태그 18-25)

**새 속성은 셋뿐입니다**: `Open=40`(bool), `OnDismiss=41`(핸들러 id), `SelectedIndex=42`(u32).

**새 이벤트 태그는 만들지 않습니다.** 해제와 탭 선택은 값을 싣지 않는 알림이므로 기존 `Clicked`(이벤트 태그 1)를 각자의 핸들러 id로 보냅니다. 어떤 탭이 선택됐는지는 노드 id가 말해 줍니다.

- **D5**: Dialog의 열림, Menu의 펼침, Tabs의 선택, 스크롤 위치는 전부 Renderer의 상태입니다. Host 속성은 초기값과 바깥에서 들어온 변경을 주고, Renderer는 바뀐 사실만 이벤트로 알립니다. 여는 애니메이션이나 탭 전환이 프레임마다 Rust를 왕복하지 않습니다.
- **자식 규약**: `Menu`는 자식 0이 앵커이고 나머지가 팝업 항목입니다. `Tooltip`은 자식이 앵커이고 설명은 `text` 속성입니다. `Tabs`의 자식이 곧 탭이며, 탭 `i`를 고르면 자식 `i`의 `OnClick` 핸들러로 `Clicked`를 보냅니다. `Dialog`, `Card`, `Surface`, `TopAppBar`의 자식은 그대로 내용입니다.
- **역할만 내보냅니다**(FR-14.1): 카드의 배경과 모서리, 대화상자의 스크림과 위치, 메뉴 팝업의 그림자, 탭 인디케이터의 모양, 툴팁의 지연 시간은 전부 디자인 시스템의 규칙입니다. Host가 지정할 속성을 두지 않습니다. Renderer 쪽 확장은 `ComponentRules`에 `container(ContainerRole)`와 `tabs()` 둘을 더하는 것으로 끝나고, 네 번째 디자인 시스템은 여전히 구현 하나입니다.
- **고도**: `Surface`와 `Card`는 `Modifier::Elevation`(FR-13.5)을 그대로 받습니다. 값이 없으면 디자인 시스템이 정한 기본 고도를 씁니다.
- **`LazyRow`**: FR-8의 `item_count`, `item_key`, `on_range_requested`를 그대로 씁니다. 가로 축이라는 것 말고 다른 점이 없어야 하고, 선읽기 버퍼도 같은 이유로 Renderer가 소유합니다.

#### 15.2.4 선택 컨트롤과 표시기(태그 12-17)

`Checkbox=12`, `RadioButton=13`, `Switch=14`, `Slider=15`, `ProgressIndicator=16`, `Divider=17`입니다. 여섯 모두 다른 위젯과 같은 Modifier 11종을 그대로 받습니다.

**새 속성 태그는 다섯입니다.** 15.2.2의 28-39 블록에서 `Image`와 `Icon`이 쓰는 28-31 뒤를 이어 32부터 채웁니다.

| 태그 | 속성 | 값 | 쓰는 위젯 |
|---|---|---|---|
| 32 | `Checked` | bool | `Checkbox`, `RadioButton`, `Switch` |
| 33 | `Steps` | i64 | `Slider`의 양 끝 사이 불연속 지점 개수. 0이면 연속 |
| 34 | `Determinate` | bool | `ProgressIndicator` |
| 35 | `Circular` | bool | `ProgressIndicator` |
| 36 | `Vertical` | bool | `Divider` |

**값과 범위는 새 태그를 따지 않습니다.** `Slider`의 위치와 `ProgressIndicator`의 진행도는 15.2.5가 이미 배정한 `Value`(태그 51)를 쓰고, `Slider`의 양 끝은 `Min`(52)과 `Max`(53)를 씁니다. 같은 개념을 스키마에 두 번 넣지 않는다는 13.1의 규칙이 블록 배정보다 우선합니다. 태그는 하나인데 값의 타입은 위젯이 정합니다. 픽커는 에폭 정수를, 슬라이더는 f32를 담습니다.

`RadioButton`의 선택 여부도 `Checked`를 씁니다. 와이어에서는 "켜져 있는가"라는 bool 하나입니다. `selected`라는 이름은 Compose 관례이므로 rsx 속성 이름으로만 남습니다(PR-7).

변경 핸들러는 기존 `OnValueChange`(태그 6)를 그대로 씁니다.

- **네 컨트롤 모두 15.2.5의 `ValueChanged`(이벤트 태그 16)를 씁니다.** 토글 셋은 새 상태를 0.0 또는 1.0으로 싣고, `Slider`는 값 자체를 싣습니다. bool을 위해 이벤트를 하나 더 만들면 같은 개념("이 컨트롤의 값이 이렇게 되었다")에 이름이 둘 생기고, 컨트롤이 늘 때마다 또 나눠야 합니다.
- **드래그 중과 누르는 중의 상태는 Renderer가 소유합니다(D5).** 손가락을 따라가는 슬라이더 위치와 토글의 전환 애니메이션이 프레임마다 경계를 넘지 않습니다. `Checked`와 `Value`는 그 상태의 시작값이고 바깥에서 들어온 변경을 전달하는 통로입니다.
- **역할만 내보냅니다**(FR-14.1). 체크 표시의 모양과 크기, 스위치가 리플을 내는지 눌린 동안 흐려지는지, 트랙과 엄지의 치수, 부정형 표시기가 도는 속도, 구분선의 두께와 색은 전부 디자인 시스템의 규칙입니다. Host가 지정할 속성을 두지 않습니다. Renderer 쪽 확장은 `ComponentRules`에 `controls()` 하나를 더하는 것으로 끝나고, 일곱 번째 디자인 시스템은 여전히 구현 하나입니다.
- `ProgressIndicator`는 `determinate`가 거짓이면 `Value`를 읽지 않습니다. `circular`는 모양을 고르는 것이지 치수를 정하는 것이 아닙니다.
- `Divider`는 `vertical`이 정하는 축 말고는 아무것도 싣지 않습니다.

**Material 3는 `androidx.compose.material3`가 그립니다.** 나머지 시스템은 우리 드로잉입니다.

그 라이브러리는 이미 Renderer의 의존성입니다(FR-11의 확장 예제가 씁니다). Material 3 컨트롤을 손으로 다시 그리면 리플, 상태 레이어, 접근성 시맨틱, 최소 터치 영역, 모션을 전부 따라 만들어야 하고, 그렇게 만든 것은 자신이 따른다고 주장하는 명세와 반드시 어긋나기 시작합니다. 스펙을 그대로 구현한 것이 이미 있는데 흉내를 유지보수할 이유가 없습니다. Cupertino와 Fluent에는 그런 라이브러리가 없으므로 우리가 그립니다.

이것이 15.1의 기준과 충돌하지 않습니다. 15.1은 **코어 어휘에 무엇을 넣을지**의 기준입니다. 여섯 컨트롤은 세 시스템 모두에 대응물이 있으므로 그 시험을 이미 통과했고, 통과한 위젯을 어느 시스템에서 무엇으로 그리는지는 그 시스템의 규칙입니다. Cupertino가 Material을 흉내 내는 스킨이 되는 경우가 15.1이 막으려는 것이고, Material이 Material로 그려지는 것은 그 반대입니다.

**그래서 `ComponentRules`가 답하는 것이 둘입니다.**

- `controls()`는 **치수와 색**을 답합니다(`ControlsStyle`). 우리 드로잉이 읽습니다.
- `controlWidgets`는 **누가 그리는지**를 답합니다. 기본값이 `controls()`에서 그리는 구현이므로, 일곱 번째 디자인 시스템은 여전히 `controls()` 하나만 구현하면 되고 이 속성은 건드리지 않습니다. Material 3만 이 기본값을 덮는 유일한 arm입니다.

두 갈래로 나눈 이유는 기본값이 없으면 새 시스템마다 컨트롤 여섯 개의 컴포저블을 처음부터 쓰게 되기 때문입니다. 그것은 "디자인 시스템 하나 = 구현 하나"라는 FR-14.2의 계약을 깨뜨립니다.

**FR-11의 `LinearProgressIndicator`(태그 100)와는 다른 위젯입니다.** 그쪽은 확장 메커니즘이 동작한다는 것을 보이는 예제입니다. 가로 막대만 있고, 결정형만 있고, 어느 디자인 시스템이 켜져 있든 Material 3로 그려지며, 코어가 아니라 확장 태그 블록에 있습니다. 여기의 `ProgressIndicator`(태그 16)는 코어 어휘이고, 결정형과 부정형을 모두 그리며, `circular`로 막대와 링을 고르고, 생김새는 활성 디자인 시스템이 정합니다. 둘 다 남깁니다. 예제를 지우면 FR-11이 보여주는 것이 없어지고, 예제를 코어로 승격하면 확장 태그 블록을 실제로 쓰는 코드가 저장소에서 사라집니다.

#### 15.2.5 값 변경 이벤트 (`ValueChanged`, 태그 16)

값을 가진 위젯이 하나의 이벤트를 공유합니다. 픽커 3종(DatePicker, TimePicker, Dropdown), 그리고 15.2.4의 `Checkbox`, `RadioButton`, `Switch`, `Slider`가 씁니다.

```
tag 16, 24바이트: node_id: u32, handler_id: u64, value: f64
```

**`f64` 하나로 통일합니다.** 픽커는 정수(에폭 일수, 자정 기준 분, 선택 위치)를 보내고 Slider는 연속값을 보내는데, `f64`는 2^53까지의 정수를 오차 없이 표현하므로 양쪽이 한 타입에 들어갑니다. 이벤트 종류를 둘로 나누면 같은 개념에 이름이 둘 생기고, 위젯이 늘 때마다 또 나눠야 합니다.

- **사용자 코드는 `f64`를 보지 않습니다.** `DatePicker`의 `on_change`는 `EventHandler<i64>`이고, `Slider`는 `EventHandler<f32>`입니다. 변환은 컴포넌트가 합니다. 와이어 표현이 하나라고 해서 API까지 하나여야 하는 것은 아닙니다.
- 토글도 같은 이벤트를 씁니다. 꺼짐이 0.0, 켜짐이 1.0입니다. `Checkbox`의 `on_change`는 `EventHandler<bool>`이고, 변환은 컴포넌트가 합니다.
- 속성 쪽도 같습니다. `PropertyKind::Value`는 하나이고, 값의 해석은 위젯이 정합니다.

#### 15.3 넣지 않는 것

| 제외 | 이유 |
|---|---|
| `Chip`, `FAB` | Material 고유 어휘. Cupertino 대응물 없음. FR-11 확장으로 |
| ~~`NavigationBar`, `NavigationRail`, `BottomSheet`~~ | **철회했습니다.** 위젯 이름이 아니라 개념을 기준으로 다시 따져 보니 여섯 시스템 모두에 있습니다. FR-21이 코어 어휘로 편입합니다 |
| `Grid` | 지연 그리드는 FR-8과 다른 윈도잉 프로토콜이 필요합니다. 별도 요구사항으로 분리 |
| `RichText`, 마크다운 | 위젯이 아니라 `Text`의 스팬 모델 문제입니다. 별도 요구사항으로 분리합니다 |
| 드래그 앤 드롭(파일), 리사이즈 핸들 | 플랫폼 파일 프로토콜이 따로 필요합니다. 1.0 이후. 포인터 드래그 자체는 FR-18에 있습니다 |

#### 15.4 수용 기준

- 29개 위젯 각각이 세 디자인 시스템에서 렌더링되고, `DesignShowcase`에 나타납니다.
- `DatePicker`가 세 시스템에서 **서로 다른 고르기 방식**으로 나타납니다. 같은 모양을 세 번 그린 것이면 실패입니다.
- 위젯을 추가해도 `DesignSystem` enum과 규칙 테이블 외에는 바뀌지 않습니다(FR-14.2 회귀 검사).
- 각 위젯에 프로토콜 왕복 테스트가 있습니다(PR-4 벡터).

### FR-16 에셋 전달 (`Agreed`)

`Image`와 `Icon`은 리소스가 경계를 넘어야 합니다. 13.7에서 "별도 요구사항으로 분리"한 그 프로토콜입니다.

#### 16.1 제약

PR-4의 고정 레이아웃 레코드에는 이미지 바이트가 들어가지 않습니다. 그리고 PR-1의 동기 경계는 배치를 호출 스택 안에서 소비하므로, 배치가 가리키는 포인터는 호출이 끝나면 무효입니다. 이미지는 프레임보다 오래 살아야 합니다.

#### 16.2 설계: 에셋 핸들

1. Host가 `RegisterAsset` 명령으로 에셋을 등록합니다. 레코드는 `(asset_id: u32, kind: u16, offset: u32, length: u32)`이고, 바이트는 배치의 문자열 영역과 같은 방식으로 레코드 뒤에 놓입니다(PR-4 그대로).
2. Renderer는 **그 호출 안에서 바이트를 복사해** 자기 캐시에 `asset_id`로 보관합니다. 이미지 디코딩은 Renderer가 합니다. 복사는 등록 시점 1회뿐이고 프레임마다 일어나지 않으므로 NFR-9의 프레임 예산 밖입니다.
3. `Image`와 `Icon` 위젯은 `asset_id`만 속성으로 받습니다. 고정 레이아웃이 유지됩니다.
4. `ReleaseAsset(asset_id)`로 캐시에서 내립니다. Host가 소유권을 갖습니다.
5. 등록되지 않은 `asset_id`는 `ProtocolError`입니다(NFR-7). 프로세스를 중단시키지 않습니다.

- `kind`: `Png=1, Jpeg=2, Svg=3, VectorIcon=4`. Renderer가 해석할 수 없는 종류는 `ProtocolError`입니다.
- **시스템 아이콘 이름을 보내는 경로는 두지 않습니다.** 이름을 보내면 존재 검증이 런타임으로 밀리고, 폰트 패밀리를 뺀 이유(13.7)와 같은 문제가 생깁니다. 디자인 시스템별 기본 아이콘 세트는 Renderer 번들이 소유하고, `VectorIcon` 종류가 그 세트의 **역할**(`Back`, `Close`, `Search` 등 닫힌 enum)을 가리킵니다. 그래야 Cupertino에서는 SF Symbols 모양, Material에서는 Material Symbols 모양이 나옵니다.

#### 16.3 수용 기준

- PNG 1장을 등록하고 `Image`로 그린 뒤 해제하면, 해제 후 같은 `asset_id` 사용이 `ProtocolError`가 됩니다.
- 등록은 프레임당 0회인 정상 경로에서 스틸 프레임 할당이 0입니다(NFR-9).
- 같은 `VectorIcon` 역할이 세 디자인 시스템에서 각 시스템의 아이콘으로 그려집니다.

### FR-17 커스텀 드로잉 (`Agreed`)

FR-15의 위젯 25개는 사각형, 모서리, 테두리, 그림자까지입니다. 기존 위젯을 아무리 조합해도 **새 픽셀은 나오지 않습니다.** 차트, 스파크라인, 서명 패드, 커스텀 로딩 표시, 아바타 링이 전부 여기 걸립니다.

Renderer가 AOT 컴파일된 바이너리이므로 Host가 그리기 코드를 보낼 수는 없습니다(D7). 대신 **닫힌 드로잉 명령 집합**을 보냅니다.

#### 17.1 Canvas 위젯

위젯 태그 `Canvas = 26`(FR-15.2에도 등재). 자식 노드 대신 드로잉 명령 목록을 갖습니다. 크기는 Modifier가 정하고, 좌표는 위젯 좌상단 기준 dp입니다.

#### 17.2 드로잉 명령

명령도 고정 레이아웃 레코드입니다(PR-4). 한 명령은 `(tag: u16, length: u16, paint: u64, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32)`입니다.

| 태그 | 명령 | a-f |
|---|---|---|
| 1 | `Line` | x1, y1, x2, y2, stroke_width |
| 2 | `Rect` | x, y, w, h, stroke_width (0이면 채움) |
| 3 | `RoundRect` | x, y, w, h, radius, stroke_width |
| 4 | `Circle` | cx, cy, radius, stroke_width |
| 5 | `Arc` | cx, cy, radius, start_deg, sweep_deg, stroke_width |
| 6 | `PolylineRef` | points 에셋 id(FR-16), stroke_width |
| 7 | `TextAt` | 문자열 (offset, length), x, y, `TypeRole` |

- `paint`는 FR-13.1의 `Paint`를 그대로 씁니다. 따라서 **`ColorRole`을 보낼 수 있고, 그러면 디자인 시스템이 색을 정합니다.** Canvas라고 해서 FR-14 바깥으로 나가지 않습니다.
- 점이 많은 경로는 레코드에 넣지 않고 FR-16 에셋으로 등록한 뒤 id로 참조합니다(`PolylineRef`). 그래서 명령 레코드가 고정 길이를 유지합니다.
- 그라데이션, 셰이더, 이미지 브러시, 임의 베지어 경로는 넣지 않습니다. 13.7의 제외 사유가 그대로 적용됩니다.
- 명령 목록이 바뀌지 않은 프레임은 명령을 다시 보내지 않습니다. 목록은 노드 속성이고, 변경 없으면 Mutation이 없습니다.

#### 17.2.1 명령 목록을 나르는 속성

- 속성 태그는 15.2.2의 26-29 블록에서 `Commands=50` 하나입니다. Canvas가 갖는 유일한 속성입니다.
- 명령 레코드는 36바이트 고정입니다(`tag`와 `length`가 각 2바이트, `paint`가 8바이트, `a`-`f`가 각 4바이트). `length`는 항상 36이고, 길이를 읽는 쪽이 모르는 태그를 건너뛸 수 있게 자리를 지킵니다.
- `a`-`f`는 32비트 낱말 6개입니다. 기하 값은 `f32`로, 에셋 id와 문자열 오프셋과 역할 태그는 `u32`로 읽습니다.
- 속성 값 종류에 `Bytes`를 더합니다. 레이아웃은 문자열과 같은 `(offset: u32, length: u32)`이고, 바이트도 같은 방식으로 레코드 영역 뒤에 놓입니다(PR-4 그대로). UTF-8 검사만 하지 않습니다.
- 값은 명령 배열 다음에 `TextAt`의 문자열들이 이어붙은 덩어리 하나입니다. `TextAt`의 `(offset, length)`는 **그 덩어리의 시작 기준** 바이트 오프셋이라, 명령 목록은 배치 안에서 자기 자신만으로 해석됩니다.

#### 17.3 수용 기준

- 꺾은선 차트 하나를 Rust `rsx!`만으로 그립니다. Kotlin 변경이 없습니다.
- `ColorRole`로 그린 선이 세 디자인 시스템에서 각 시스템의 색으로 나옵니다.
- 명령 목록이 그대로인 프레임에서 Host 힙 할당이 0입니다(NFR-9).

### FR-18 포인터 제스처 (`Agreed`)

입력이 `Clicked`와 `KeyDown`뿐이었습니다. 제스처 인식은 Compose 쪽에서 일어나므로 위젯 조합으로 만들어낼 수 없고, 없으면 Host가 우회할 방법도 없습니다.

#### 18.1 이벤트

이벤트 태그 8부터 이어 붙입니다(1-7은 배정 완료). **8-15는 제스처가 예약합니다.** 값 변경 이벤트는 16번입니다(15.2.5).

**2026-09-21 정정**: 픽커 구현이 SPEC에 없는 `ValueChanged`를 태그 8에 넣어 이 예약을 침범한 채 develop에 들어갔습니다. 코드와 SPEC이 어긋나면 코드를 고친다는 규칙에 따라, 그 이벤트를 16번으로 옮기고 아래 15.2.5로 명세합니다.

| 태그 | 이벤트 | 페이로드 |
|---|---|---|
| 8 | `PointerEnter` | 없음 |
| 9 | `PointerExit` | 없음 |
| 10 | `LongPress` | x, y (f32) |
| 11 | `ContextMenu` | x, y (f32). 우클릭, 그리고 터치의 관례적 대응 동작 |
| 12 | `DragStart` | x, y (f32) |
| 13 | `DragMove` | x, y, dx, dy (f32) |
| 14 | `DragEnd` | x, y (f32) |
| 15 | `Scroll` | dx, dy (f32). 휠과 트랙패드 |

- 좌표는 이벤트를 받은 노드 기준 dp입니다. 화면 좌표를 보내지 않습니다. Host는 창 위치를 모르고, 알 필요도 없습니다.
- 구독하지 않은 제스처는 **인식기를 붙이지 않습니다.** `on_drag_move`를 선언한 노드에만 드래그 인식이 걸립니다. 모든 노드에 거는 것은 NFR-9 위반입니다.
- `DragMove`는 프레임당 최대 1건으로 합칩니다. 포인터 이벤트는 프레임보다 자주 오므로 합치지 않으면 프레임당 여러 번 VirtualDom을 돌게 됩니다.
- FR-12의 `consume`이 그대로 적용됩니다. 부모로 전파할지는 핸들러가 정합니다.
- 플랫폼 차이는 Renderer가 흡수합니다. 터치에는 호버가 없으므로 iOS와 Android에서 `PointerEnter`는 발생하지 않습니다. Host 코드는 그대로 두고, 호버로만 보이는 UI를 두지 않는 것은 애플리케이션의 책임입니다.

#### 18.1.1 핸들러 속성 태그

제스처는 어느 위젯에나 붙으므로 15.2.2의 위젯별 블록에 들어가지 않습니다. 그 블록들 뒤인 60번부터 이어 붙입니다.

`OnPointerEnter=60, OnPointerExit=61, OnLongPress=62, OnContextMenu=63, OnDragStart=64, OnDragMove=65, OnDragEnd=66, OnScroll=67`

값은 기존 핸들러 속성과 같은 핸들러 id이고, 값이 없으면(`None`) 구독이 사라집니다. Renderer는 핸들러 id가 있는 축에만 인식기를 붙입니다.

#### 18.2 수용 기준

- 호버로 버튼을 띄우는 코드가 macOS에서 동작하고, iOS에서 이벤트가 오지 않아도 앱이 깨지지 않습니다.
- 드래그로 항목을 옮기는 목록에서 `DragMove`가 프레임당 1건입니다.
- 제스처를 선언하지 않은 노드에는 인식기가 붙지 않습니다(인터프리터 단위 테스트).

### FR-19 창 크롬 (`Agreed`)

데스크톱 창이 지금 플랫폼 기본 타이틀바를 그대로 씁니다. macOS는 오래전에 타이틀바가 앱 내용 안으로 들어가는 형태로 바뀌었고, GNOME은 클라이언트 사이드 데코레이션(CSD)이 기본이며, Windows 11도 캡션을 앱이 그리는 쪽이 일반적입니다. 지금 모습은 어느 플랫폼에서도 최신이 아닙니다.

**기본값을 현대적 크롬으로 바꿉니다.** 앱이 아무것도 요청하지 않아도 그렇게 보여야 합니다. 네이티브 데스크톱 UI를 표방하면서 10년 전 창을 띄우는 것은 FR-14.3에서 기본 테마를 adaptive로 바꾼 것과 같은 이유로 맞지 않습니다.

#### 19.1 플랫폼별 방식

세 플랫폼이 서로 다른 방식을 요구합니다. 공통 추상화를 억지로 만들지 않고, 플랫폼이 제공하는 것을 그대로 씁니다.

| 플랫폼 | 방식 | 창 버튼 |
|---|---|---|
| macOS | 타이틀바를 투명하게 하고 콘텐츠를 그 아래까지 확장 | **시스템이 그립니다** (신호등 버튼 유지) |
| Windows | 장식 없는 창 + 앱이 캡션을 그림 | 우리가 그립니다 |
| Linux | 장식 없는 창 + 앱이 캡션을 그림 (CSD) | 우리가 그립니다 |

**macOS에서 신호등 버튼을 직접 그리지 않습니다.** 위치, 모양, 호버 동작, 전체화면 전환, 접근성 레이블이 전부 시스템 소유입니다. 흉내 내면 반드시 어긋나고, 그건 네이티브를 표방하는 프로젝트에서 가장 눈에 띄는 실패입니다. AWT가 제공하는 클라이언트 속성(`apple.awt.fullWindowContent`, `apple.awt.transparentTitleBar`, `apple.awt.windowTitleVisible`)으로 충분하며 JNI가 필요 없습니다.

#### 19.2 콘텐츠 안전 영역

콘텐츠가 타이틀바 아래까지 확장되면 **그 영역과 겹치는 문제**가 생깁니다. macOS 신호등 버튼 자리에 앱의 버튼을 그리면 둘 다 못 쓰게 됩니다.

- Renderer가 `WindowInsets` 값을 알고 있으며, 루트 노드에 적용합니다. Host는 이 값을 모릅니다.
- `TopAppBar`(FR-15.2 태그 23)를 루트 자식으로 두면, Renderer가 그것을 캡션 영역으로 인식해 시스템 버튼을 피해 배치하고 드래그 영역으로 만듭니다. `TopAppBar`가 없으면 콘텐츠 전체에 인셋을 적용합니다.
- **드래그 영역**: 캡션 영역의 빈 공간을 잡고 창을 움직일 수 있어야 합니다. 상호작용 위젯(Button, TextField 등) 위에서는 드래그가 일어나지 않습니다.

#### 19.3 Host가 정할 수 있는 것

창 자체는 Renderer 소유이지만, 몇 가지는 애플리케이션이 정해야 합니다.

```rust
LaunchBuilder::new()
    .with_window(Window::new().title("Chat").size(900.0, 640.0))
    .launch(app);
```

- `title`, `size`, `min_size`, `resizable`
- `chrome`: `Chrome::Modern`(기본) 또는 `Chrome::System`(플랫폼 기본 타이틀바)
- `Chrome::System`은 없애지 않습니다. 시스템 타이틀바가 필요한 도구형 앱이 있고, 무엇보다 **Modern이 깨졌을 때 돌아갈 곳**이 있어야 합니다.
- 창 버튼의 색, 위치, 모양을 Host가 지정하는 경로는 두지 않습니다. 그것은 디자인 시스템과 플랫폼의 몫입니다(FR-14).

#### 19.4 수용 기준

1. 아무 설정 없이 실행한 창이 macOS에서 신호등 버튼을 유지하면서 콘텐츠가 타이틀바 영역까지 올라옵니다. 타이틀 문자열은 표시되지 않습니다.
2. Windows와 Linux에서 시스템 타이틀바가 없고, 최소화·최대화·닫기가 동작하며, 캡션 빈 영역 드래그로 창이 움직이고, 가장자리로 크기 조절이 됩니다.
3. `TopAppBar`가 있는 앱에서 그 내용이 macOS 신호등 버튼과 겹치지 않습니다.
4. `TopAppBar`가 없는 앱의 콘텐츠가 캡션 영역에 가려지지 않습니다.
5. `Chrome::System`으로 실행하면 플랫폼 기본 타이틀바가 그대로 나옵니다.
6. 창 버튼이 접근성 트리에 노출됩니다. macOS는 시스템이 제공하므로 자동이고, Windows와 Linux는 우리가 그리므로 역할과 레이블을 직접 붙여야 합니다.

### FR-20 반응형 레이아웃 기반 (창 크기 클래스) (`Done`)

UI는 Rust가 저작하지만 측정은 Kotlin이 합니다. 그래서 지금은 Host 컴포넌트가 "폰에서는 한 열, 태블릿에서는 두 열, 데스크톱에서는 사이드바"라고 말할 방법이 전혀 없습니다. 브레이크포인트도, 크기 클래스도, 폭을 알아낼 경로도 없습니다. 폰/태블릿/데스크톱 적응은 그 위에 얹는 것이고, 이 요구사항은 그 바닥을 만듭니다.

#### 20.1 크기 클래스

Material 3 window size class를 그대로 따릅니다. 경계는 폭 기준 600dp와 840dp입니다.

| 클래스 | 폭 | 대표 기기 |
|---|---|---|
| `Compact` | < 600dp | 세로 방향 폰 |
| `Medium` | 600dp 이상 840dp 미만 | 가로 폰, 세로 태블릿, 작은 창 |
| `Expanded` | 840dp 이상 | 태블릿 가로, 데스크톱 창 |

독자적인 경계를 만들지 않는 이유는 이 숫자들이 이미 Compose, Android, 그리고 이 프로젝트가 기본으로 삼는 디자인 시스템의 가이드라인과 같은 값이기 때문입니다. 다른 값을 쓰면 Renderer 쪽 Compose 컴포넌트의 적응 동작과 Host 쪽 분기가 서로 다른 지점에서 바뀌고, 그 어긋남은 화면에서 바로 보입니다. 높이 기준 클래스(480dp/900dp)는 지금 쓰는 곳이 없으므로 도입하지 않고, 높이는 dp 값으로만 전달합니다.

경계는 **폭만** 기준으로 정합니다. Host는 높이 dp도 함께 받지만 클래스 결정에는 쓰이지 않습니다.

#### 20.2 범위: 창 단위만

크기 클래스는 **창 하나에 하나**입니다. 루트 콘텐츠가 측정된 크기가 곧 그 창의 크기입니다.

컨테이너 단위(임의의 노드가 자기 폭을 아는 것)는 지금 만들지 않습니다. 그것은 노드마다 측정 구독을 붙이고 레이아웃 결과를 Host로 올리는 일이라, 노드 수만큼의 이벤트 경로와 레이아웃 패스마다의 비교 비용이 생깁니다. 필요해지면 **후속 확장**으로 별도 요구사항을 세우고, 이 이벤트의 `node_id` 자리를 그대로 써서 특정 노드의 크기를 싣는 형태로 넓힙니다. 지금은 루트를 뜻하는 `node_id = 0`만 보냅니다.

#### 20.3 와이어 형태

Renderer에서 Host로 올라가는 **이벤트 레코드 한 종류**를 추가합니다. 두 번째 전달 수단을 만들지 않습니다.

| 태그 | 이벤트 | 페이로드 |
|---|---|---|
| 17 | `WindowSizeChanged` | width_dp (f32), height_dp (f32), size_class (u32) |

- 태그 8-15는 FR-18.1이 제스처로 예약했고 16은 `ValueChanged`이므로, 이어지는 첫 자리인 17을 씁니다.
- 레코드는 고정 28바이트입니다. 공통 머리 16바이트(tag, record_len, node_id, handler_id) 뒤에 width_dp, height_dp, size_class가 각각 4바이트입니다. 가변 부분도, 문자열 참조도 없습니다.
- `node_id = 0`(루트), `handler_id = 0`입니다. 이 이벤트는 핸들러가 소유하지 않습니다. Host는 핸들러 조회 없이 처리하고, 배치를 돌려줍니다.
- `size_class`는 `Compact=0, Medium=1, Expanded=2`인 닫힌 열거형이고 스키마 해시에 들어갑니다. 모르는 값은 `ProtocolError`입니다.
- dp 단위입니다. 픽셀도, 화면 좌표도 보내지 않습니다. 밀도 변환은 Renderer가 합니다(FR-18의 좌표 규칙과 같습니다).

#### 20.4 보내는 시점

Renderer는 **클래스가 실제로 바뀔 때만** 보냅니다.

- 루트 콘텐츠가 측정되는 곳에서 폭을 읽고, 직전에 보낸 클래스와 비교합니다. 같으면 아무것도 하지 않습니다.
- 창을 잡고 끄는 동안 레이아웃 패스는 초당 수십 번 일어납니다. 패스마다 이벤트를 보내면 그만큼 VirtualDom을 돌게 되고, 이는 NFR-9 위반입니다.
- **양쪽의 시작 상태는 `Compact`입니다.** Host는 아무것도 받기 전에 `Compact`를 들고 있고, Renderer도 마지막으로 보낸 클래스를 `Compact`로 두고 시작합니다. 그래서 폰 크기로 열린 창은 첫 측정에서도 아무것도 보내지 않습니다. 이미 서로 같은 값을 알고 있는데 그것을 한 번 더 보내는 것은 정의상 불필요한 경계 호출이고, 그 호출은 VirtualDom을 한 번 돌립니다.
- 창이 그보다 넓게 열리면 첫 측정에서 한 번 보냅니다. Host가 실제 크기를 알게 되는 시점은 그때입니다.
- 새 Host는 `Compact` 기본값에서 시작합니다. 한 프로세스에서 Host를 다시 만들어도 두 시작 상태가 어긋나지 않습니다.
- 클래스가 같고 dp만 바뀐 경우는 보내지 않습니다. 따라서 Host가 보는 dp 값은 **마지막 클래스 전환 시점의 값**입니다. 픽셀 단위로 따라오는 dp가 필요한 레이아웃은 Compose 쪽에서 하는 일이고, 그것을 Host로 올리면 프레임마다 VirtualDom을 도는 것과 같습니다.

#### 20.5 비용

- **추가 지연 프레임 없음**: 이벤트는 기존 동기 dispatch 경로를 그대로 타고, 그 호출이 돌려준 배치가 같은 프레임에 적용됩니다. 큐도, 프레임 지연도 없습니다.
- **정상 상태 할당 없음**: Host 쪽 구독 목록은 컴포넌트가 훅을 처음 부를 때만 자라고, 이벤트 처리 경로는 할당하지 않습니다. Renderer 쪽 비교는 마지막 클래스 하나를 들고 있는 것이 전부이며, 레이아웃 패스마다 객체를 만들지 않습니다.

#### 20.6 Host API

```rust
let window = use_window_size();
if window.class == WindowSizeClass::Expanded { /* 사이드바 */ }
```

- `use_window_size()`는 Dioxus 훅 관례를 따릅니다(PR-7). 현재 클래스와 dp 값을 담은 `WindowSize`를 돌려주고, 클래스가 바뀌면 그 훅을 부른 컴포넌트만 다시 렌더링합니다.
- 구독은 훅 상태의 수명을 따릅니다. 컴포넌트가 사라지면 구독도 사라집니다.
- dp만 바뀌고 클래스가 그대로면 아무 컴포넌트도 다시 렌더링되지 않습니다(20.4에 따라 이벤트 자체가 오지 않습니다).

#### 20.7 수용 기준

1. 599dp에서 601dp로 창을 넓히면 `WindowSizeChanged` 이벤트가 **정확히 한 번** 발생합니다.
2. `Compact` 크기로 열린 창은 첫 측정에서 이벤트를 보내지 않습니다.
3. 300dp에서 599dp까지 같은 클래스 안에서 크기를 바꾸는 동안 이벤트가 **한 번도** 발생하지 않습니다.
4. 28바이트 고정 레코드가 Rust와 Kotlin 양쪽에서 같은 바이트로 인코딩되고 디코딩됩니다(프로토콜 벡터).
5. `use_window_size()`를 쓴 컴포넌트가 클래스 전환에서 다시 렌더링되고, 그렇지 않은 형제는 다시 렌더링되지 않습니다.
6. 알 수 없는 `size_class` 값은 `ProtocolError`가 되고 프로세스가 죽지 않습니다.

### FR-21 탐색, 시트, 일시 메시지 (`Agreed`)

FR-15.2의 코어 어휘 29개로는 애플리케이션이 세 가지를 말할 수 없습니다. 화면을 목적지로 나누는 것(탐색), 화면 위에 임시로 무언가를 덮는 것(시트), 그리고 "했습니다"라고 알리는 것(일시 메시지)입니다. 셋 다 없어서는 안 되는 부류이고, 셋 다 FR-15.3에서 한 번 제외됐거나 아예 논의되지 않았습니다.

특히 탐색은 FR-20이 만든 창 크기 클래스가 존재하는 이유 그 자체입니다. FR-20은 Host에게 `use_window_size()`를 줬지만, 그 값으로 만들 수 있는 것을 하나도 주지 않았습니다.

#### 21.1 FR-15.3의 제외를 되짚습니다

FR-15.3은 `NavigationBar`를 "모바일 고유. Android와 iOS에만 있고 데스크톱 세 시스템에 공통 개념이 없습니다"라며, `BottomSheet`를 "Material 고유 어휘"라며 제외했습니다. 두 판단 모두 **위젯 이름을 기준으로 삼았기 때문에** 틀렸습니다.

공통인지 따져야 할 대상은 `NavigationBar`라는 컴포넌트가 아니라 **"목적지 집합 중 하나가 선택되어 있다"는 개념**입니다. 그 개념은 여섯 시스템 모두에 있습니다.

| 시스템 | 좁은 창 | 중간 창 | 넓은 창 |
|---|---|---|---|
| Material 3 | `NavigationBar` | `NavigationRail` | `PermanentNavigationDrawer` |
| Apple | 탭 바(iOS) | 탭 바 또는 사이드바 | 사이드바(`NSSplitViewController`) |
| Fluent 2 | `NavigationView` minimal | `NavigationView` compact | `NavigationView` expanded |
| GNOME | `AdwViewSwitcherBar` | `AdwNavigationSplitView` 접힘 | `AdwNavigationSplitView` 펼침 |
| KDE Breeze | Kirigami 하단 액션 | 좁은 사이드바 | `KPageView` 사이드바 |
| Deepin | 하단 탭 | 사이드바 | 사이드바 |

Fluent의 `NavigationView`가 결정적입니다. 한 컴포넌트가 폭에 따라 세 가지 표시 모드를 스스로 고릅니다. "데스크톱에 공통 개념이 없다"가 아니라, 데스크톱 시스템들은 **이미 그것을 폭에 따라 달라지는 하나의 개념으로 취급하고 있었습니다.**

시트도 같습니다. Material의 bottom sheet, Apple의 `UISheetPresentationController`, Fluent의 `Sheet`/`TeachingTip`, GNOME의 `AdwBottomSheet`, Kirigami의 `OverlayDrawer`. 화면 가장자리에서 밀려 들어오는 임시 표면은 여섯 시스템 모두의 어휘입니다.

따라서 FR-15.3의 해당 두 줄을 철회하고, 여기서 코어 어휘로 편입합니다. 나머지 제외 항목(`Chip`, `FAB`, `Grid`, `RichText`)은 그대로 둡니다.

#### 21.2 탐색은 위젯 셋이 아니라 개념 하나입니다

**Host는 `Navigation` 하나만 선언하고, 막대/레일/서랍 중 무엇으로 그릴지는 Renderer가 고릅니다.**

대안은 Host가 고르는 것, 즉 `use_window_size()`를 읽어 `NavigationBar`, `NavigationRail`, `Drawer` 세 위젯 중 하나를 `rsx!`에서 분기하는 방식입니다. 채택하지 않는 이유는 셋입니다.

1. **Renderer가 이미 폭을 압니다.** Host가 고르려면 폭이 경계를 한 번 올라갔다가 그 결과로 만들어진 트리가 다시 내려와야 합니다. 클래스가 바뀔 때마다 VirtualDom 전체가 돌고, 목적지 노드들이 전부 지워졌다가 다시 만들어집니다. Renderer가 고르면 같은 노드가 배치만 바꿉니다.
2. **경계가 좁아야 합니다(PR-2).** 세 위젯은 위젯 태그 셋, 속성 셋, Renderer 분기 셋입니다. 하나면 전부 하나입니다.
3. **이것이 이 프로젝트의 주장 그 자체입니다.** "선언 하나가 폰에서도 데스크톱에서도 그 플랫폼답게 나온다." 그 주장을 가장 잘 보여줄 자리에서 Host에게 분기를 시키면, 주장은 남지만 근거가 사라집니다.

세 표현 사이의 대응은 **디자인 시스템의 규칙**이지 고정값이 아닙니다(FR-14.1). Renderer의 `ComponentRules`가 크기 클래스를 받아 표현을 답합니다. 기본 대응은 `Compact→막대`, `Medium→레일`, `Expanded→서랍`이고, 어떤 시스템이 다르게 답해도 위젯 코드는 바뀌지 않습니다.

**목적지는 데이터여야 합니다.** `Tabs`처럼 자식을 그대로 그리게 하면 안 됩니다. `Column { Icon, Text }`를 자식으로 받으면 Host가 이미 "아이콘 위에 라벨"이라는 배치를 정해 버린 것이고, 레일이 라벨을 작게 줄이거나 서랍이 아이콘 옆에 라벨을 놓는 결정을 Renderer가 할 수 없게 됩니다. 그래서 목적지는 라벨과 아이콘을 **속성으로** 싣는 전용 위젯 `NavigationItem`입니다.

- **자식 규약**: `Navigation`의 자식 중 `NavigationItem`인 것이 목적지이고, 나머지 자식이 그 화면의 내용입니다. 인덱스로 나누지 않는 이유는, 목적지 수가 조건에 따라 달라지는 순간 Host가 두 목록의 순서를 손으로 맞춰야 하기 때문입니다.
- **선택은 Renderer의 상태입니다(D5).** `SelectedIndex`(태그 42)가 초깃값과 바깥에서 들어온 변경을 주고, 목적지를 고르면 Renderer가 선택을 옮긴 뒤 그 목적지 자신의 `OnClick` 핸들러로 `Clicked`를 보냅니다. `Tabs`와 같은 규약이고 새 이벤트 태그가 없습니다.
- **서랍이 열려 있는지, 레일이 접혀 있는지는 Renderer의 상태입니다.** Host 속성이 없습니다. 창이 넓으면 서랍은 항상 보이고, 그 사실은 Host가 알 필요가 없습니다.

#### 21.3 시트

`Sheet`는 `Dialog`와 같은 오버레이이고, 같은 두 속성을 씁니다: `Open`(태그 40)이 Renderer의 열림 상태를 seed하고, `OnDismiss`(태그 41)가 사용자가 닫으려 했다는 사실을 한 번 알립니다. 자식이 내용입니다.

- **끌린 거리, 스냅 위치, 애니메이션은 경계를 넘지 않습니다(D5).** 사용자가 시트를 절반쯤 끌어내리는 동안 Rust는 아무것도 듣지 않습니다. 끝까지 내려가서 닫히면 그때 `OnDismiss` 한 번입니다.
- **어느 가장자리에서 들어오는지는 Renderer가 정합니다.** 좁은 창에서는 아래에서, 넓은 창에서는 옆에서 들어오는 것이 여섯 시스템의 공통 관행입니다. 탐색과 같은 이유로 이 선택도 `ComponentRules`가 크기 클래스를 받아 답합니다. Host에 `bottom: bool` 같은 속성을 두지 않습니다.

#### 21.4 일시 메시지는 노드가 아닙니다

Snackbar를 위젯 태그로 만들면 Host가 **자기 소멸 타이머를 모델링해야 합니다.** "지금부터 4초 동안 열림"이라는 상태를 Rust가 들고, 4초 뒤에 워커가 신호를 보내 프레임을 돌려서 노드를 지워야 합니다. 4초짜리 애니메이션을 위해 VirtualDom이 도는 셈이고, D5가 스크롤 위치와 IME 조합을 Renderer에 둔 것과 정확히 같은 이유로 틀렸습니다.

메시지는 **수명을 가진 사건**이지 트리의 일부가 아닙니다. 그래서 새 **Mutation 태그 12 `ShowMessage`** 하나를 더합니다. Host는 "이 말을 전하라"고 한 번 말하고 잊습니다.

```
tag 12, 32바이트: handler_id: u64, text: (offset, len), action: (offset, len), duration: u16, reserved: u16
```

- `handler_id`는 동작 라벨을 눌렀을 때 보낼 핸들러 id입니다. 동작이 없으면 0이고, 그때 `action`은 빈 문자열입니다. 누르면 기존 `Clicked`(이벤트 태그 1)를 `node_id = 0`으로 보냅니다. 메시지에는 노드가 없기 때문입니다.
- `duration`은 `Short=1`, `Long=2`인 닫힌 열거형입니다. 실제 밀리초는 디자인 시스템이 정합니다. 반드시 확인을 받아야 하는 메시지는 일시 메시지가 아니라 `Dialog`이므로 "무기한"은 두지 않습니다.
- **연달아 온 메시지는 줄을 섭니다.** 한 번에 하나만 보이고, 보이는 것이 수명을 다하면 다음 것이 나타납니다. 나중 것이 앞의 것을 지우면 사용자가 방금 한 일에 대한 응답을 못 보게 되고, 둘을 겹쳐 쌓으면 화면이 가려집니다. 줄은 유한하며(8개) 넘치면 **가장 오래 기다린 것부터 버립니다.** 밀려 버려지는 쪽이 이미 사용자가 잊은 조작에 대한 응답이기 때문입니다.
- 큐도, 스레드 홉도 아닙니다. `ShowMessage`는 다른 모든 레코드와 같은 배치를 타고 같은 호출 안에서 적용됩니다(PR-1).

#### 21.5 와이어 추가분

위젯 태그(FR-15.2의 29 뒤에 덧붙입니다):

| 태그 | 위젯 | 여섯 시스템 대응 |
|---|---|---|
| 30 | `Navigation` | 21.1의 표. 표현은 Renderer가 고릅니다 |
| 31 | `NavigationItem` | 목적지 하나. 라벨과 아이콘을 속성으로 싣습니다 |
| 32 | `Sheet` | 가장자리에서 들어오는 임시 표면 |

속성 태그: 위젯 태그 30-32의 블록은 **60-69**입니다(FR-15.2.2의 다음 블록).

| 태그 | 속성 | 값 |
|---|---|---|
| 60 | `Icon` | `IconRole` 태그(u32). 0은 "보내지 않음" |

나머지는 전부 기존 태그를 씁니다. `Navigation`은 `SelectedIndex`(42), `NavigationItem`은 `Text`(1), `Enabled`(3), `OnClick`(5), `Sheet`는 `Open`(40)과 `OnDismiss`(41)입니다.

`IconRole`에 탐색이 필요로 하는 의미 셋을 덧붙입니다: `Home=9`, `List=10`, `Inbox=11`. 기존 8개와 같은 규칙으로, Host는 의미만 보내고 그림은 Renderer가 시스템별로 그립니다.

#### 21.6 디자인 시스템은 하나씩 늘어나야 합니다

세 부류 모두 `ComponentRules`에 답을 하나씩 더합니다: `navigation(WindowSizeClass)`, `sheet(WindowSizeClass)`, `message()`.

**셋 다 기본 구현을 가집니다.** 기본값은 그 시스템의 토큰 표(색, 모서리, 간격)에서 유도되므로, 아무것도 재정의하지 않은 시스템도 남의 색이 아니라 자기 색으로 나옵니다. 재정의하지 않으면 안 되는 것은 없고, 시스템은 의견이 있는 것만 답합니다. 일곱 번째 디자인 시스템이 들어올 때 이 세 멤버 때문에 컴파일이 깨지지 않는 것이 요점입니다(FR-14.2).

#### 21.7 수용 기준

1. 같은 `Navigation` 선언이 폭 500dp에서 하단 막대로, 700dp에서 레일로, 1100dp에서 서랍으로 그려집니다. 세 폭의 스크린샷으로 확인합니다.
2. 창을 넓혀 표현이 바뀌는 동안 `Navigation`의 자식 노드가 새로 만들어지지 않습니다(`Create` 레코드가 나오지 않습니다).
3. 목적지를 고르면 그 목적지의 `OnClick` 핸들러로 `Clicked` 이벤트가 **정확히 한 번** 갑니다. 선택 표시는 Host 배치 없이 바뀝니다.
4. `Sheet`를 끌어 닫으면 `OnDismiss`가 한 번 가고, 끄는 동안에는 아무 이벤트도 가지 않습니다.
5. `ShowMessage` 두 개가 한 배치에 오면 첫 번째만 보이고, 그 수명이 끝난 뒤에 두 번째가 보입니다.
6. 메시지의 동작 라벨을 누르면 `node_id = 0`으로 `Clicked`가 갑니다.
7. 32바이트 `ShowMessage` 레코드가 Rust와 Kotlin 양쪽에서 같은 바이트로 인코딩되고 디코딩됩니다(PR-4 벡터).
8. 세 위젯이 여섯 디자인 시스템 전부에서 렌더링되고, 새 디자인 시스템은 `ComponentRules` 구현 하나로 끝납니다.

#### 21.8 지금까지 확인된 것

1번부터 7번까지 통과했습니다. 1번은 `fr21_one_declaration_is_a_bar_a_rail_and_a_drawer`가 500dp/700dp/1100dp에서 막대/레일/서랍의 치수를 재고, 같은 폭에서 찍은 스크린샷 아홉 장(디자인 시스템 3 × 폭 3)을 눈으로 확인했습니다. 2번은 `fr21_navigation_is_one_declaration_and_a_resize_creates_nothing`, 3번은 Host와 Renderer 양쪽, 4번은 `fr21_dragging_a_sheet_shut_reports_one_dismissal_and_nothing_during_the_drag`, 5번은 `fr21_two_messages_in_one_batch_are_shown_one_at_a_time_in_order`, 6번은 양쪽, 7번은 갱신된 `mutations.bin` 벡터와 `fr21_the_message_record_in_the_vector_decodes_to_the_same_values`입니다.

**8번은 아직입니다.** Renderer의 `ComponentRules` 구현은 현재 셋(Material 3, Cupertino, Fluent)이고, GNOME/Breeze/Deepin은 다른 작업에서 들어오는 중입니다. 세 멤버 전부 토큰 표에서 유도한 기본 구현을 가지므로 그 셋이 합류할 때 컴파일이 깨지지 않고 각자의 색으로 나오지만, "여섯 시스템 전부"는 그 작업이 합쳐진 뒤에 확인해야 합니다. 그때까지 이 요구사항은 `Done`이 아닙니다.


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

### PR-2 경계 표면 (`Agreed`)
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

**검증 (2026-09-21, macOS arm64 + iOS 시뮬레이터)**

데스크톱용 C 스모크 호스트(`desktop/c/smoke_host.c`)를 **한 글자도 고치지 않고** iOS 정적 아카이브에 링크해 실행했습니다. 같은 `main`, 같은 다섯 개 `dioxus_compose_host_*` 함수가 GraalVM native-image 공유 라이브러리와 Kotlin/Native 아카이브 양쪽에 그대로 붙습니다. 경계 표면이 두 런타임에서 하나라는 근거입니다.

```
dioxus_compose_host_init: 180 bytes, 8 records
dioxus_compose_host_dispatch_event: click 1
```

- 핸드셰이크와 초기 배치(180바이트, 8레코드)가 양쪽에서 바이트 단위로 동일합니다.
- 시뮬레이터에서 버튼을 탭하면 `dispatch_event`가 Rust 핸들러까지 도달합니다. `simctl`에 탭 명령이 없어 이 확인은 자동화되지 않습니다. `ios-smoke-test.sh --await-click`으로 사람이 실행합니다. CI는 이 플래그 없이 기동과 렌더링까지만 증명합니다.
- iOS에는 isolate가 없어 `@CName`이 공개 심볼을 Kotlin 함수에 직접 붙입니다. isolate 심이 하던 나머지 역할은 Kotlin/Native 런타임과 `NSThread.isMainThread` 검사가 대신합니다.
- Android와 Web은 아직 검증되지 않았습니다. 네 타깃이 모두 확인되면 `Done`으로 올립니다.

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
- **이벤트 태그** (Renderer→Host): 입력 이벤트(FR-3), `TextChanged`/`TextSubmitted`/`FocusLost`(FR-5), `RangeRequested`(FR-8), `Lifecycle(Start|Stop)`, `Resync`, `SaveState`/`RestoreState`(선택), `ProtocolError`
- **플랫폼별 메모리 접근**
  - Desktop(GraalVM): `Pointer`로 직접 읽습니다.
  - iOS(Kotlin/Native): `CPointer`로 읽습니다.
  - Android: `NewDirectByteBuffer`로 arena를 감싸서 읽습니다.
  - Web: 공유 linear memory(PR-6)
- 수용 기준: 텍스트 하나를 바꾸는 이벤트 처리에서 경계 호출 2회(`dispatch_event`, `release_batch`), 힙 할당은 Compose `String` 생성 1회 이하

### PR-5 Android (`Agreed`)
- 호스트 관계: Kotlin Activity가 프로세스와 루프를 소유합니다(`LoopMode::Platform`). Rust는 cdylib입니다. VirtualDom은 PR-3에 따라 UI 스레드에서 돕니다.
- JNI 심: PR-2의 논리 연산에서 jni-rs 기반 심과 Kotlin `external fun` 선언을 코드젠으로 생성합니다. UniFFI(JNA 경유)는 호출당 오버헤드가 커서 쓰지 않습니다.
- 생명주기:
  - Surface 파괴와 config change: Compose만 재구성됩니다. VirtualDom도 노드 테이블도 프로세스에 남으므로, 다시 만들어진 Activity는 같은 테이블을 그대로 그립니다. 경계 호출도 상태 손실도 없고, `Resync`는 이 자리에서 보내지 않습니다.
  - `Resync`는 노드 테이블을 지킬 수 없는 Renderer를 위한 것입니다. Host는 이미 보낸 트리의 사본을 들고 있지 않아서 애플리케이션을 처음부터 다시 만들어 답하며, 그래서 돌아오는 배치의 노드 id는 1부터 다시 시작하고 컴포넌트 상태는 사라집니다. 받는 쪽은 그 배치가 오기 전에 기존 트리와 에셋과 메시지를 먼저 버려야 합니다. 지킬 수 있는 테이블은 지키는 쪽이 언제나 낫습니다.
  - `onStop`/`onStart`: `LifecycleStop`과 `LifecycleStart`를 보냅니다. Host는 그 사이에 도착한 워커의 프레임 요청을 기억만 해 두었다가 시작할 때 한 번 내보내고, 그동안 타이머와 애니메이션은 억제됩니다. `Resumed`/`Paused`/`Destroyed`로 나누지 않습니다. 그리는 일이 실제로 멈추는 경계는 `onStop`이고, `Destroyed`는 프로세스가 사라지는 자리라 Host가 들을 수 없습니다.
  - 프로세스 kill: 메모리 상태는 복원하지 않습니다. 필요하면 `SaveState`로 작은 blob을 `onSaveInstanceState`에 저장합니다.
- 에셋: Android는 자기 그래픽 스택으로 그리므로 SVG 파서가 없습니다. `Svg` 종류의 등록은 FR-16이 정한 대로 읽을 수 없다고 보고하고, 나머지 배치는 그대로 적용됩니다. `VectorIcon`은 영향이 없습니다. 모양을 그리는 것은 디자인 시스템이기 때문입니다.
- android-activity, NativeActivity, GameActivity 진입점은 쓰지 않습니다. ComposeView와 공존한 사례가 없고 IME 충돌 위험이 있습니다. JavaVM은 `JNI_OnLoad`에서 얻습니다.

#### 5.1 Kotlin이 앱 빌드에 들어가는 방법 (INTENT D11)

데스크톱과 iOS는 Kotlin을 AOT로 굳혀 바이너리 하나로 내보내지만, Android는 ART라 그럴 수 없습니다. Kotlin이 사용자 앱의 Gradle 빌드에 참여해야 하고, 그 경로를 dx가 정해 둡니다.

**크레이트가 Kotlin 소스를 품고, 빌드 스크립트가 Gradle 프로젝트의 `src/main/kotlin`에 풀어놓습니다.**

- dx의 Gradle 템플릿은 `gradle_dependencies` 항목을 `implementation("...")`로 감싸므로 **Maven 좌표만** 받습니다. 크레이트에 AAR을 넣어도 Gradle에 알릴 방법이 없어서, 컴파일된 아티팩트가 아니라 소스를 배포합니다.
- 같은 템플릿이 `sourceSets { main { java.srcDirs("src/main/kotlin", ...) } }`를 선언하므로, 거기 놓인 `.kt`는 사용자 앱과 함께 컴파일됩니다. wry가 Kotlin을 생성해 넣는 것과 같은 경로입니다.
- **Compose와 androidx는 좌표로 선언합니다.** 남의 라이브러리이고 mavenCentral에 이미 있으므로 우리가 배포하지 않습니다.
- **MainActivity는 생성합니다.** dx 템플릿은 패키지를 `dev.dioxus.main`으로 고정하고 앱 id는 `BuildConfig` 별칭에만 씁니다. 정적 파일로 주면 그 치환을 못 받으므로, 빌드 스크립트가 앱 id를 받아 만들어냅니다. `WryActivity` 대신 `ComponentActivity`를 상속하고 `setContent`로 렌더러를 띄웁니다.
- 수용 기준(M6)에 추가합니다: **4. 사용자가 `Dioxus.toml`에 우리 좌표를 적지 않고도 APK가 빌드됩니다.** Kotlin 소스가 자동으로 들어가고 Maven 의존성이 없어야 통과입니다.
- 수용 기준(M6):
  1. 일반 JNI와 `@FastNative`의 호출당 비용을 실측합니다. 공개 수치(약 115ns, 약 35ns)와 비교해 기록합니다.
  2. M0 화면을 같은 Rust 소스로 띄우고, 초당 100회 추가되는 스트리밍 중 프레임 끊김이 없음을 Macrobenchmark `FrameTimingMetric`으로 확인합니다.
  3. 화면 회전, 다크모드 전환, 홈→복귀, `am kill` 후 복귀에서 크래시가 없습니다.
- 구현 상태(2026-09-22): 경계 심 생성, 생명주기와 `Resync`, Activity 호스팅, cdylib 빌드가 들어왔습니다. 심은 `aarch64-linux-android`로 컴파일되고, cdylib이 내보내는 JNI 심벌은 컴파일된 Kotlin 클래스가 native로 선언한 이름과 정확히 일치합니다. 수용 기준 1~3은 모두 기기나 에뮬레이터에서만 확인할 수 있어 아직 미검증이고, 5.1의 수용 기준 4(크레이트가 Kotlin 소스를 품고 Maven 좌표 없이 APK가 빌드되는 것)는 아직 착수 전입니다.

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
| Kotlin 생성 코드 | `generated` 패키지, 파일 이름 접미사 `.gen.kt` | `dioxus.compose.protocol` |
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
| NFR-3 | 데스크톱 무게 | 빈 창 physical footprint < 56MB, 배포 용량 < 100MB. 측정 기준과 근거는 §5.2 | Agreed |
| NFR-4 | 플랫폼 | macOS, Windows, Linux 데스크톱, iOS, Android, Web(wasm). Android와 Web의 경계는 PR-5, PR-6 참조 | Agreed |
| NFR-5 | 개발 경험 | Renderer는 JVM 개발 셸에서 hot reload와 `@Preview`로 작업 가능. native-image 빌드는 개발 루프에 필요 없음. 새 머신의 준비 상태를 `scripts/setup-check.sh` 한 번으로 확인 가능 | Agreed |
| NFR-6 | 안정 API만 사용 | `@InternalComposeUiApi`, `@ExperimentalComposeUiApi` 의존을 금지하거나, 쓰더라도 어댑터 한 파일에 격리하고 버전 핀을 둠 | Agreed |
| NFR-7 | 크래시 격리 | 프로토콜 오류로 프로세스가 종료되지 않고 `ProtocolError` 이벤트를 보냄 | Agreed |
| NFR-8 | 데스크톱 접근성 | native-image 빌드의 접근성 트리가 JVM 개발 셸과 같은 구조로 노출될 것. smoke test의 종료 코드 0은 근거가 되지 않습니다(접근성을 질의하지 않으므로). **2026-09-21 트리 노출 충족**, VoiceOver 수동 확인은 미완료(§7) | Agreed |
| NFR-9 | 네이티브 수준 프레임 성능 | §5.1 기준 충족 | Agreed |
| NFR-10 | 렌더러 탐색 경로 | `DIOXUS_COMPOSE_RENDERER_DIR` → 워크스페이스 빌드 결과물 → 버전·타깃별 캐시 → 릴리스 다운로드 순서로 찾음. 규격과 수용 기준은 §5.3. **2026-09-21 충족** | Done |
| NFR-11 | 배포 | 크레이트는 crates.io, 렌더러는 플랫폼별 체크섬 릴리스 아티팩트. 설치는 `Cargo.toml` 한 줄이 전부이고 빌드 스크립트가 아티팩트를 가져옵니다. 규격과 수용 기준은 §5.3 (INTENT D10). **2026-09-21 macOS에서 충족**, Windows와 Linux는 실행 확인 미완료 | Agreed |
| NFR-12 | 워크트리 빌드 격리 | 워크트리마다 자기 `target/`에 빌드하고, 다른 워크트리의 빌드 디렉터리를 가리키는 설정이 없음. 한 트리에서 컴파일된 codegen 바이너리가 다른 트리에 쓸 수 없음. 규격과 수용 기준은 §5.4 (INTENT D13). **2026-09-22 충족** | Done |

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

**측정 기준은 macOS의 physical footprint입니다.** RSS는 이미지에서 매핑된 깨끗한 페이지까지 세기 때문에 실제 점유량을 과장합니다. 같은 프로세스가 RSS 127MB, footprint 54MB로 두 배 넘게 차이납니다. Activity Monitor의 "메모리" 열이 footprint입니다. 측정은 `desktop/scripts/measure-memory.sh`로 재현합니다.

**현재값 (2026-09-20, macOS 26.5.1, Apple M1, 스모크 테스트 창)**

| 영역 | dirty | 정체 |
|---|---|---|
| `MALLOC_SMALL` | 14.0MB | **Skia의 네이티브 할당**. Java 힙이 아닙니다 |
| 그래픽(Metal, 창 서페이스) | 10.0MB | 창 크기에 비례합니다 |
| `IOSurface` | 7.5~9.4MB | 창 서페이스 |
| `__DATA` dirty | 6.0MB | 이미지 힙의 쓰기 페이지 |
| `IOAccelerator` | 4.9MB | 이 중 4.7MB는 회수 가능 |
| 매핑된 파일 | 4.3MB | |
| `untagged (VM_ALLOCATE)` | 2.5MB | **실제 SubstrateVM 힙** |
| 기타 | 약 3.5MB | malloc 메타데이터, 페이지 테이블, 스택 |
| **합계** | **54~56MB** | |

**목표를 45MB에서 56MB로 고쳤습니다.** 45MB는 측정 전에 네이티브 AppKit 앱(20~25MB)을 보고 잡은 숫자였고, 실측으로 근거가 무너졌습니다.

**측정으로 확인된 사실**

1. **SubstrateVM 힙 상한은 footprint를 바꾸지 않습니다.** 기본값(RAM의 80%), 64MB, 24MB에서 `MALLOC_SMALL`이 모두 정확히 14MB입니다. Serial GC의 적응 정책이 이미 실제 사용량에 맞춰 힙을 잡습니다. 5.2가 이전에 이것을 최대 레버로 지목한 것은 틀린 전제였습니다.
2. **14MB는 Skia의 네이티브 할당입니다.** `-R:` 계열 플래그가 닿지 않습니다. 실제 Java 힙은 2.5MB입니다.
3. **로케일과 도달 가능 코드는 footprint에 영향이 없습니다.** 이미지 코드와 읽기 전용 힙은 `__TEXT`(18MB)와 깨끗한 `__DATA`(8.7MB)에 들어가고, footprint는 dirty 페이지만 셉니다. 이 레버들은 디스크 용량(65MB)을 줄이지 resident를 줄이지 않습니다.
4. **`skiko.buffering=DOUBLE`은 JVM에서는 1.9MB를 줄이지만 네이티브 이미지에서는 동작하지 않습니다.** Skiko의 속성 보관 객체가 이미지 빌드 시점에 초기화돼서, 시작 시점에 설정한 값이 너무 늦게 도착합니다. IOSurface가 9408KB로 동일한 것을 측정으로 확인했습니다.

**남은 여지**: 그래픽이 약 22.5MB로 전체의 40%를 넘고 창 크기에 비례합니다. Skia와 그래픽을 합친 약 36.5MB가 Compose/Skia 프로세스의 바닥이며, 시스템 텍스트 스택을 공유하는 AppKit 앱은 지지 않는 비용입니다.

**더 줄이려면 IME 등록 메타데이터를 건드려야 합니다.** `ImeReachabilityFeature`가 패키지 단위로 등록하면서 약 7.8MB의 이미지 힙을 만듭니다. 이것을 좁히는 것이 남은 유일한 큰 레버지만, 실패하면 입력기가 텍스트 필드를 건드리는 순간 프로세스가 죽습니다(§6). **사람이 네이티브 빌드에서 한글을 직접 입력해 확인할 수 있을 때만 시도합니다.**

### 5.3 렌더러 획득 (NFR-10, NFR-11)

**요구사항: `Cargo.toml`에 `dioxus-compose`를 추가하는 것이 설치의 전부입니다.** 설정할 환경 변수, 손으로 내려받을 파일, 실행할 스크립트가 없습니다. `cargo build`가 렌더러를 확보하고 링크합니다.

#### 5.3.1 아티팩트 규격

| 항목 | 값 |
|---|---|
| 파일 이름 | `dioxus-compose-renderer-v{crate_version}-{target}.tar.gz` |
| 체크섬 파일 | 같은 이름에 `.sha256`. 내용은 `shasum -a 256` 출력(`<64자리 16진수>  <파일 이름>`) |
| 주소 | `https://github.com/DarkPyonix/dioxus-compose/releases/download/v{crate_version}/{파일 이름}` |
| `{target}` | `macos-aarch64`, `windows-x64`, `linux-x64`, `linux-arm64` |
| 내용 | 아티팩트 루트에 플랫폼별 렌더러 디렉터리. Windows는 `bin/`, 나머지는 `lib/` |

`{target}`은 Cargo의 타깃 트리플에서 만들되, 아키텍처 표기가 릴리스와 다릅니다. Cargo의 `x86_64`는 `x64`, Linux의 `aarch64`는 `arm64`입니다. macOS만 두 표기가 같습니다.

#### 5.3.2 탐색 순서

1. `DIOXUS_COMPOSE_RENDERER_DIR`. 설정돼 있으면 언제나 이깁니다. 없으면 실패하고, 다음 단계로 넘어가지 않습니다. 직접 빌드한 렌더러를 가리켜 둔 빌드가 조용히 다운로드로 바뀌면 안 됩니다.
2. 워크스페이스 빌드 결과물. 이 저장소의 체크아웃에만 있습니다.
3. 버전·타깃별 캐시. 있으면 네트워크를 쓰지 않습니다.
4. 릴리스에서 내려받아 캐시에 풉니다.

#### 5.3.3 캐시

- 위치는 `target/` 밖입니다. Unix는 `$HOME/.cache/dioxus-compose/renderer/v{version}/{target}`, Windows는 `%LOCALAPPDATA%\dioxus-compose\renderer\v{version}\{target}`입니다. `DIOXUS_COMPOSE_CACHE_DIR`로 뿌리를 옮길 수 있습니다(선택이며, 설치에 필요하지 않습니다).
- 키는 버전과 타깃입니다. `cargo clean`을 견디고 프로젝트 사이에서 공유됩니다.
- 내려받은 `.tar.gz`와 `.sha256`은 캐시 뿌리의 `downloads/`에 그대로 둡니다. 이 디렉터리가 오프라인 안내가 가리키는 자리이기도 합니다. 여기에 두 파일을 놓으면 다음 빌드가 네트워크 없이 검증하고 풉니다. 다운로드 경로와 손으로 놓는 경로가 같은 코드라서, 안내한 방법이 실제로 동작하는지 테스트가 확인할 수 있습니다.
- 푸는 매 번 체크섬을 검증합니다. 맞지 않으면 풀지 않고, 받은 값과 기대값을 모두 출력합니다.
- 푸는 작업은 임시 디렉터리에서 하고 마지막에 이름만 바꿉니다. 중간에 끊긴 빌드가 반쯤 풀린 디렉터리를 남기면 다음 빌드가 그것을 캐시 적중으로 읽습니다.

#### 5.3.4 링크된 바이너리가 실제로 실행될 것

크레이트를 의존성으로 추가한 애플리케이션이 링크에 성공하고 실행에 실패하면 설치가 끝난 것이 아닙니다. Cargo는 의존성 빌드 스크립트의 링크 탐색 경로와 링크 라이브러리는 최종 바이너리까지 넘기지만 **링크 인자는 넘기지 않습니다.** rpath가 링크 인자이므로 소비자 바이너리에는 rpath가 없습니다. 그래서 다음 두 가지를 크레이트 안에서 해결합니다(INTENT D12).

- **라이브러리는 자기가 놓인 절대 경로를 자기 이름으로 답니다.** 빌드 스크립트가 렌더러를 확보한 직후, 그 라이브러리가 지금 있는 절대 경로를 라이브러리 자신에게 새깁니다. 소비자 바이너리는 그 절대 경로를 기록하고 로더는 rpath 없이 찾습니다. `DIOXUS_COMPOSE_RENDERER_DIR`, 워크스페이스 빌드 결과물, 캐시, 다운로드 네 경로 모두에 적용하며, 이미 그 이름이면 아무것도 하지 않습니다.
- **`dioxus_compose_host_*` 심벌의 보존.** 렌더러는 적재된 뒤 이름으로 이 심벌들을 찾습니다. 애플리케이션 코드는 그 이름을 한 번도 쓰지 않으므로 링커가 죽은 코드로 판단해 지웁니다. 지워진 바이너리는 실행 첫 순간에 심벌을 못 찾고 죽습니다. 주소를 담은 `#[used]` static이 죽은 코드 제거의 뿌리 역할을 합니다.

**이 크레이트의 빌드 스크립트는 rpath를 내보내지 않습니다.** 내보내면 이 저장소의 테스트 바이너리와 예제만 rpath로 살아나고, 이름 새기기가 고장나도 소비자 쪽에서만 드러납니다. rpath가 없으면 우리 바이너리가 소비자 바이너리와 같은 방식으로 적재되므로 같은 고장을 같은 자리에서 봅니다. 같은 이유로 `samples/`에는 `build.rs`가 없습니다.

##### 플랫폼별 수단

| 플랫폼 | 파일 안의 이름 | 빌드 스크립트가 하는 일 |
|---|---|---|
| macOS | `LC_ID_DYLIB`. 아티팩트는 `@rpath/libdioxus_compose_renderer.dylib`로 나옵니다 | `install_name_tool -id <절대 경로>`. Xcode 명령줄 도구에 들어 있고 Rust 링커가 어차피 필요로 합니다 |
| Linux | `DT_SONAME`. 아티팩트에는 없습니다 | SONAME이 없으면 링커가 연 경로를 그대로 `DT_NEEDED`에 적으므로 할 일이 없습니다. 있으면 푸는 시점에 `.dynamic`에서 그 항목을 들어냅니다. patchelf는 쓰지 않습니다 |
| Windows | 없습니다. DLL은 로더의 탐색 경로로만 찾습니다 | 렌더러 디렉터리를 이름으로 말합니다. 소비자가 그 디렉터리를 `PATH`에 넣거나 안의 파일을 실행 파일 옆에 두어야 합니다 |

Linux 공유 객체에는 `$ORIGIN` 런패스가 붙어 있어 심이 옆에 있는 이미지를 찾습니다. macOS 렌더러는 자기 위치를 `dladdr`로 알아내 옆의 Skia와 AWT 라이브러리를 찾습니다. 그래서 위의 절대 경로는 렌더러가 실제로 풀린 자리여야 하고, 그 아래 배치가 `<루트>/lib/`로 유지되어야 합니다. 라이브러리 파일 하나만 다른 곳에 복사해 두고 그 경로를 이름으로 박으면 링크와 적재는 되지만 첫 프레임에서 Skia를 찾지 못합니다.

**Windows의 자동 해결은 아직 남아 있습니다.** 없애려면 빌드 스크립트가 `bin/`을 실행 파일 옆으로 복사해야 하고, 그러면 프로파일마다 수십 MB가 복사됩니다. **Windows 기계에서 확인할 수 있을 때 결정합니다.** 확인 없이 복사부터 넣으면 검증되지 않은 수십 MB짜리 동작이 모든 빌드에 들어갑니다. 그때까지는 빌드 스크립트가 할 일을 이름으로 말하는 것이 해결입니다.

##### 절대 경로가 지나가는 세 단계

절대 경로는 빌드한 기계의 사실이지 출하물의 사실이 아닙니다. 세 단계의 계약이 각각 다릅니다.

| 단계 | 누가 | 라이브러리 이름 | 지켜야 할 것 |
|---|---|---|---|
| 1. 우리가 배포 | 릴리스 아티팩트를 만드는 쪽 | 상대 이름. macOS는 `@rpath/libdioxus_compose_renderer.dylib`, Linux는 SONAME 없음 | 아티팩트는 어디에 풀어도 되어야 합니다. 절대 경로를 박아 배포하면 그 경로가 있는 기계에서만 동작합니다 |
| 2. 소비자의 빌드 | `dioxus-compose`의 빌드 스크립트 | 그 기계에서의 절대 경로 | 이름을 바꾸는 대상은 소비자의 캐시나 작업 트리 안에 있는 사본입니다. 원본 아티팩트(`.tar.gz`)는 건드리지 않으므로 1단계는 그대로 유지됩니다 |
| 3. 소비자가 앱을 출하 | 애플리케이션의 번들러 | 다시 상대 이름. macOS는 `@executable_path/../Frameworks/lib/...`, Linux는 `$ORIGIN/lib` 런패스와 짧은 `DT_NEEDED` | 2단계가 실행 파일에 남긴 절대 경로를 반드시 지워야 합니다. 지우지 않으면 개발자의 홈 디렉터리 경로가 출하된 바이너리에 남고, 다른 기계에서는 적재에 실패합니다 |

3단계에서 렌더러를 어디에 복사하는지는 자유가 아닙니다. 렌더러는 자기가 적재된 디렉터리에서 Skia와 AWT 동반 파일을 찾고, AWT는 그 디렉터리의 부모에 `lib`을 붙인 자리에서 자기 파일을 읽습니다. macOS에서 확인한 결과, 동작하는 배치는 두 가지입니다. 렌더러 파일을 실행 파일 옆에 두는 것과, 렌더러의 `lib` 디렉터리를 통째로 옮기는 것(깊이는 상관없습니다)입니다. 그 둘 중 어느 쪽도 아닌 자리에 파일만 펼쳐 놓으면 안 됩니다. 파일을 `Contents/Frameworks`에 펼친 앱은 시작해서 렌더러까지 적재한 뒤 `Contents/lib/libjawt.dylib`을 찾다가 죽습니다. `Contents/Frameworks/lib`은 동작합니다.

3단계는 애플리케이션이 정하는 일이며 이 프로젝트의 범위 밖입니다. 범위 안의 책임은 3단계가 가능하도록 남겨 두는 것입니다. 2단계는 되돌릴 수 없는 것을 굽지 않습니다. 실행 파일이 기록한 절대 경로는 `otool -L`(macOS)과 `readelf -d`(Linux)로 그대로 읽히고, macOS는 `install_name_tool -change <절대 경로> <상대 이름>`으로, Linux는 `patchelf --replace-needed`와 `--set-rpath '$ORIGIN/lib'`로 바꿀 수 있습니다. 번들러는 앱 개발자가 한 번 준비하는 환경이므로 patchelf를 요구해도 됩니다. 이 저장소의 `scripts/bundle-renderer.sh`가 그 작업을 하고, 샘플 릴리스 워크플로가 샘플을 묶을 때 실제로 그것을 씁니다.


#### 5.3.5 렌더러가 링크되었는지는 기능 플래그가 아니라 빌드 결과가 정한다

`native-renderer`가 켜져 있다고 렌더러가 링크된 것은 아닙니다. docs.rs는 기능을 켠 채 아무것도 링크하지 않습니다. 빌드 스크립트는 렌더러를 실제로 링크했을 때만 `renderer_linked` cfg를 내보내고, Host는 그 cfg로 외부 심벌을 부를지 시끄럽게 실패할지 정합니다. 기능 플래그로 정하면 docs.rs가 만든 바이너리가 있지도 않은 심벌을 부르게 됩니다.

데스크톱이 아닌 타깃은 Cargo가 렌더러를 링크하지 않지만 심벌은 실행 시점에 존재합니다(iOS는 Xcode가 XCFramework를, 웹은 Kotlin/Wasm 모듈이 임포트를 해결합니다). 그쪽도 `renderer_linked`입니다.

#### 5.3.6 실패

| 상황 | 요구되는 동작 |
|---|---|
| 네트워크 없음 | 어떤 파일을 어디에 두면 되는지 이름과 경로로 말합니다. 파일을 그 자리에 두면 다음 빌드가 그것을 씁니다 |
| 해당 타깃의 아티팩트 없음 | 게시된 타깃 목록을 보여줍니다. 404를 그대로 보여주지 않습니다 |
| 체크섬 불일치 | 풀지 않고 실패합니다. 지울 경로를 알려줍니다 |
| `DOCS_RS` 설정됨 | 내려받지 않고, 렌더러를 링크하지 않고, 빌드는 성공합니다 |
| 렌더러 없이 만들어진 바이너리의 실행 | 조용히 성공하지 않습니다. 무엇이 없는지 표준 오류로 말하고 0이 아닌 상태로 끝냅니다 |

#### 5.3.7 수용 기준

1. 캐시가 이미 있으면 네트워크 접근 없이 빌드가 성공합니다.
2. 체크섬이 맞지 않는 아티팩트는 풀리지 않고 빌드가 실패합니다.
3. 게시되지 않은 타깃은 게시된 목록을 이름으로 말합니다.
4. `DIOXUS_COMPOSE_RENDERER_DIR`은 캐시와 다운로드보다 우선합니다.
5. 렌더러도 mock도 없이 앱을 시작하면 성공을 반환하지 않습니다.
6. 빌드 스크립트가 출력하는 어떤 메시지도 저장소에 없는 파일을 실행하라고 안내하지 않습니다.
7. 비어 있는 캐시에서 실제 다운로드가 동작하고, 걸린 시간과 캐시 크기를 기록합니다.
8. 크레이트에 의존하기만 한 애플리케이션이 빌드되고, 실행되고, 창을 엽니다.
9. **`build.rs`가 없는 소비자 크레이트**가 빌드되고, rpath를 하나도 갖지 않고, 실행됩니다. 이것이 이 절의 회귀 기준입니다. `build.rs`를 가진 샘플로는 확인되지 않습니다.
10. 워크스페이스에서 직접 빌드한 렌더러도 같은 대접을 받습니다. 캐시에서 온 렌더러와 개발용 렌더러의 적재 방식이 다르면 안 됩니다.
11. 번들러를 거친 실행 파일에는 빌드한 기계의 절대 경로가 남아 있지 않습니다.

**2026-09-21 1에서 7까지 충족.** 1에서 6은 `dioxus-compose/tests/renderer_resolution.rs`가 매 실행 확인합니다(5는 렌더러가 정말 없는 빌드에서만 컴파일되므로 `scripts/check.sh`가 `--no-default-features`로 한 번 더 돌립니다). 7은 Apple Silicon Mac에서 캐시를 비우고 측정했습니다: **6.3초, 캐시 119MB**(`.tar.gz` 32MB + 푼 것 86MB). 8은 `cargo build -p sample-calculator`로 확인했습니다. 바이너리가 캐시 절대 경로를 기록했고 창이 열렸습니다.

**2026-09-21 9에서 11까지 macOS에서 충족.** 9는 `scripts/tests/consumer-crate.test.sh`가 확인합니다. `dioxus-compose/tests/fixtures/consumer/`에 `build.rs`가 없는 크레이트가 들어 있고, 스크립트가 그것을 빌드해 rpath 개수가 0인지와 실행이 0으로 끝나는지를 봅니다. cargo 테스트가 아니라 셸 스크립트인 이유는 두 가지입니다. 하나는 cargo 안에서 cargo를 부르는 일이라 같은 `target/`을 쓰면 잠금에서 멈추고, 별도 `target/`을 쓰면 의존성 전체를 한 번 더 빌드하기 때문입니다. 다른 하나는 이 확인이 링크된 실행 파일의 적재 명령을 읽는 일이라 확인 대상이 호스트 플랫폼의 도구(`otool`, `readelf`)이기 때문입니다. CI의 `shell` 잡이 `scripts/tests/*.test.sh`를 모두 돌립니다.

10은 `renderer_resolution.rs`가 워크스페이스 경로와 변수 경로 각각에 대해, 진짜 Mach-O 라이브러리를 만들어 이름이 절대 경로로 바뀌는지 확인합니다. 이미 제 자리 이름을 달고 있는 라이브러리는 쓰기 권한을 뗀 채로 통과해야 하므로, 다시 쓰지 않는다는 것도 같은 방식으로 확인됩니다. 11은 같은 스크립트의 후반부가 확인합니다. 만들어진 실행 파일과 렌더러를 임시 디렉터리로 복사해 `scripts/bundle-renderer.sh`로 상대 이름으로 바꾸고, 기록된 경로에 절대 경로가 남지 않았는지 읽은 뒤, 빌드와 아무 상관 없는 디렉터리에서 실행합니다.

이 절에 손으로 확인한 것은 다음과 같습니다. `build.rs` 없는 소비자 크레이트가 `LC_RPATH` 0개로 빌드되어 실행됐고, 샘플 네 개가 모두 같은 모양으로 빌드됐으며 `sample-calculator`가 창을 열었습니다. 번들한 샘플을 `/tmp` 아래 세 가지 배치(실행 파일 옆, `<루트>/lib`, `Contents/Frameworks/lib`)로 실행해 창이 뜨는 것을 확인했고, `Contents/Frameworks`에 펼친 배치가 실패하는 것도 확인했습니다.

Windows와 Linux는 이 저장소에 실행할 기계가 없어 자동 테스트와 코드 검토까지만 확인했습니다. Linux 쪽 판단은 게시된 `.so`의 ELF 헤더를 읽어 SONAME이 없고 런패스가 `$ORIGIN`임을 확인한 것이 근거이고, SONAME을 지우는 편집 자체는 손으로 만든 ELF 픽스처로 `renderer_resolution.rs`가 확인합니다. `scripts/tests/consumer-crate.test.sh`는 두 플랫폼에서 같은 검사를 하도록 쓰여 있으므로, Linux 확인의 다음 단계는 CI의 `shell` 잡이 그것을 돌리는 것입니다.

### 5.4 워크트리 빌드 격리 (NFR-12)

**요구사항: 한 체크아웃에서 실행한 빌드는 다른 체크아웃의 파일을 읽지도 쓰지도 않습니다.**

Cargo는 path 패키지의 유닛 해시에 패키지 경로를 넣지 않습니다. 그래서 워크트리 여럿이 빌드 디렉터리 하나를 같이 쓰면, 내용이 같은 두 워크트리가 캐시 항목 하나가 되고, 컴파일 시점에 박히는 절대 경로(`env!("CARGO_MANIFEST_DIR")`, `include_str!`, 빌드 스크립트의 `OUT_DIR` 산출물, 5.3.4의 절대 이름)가 전부 먼저 빌드한 쪽 것이 됩니다. 결정과 확인된 사고 두 건은 INTENT D13에 있습니다.

#### 5.4.1 설정

1. **빌드 디렉터리는 Cargo의 기본값입니다.** 워크스페이스 루트 아래의 `target/`이고, 워크트리마다 따로입니다.
2. **`target-dir`을 적은 기계 로컬 설정을 두지 않습니다.** `.cargo/config.toml`에도, `CARGO_TARGET_DIR`에도 두지 않습니다. 절대 경로를 적어 두는 설정은 체크아웃을 옮기거나 워크트리를 만들 때마다 어긋날 자리를 하나 남깁니다.
3. **예외는 스스로 만들고 스스로 지우는 임시 디렉터리뿐입니다.** `scripts/tests/consumer-crate.test.sh`가 임시 디렉터리를 `CARGO_TARGET_DIR`로 쓰는 것이 그 경우입니다. 바깥 cargo와 같은 `target/`을 쓰면 잠금에서 멈추기 때문입니다.
4. `scripts/setup-worktrees.sh`가 이 상태를 만들고 확인하며, 워크트리별 빌드 디렉터리 크기와 남은 공간을 함께 보고합니다. 디스크가 D13이 받아들인 비용이므로, 349GB 사건이 사후가 아니라 사전에 보여야 합니다.

#### 5.4.2 소스 트리에 쓰는 도구

설정은 어긋날 수 있으므로 소스 트리에 쓰는 도구는 스스로 확인합니다.

- 자기가 컴파일된 크레이트 디렉터리(`env!("CARGO_MANIFEST_DIR")`)와 cargo가 실행 시점에 환경으로 알려 준 크레이트 디렉터리(`CARGO_MANIFEST_DIR`)를 비교합니다.
- 두 값이 같으면 그 디렉터리에 씁니다.
- 실행 시점 값이 없으면(cargo를 거치지 않고 바이너리를 직접 실행한 경우) 컴파일된 디렉터리를 씁니다. 의도한 트리를 알 방법이 달리 없고, 그것이 그 바이너리가 아는 유일한 트리입니다.
- 두 값이 다르면 **아무것도 쓰지 않고** 두 경로와 고치는 법을 찍으며 실패합니다.

#### 5.4.3 수용 기준

1. 6줄짜리 크레이트를 두 디렉터리에 같은 내용으로 두고 각자 빌드 디렉터리를 주면, 각 디렉터리의 `cargo run`이 자기 디렉터리를 출력합니다. **같은 빌드 디렉터리를 주면 두 번째가 첫 번째에서 컴파일된 바이너리를 실행한다는 것은 단언하지 않고 함께 보고만 합니다.** cargo가 나중에 이것을 고치더라도 검사가 빨개지면 안 되고, 고쳐지든 아니든 이 저장소가 빌드 디렉터리를 나눈다는 규칙은 그대로이기 때문입니다.
2. `CARGO_MANIFEST_DIR`을 빈 임시 디렉터리로 두고 codegen 바이너리를 실행하면 종료 코드가 0이 아니고, 그 디렉터리는 비어 있으며, 메시지에 두 경로가 모두 나옵니다.
3. 이 저장소의 어떤 워크트리에도 `target-dir` 설정이 없고, 환경에 `CARGO_TARGET_DIR`도 없습니다.
4. `scripts/setup-worktrees.sh`와 에이전트 런처(`scripts/launch-agent.sh`)가 같은 규칙을 따릅니다. 런처는 빌드 디렉터리를 스스로 적지 않고 `scripts/setup-worktrees.sh`에 맡깁니다.

1과 3과 4를 `scripts/tests/worktree-target.test.sh`가, 2를 `dioxus-compose/tests/codegen_tree.rs`가 확인합니다. `scripts/setup-check.sh`도 이 체크아웃 하나에 대해 3을 봅니다.

**2026-09-22 측정.** 자기 `target/`에 처음부터 빌드하는 데(`cargo build --workspace --tests`, 레지스트리 캐시는 더운 상태) 23초가 걸렸고 `target/`은 1.0GB가 됐습니다. `scripts/check.sh`를 한 번 돌리면 벤치마크까지 포함해 2.0GB가 됩니다. 워크트리 여덟 개가 같이 쓰던 빌드 디렉터리 하나는 그때 16GB였습니다. 워크트리마다 나누는 쪽이 이 기계에서는 디스크도 덜 씁니다. 공유 디렉터리는 워크트리 여덟 개분의 핑거프린트를 한꺼번에 들고 있으면서 아무도 치우지 않기 때문입니다.

## 6. IME 수용 체크리스트 (FR-5, M1)

native-image 빌드에서 macOS와 Windows 각각 수동으로 확인합니다.

**macOS arm64 결과 (2026-09-21, Liberica NIK 25, native-image 빌드)**: 5개 항목을 사람이 직접 확인했습니다. 조합 과정 표시, 조합 중 자모 단위 백스페이스, 조합 중 화살표 이동 시 확정 후 이동, 문장 중간 삽입, 한글 폰트 폴백입니다. **M1의 관문이 이 항목이었고, 통과했습니다.**

확인 절차: `DIOXUS_COMPOSE_SMOKE_IME=1 desktop/scripts/smoke-test.sh`로 텍스트 필드가 있는 스모크 창을 띄우고 한국어 입력기로 입력합니다. 이 플래그가 없으면 창에 라벨과 버튼만 있어서 타이핑할 곳이 없습니다.

남은 4개는 미확인이며, 자동화할 수 없으므로 사람이 실행해야 합니다.

여기서 발견한 실패 양상을 남겨 둡니다. 등록되지 않은 입력 경로는 빌드도 렌더링도 멀쩡히 통과한 뒤, 입력기가 텍스트 필드를 건드리는 순간 Objective-C 예외로 프로세스를 abort시킵니다. Java 스택 트레이스 없이 창이 그냥 사라지므로, 이 증상이 보이면 실행 로그에서 `JNI Lookup Exception`과 그 앞의 `NoSuchMethodError`를 먼저 찾으십시오. 근본 대응은 `ImeReachabilityFeature`가 패키지 단위로 등록하는 것입니다(INTENT D9-macOS).

- [x] "안녕하세요" 입력 시 조합 과정이 정상 표시됨
- [x] 조합 중 백스페이스로 자모 단위 삭제
- [x] 조합 중 화살표로 커서 이동 시 조합 확정 후 이동
- [x] 문장 중간에 커서를 두고 한글 삽입
- [ ] 멀티라인 필드에서 조합 중 Enter 처리 (조합 확정과 줄바꿈/제출 구분)
- [ ] 한글이 섞인 긴 텍스트 붙여넣기
- [ ] 일본어/중국어 IME 후보창이 커서 위치에 뜸
- [ ] Host가 `TextChanged`를 받는 동안 조합이 리셋되지 않음
- [x] 한글 폰트 폴백 (두부 문자 없음)

실패하면 INTENT D4에 따라 native-image 설정(ServiceLoader, JNI/리플렉션 config, 로케일/문자셋)부터 점검합니다.

**입력 경로 등록은 반드시 빌드 시점 Feature(`ImeReachabilityFeature`)로 합니다.** 메타데이터에 `allDeclaredMethods`를 쓰면 안 됩니다. JDK가 선언만 하고 라이브러리에 넣지 않은 네이티브 메서드(`CInputMethod.nativeHandleEvent`)까지 링크 대상이 되어, Java 스택 트레이스 없이 dyld 단계에서 라이브러리 로드가 실패합니다. 반대로 등록이 부족하면 렌더링까지 정상 동작한 뒤 입력기가 텍스트 필드를 건드리는 순간 프로세스가 abort합니다.

## 7. 접근성 (NFR-8)

**2026-09-21: 네이티브 이미지가 라벨된 접근성 트리를 노출하고, 트리를 읽어도 죽지 않습니다.**

| | JVM 개발 셸 | native-image (수정 전) | native-image (수정 후) |
|---|---|---|---|
| 트리 요소 수 | 14 | 1 | 12 |
| 라벨된 컨트롤 | 있음 | 없음 | `AXStaticText`, `AXButton` |
| 질의 후 프로세스 | 정상 | 중단(exit 134) | 정상 |

### 원인: 네이티브 링크에서 클래스가 제거됨

Objective-C 쪽은 Java의 역할(role)을 **클래스 이름 문자열**로 바꿔 `NSClassFromString`으로 찾습니다. 그래서 `GroupAccessibility`, `ButtonAccessibility`, `StaticTextAccessibility`, `IgnoreAccessibility`를 심볼로 참조하는 코드가 이미지 안에 없습니다. `-force_load`가 오브젝트를 가져와도 링커가 참조 없는 클래스로 보고 제거합니다. 수정 전 빌드에는 아카이브의 `*Accessibility` 클래스 38개 중 12개만 남아 있었고, 사라진 것이 정확히 역할이 가리키는 클래스들이었습니다.

`NSClassFromString`이 nil을 반환하고, `[nil alloc]`이 nil이 되고, 그 nil을 AppKit이 자식 배열에 넣으려다 예외를 던집니다.

**정적 분석이 이것을 볼 수 없습니다.** 문제가 Java 도달 가능성이 아니라 네이티브 링크에 있기 때문입니다. `--exact-reachability-metadata`가 아무 누락도 보고하지 않으면서 증상이 그대로였던 이유입니다.

기각된 가설 둘을 기록해 둡니다. `GetFieldID(AccessibleRole, "key")` 실패는 원인이 아닙니다. 해당 네이티브가 **호출조차 되지 않았습니다**. Java 예외도 아닙니다. Java 예외는 `NSGenericException`으로 나타나며 실제 예외는 `NSInvalidArgumentException`이었고, 이는 nil 자식만이 만듭니다.

### 수정

`build-native.sh`가 아카이브에서 `*Accessibility` 클래스 목록을 읽어 전부 링크 루트(`-Wl,-u`)로 지정합니다. JDK가 역할을 추가해도 목록이 낡지 않습니다. 이미지 크기는 50KB 늘었습니다.

`desktop/scripts/tests/accessibility-link.test.sh`가 아카이브의 클래스와 빌드된 라이브러리의 클래스를 비교해 누락이 있으면 실패합니다. 빌드도 스모크 테스트도 둘 다 통과하므로, 이 검사가 없으면 부재를 알 방법이 없습니다. CI의 macOS 빌드 뒤에 붙였습니다.

### 남은 수용 기준

- [x] native-image의 접근성 트리가 JVM과 같은 구조로 노출됨
- [x] 트리를 질의해도 프로세스가 중단되지 않음
- [ ] macOS VoiceOver가 Text와 Button 라벨을 읽음: **사람이 확인해야 합니다**
- [ ] Tab 키 포커스 순회
- [ ] Windows Narrator: Windows 빌드 검증 후

VoiceOver 수동 절차는 `experiments/accessibility/README.md`에 있습니다. 오늘까지는 VoiceOver를 켜는 것만으로 프로세스가 죽어서 2단계를 넘어갈 수 없었고, 이제 끝까지 진행할 수 있습니다.

## 8. 열린 질문 추적

[PROJECT.md](../PROJECT.md#열린-질문)의 열린 질문 표를 참고하세요. 결정이 나면 해당 SPEC 항목을 `Agreed`로 올리고 INTENT에 결정을 기록합니다.
