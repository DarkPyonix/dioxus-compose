# 편집기 스파이크: Monaco 모양 코드 편집기를 HTML/CSS로

이슈 #146. Ember의 코드 편집 영역을 네이티브 위젯 대신 dioxus-compose HTML/CSS(Monaco의 DOM 구조와 CSS, 줄 가상화)로 만들 수 있는지 숫자로 판정한다. 리더 결정(2026-10-26, 오너 위임)은 "Ember IDE 창 전체를 HTML/CSS로 하되 이 스파이크로 게이트한다"이고, 기준을 못 넘으면 대안은 compose-rust의 CodeEditor 위젯을 widgets-in-HTML 경로로 끼워 넣는 것이다.

## 결론

**기준 미달.** CI macOS 러너, 100,000줄 생성 파일, Monaco와 같은 `top` 방식에서:

| 기준 | 목표 | 측정 (p50 / p95 / max) | 판정 |
|---|---|---|---|
| 키 입력에서 프레임까지 (500회) | p95 <= 16 ms | 16.14 / 25.47 / 50.73 ms | 미달 |
| 실제 텍스트 필드 경로 키 입력 (렌더러 계측, 5회) | p95 <= 16 ms | 평균 23.05, 최악 32.38 ms | 미달 |
| 부드러운 휠 스크롤 프레임 (12px/프레임, 300회) | p95 <= 16.7 ms | 39.42 / 57.66 / 93.14 ms | 미달 |
| 한 페이지 스크롤 프레임 (100회) | p95 <= 16.7 ms | 58.36 / 92.70 / 106.73 ms | 미달 |

Windows 러너도 같은 결론이다(키 입력 p95 28.17 ms, 휠 p95 51.83 ms). 반면 아무것도 바뀌지 않는 프레임은 디스플레이 주기(16.4 ms)를 지킨다. 정적인 IDE 크롬은 HTML/CSS로 충분하고, 매 프레임 바뀌는 편집 영역이 안 된다.

**#43 확인:** HTML 화면은 실제 창에 그려진다. macOS(Kotlin/Native 렌더러)와 Windows(GraalVM 렌더러) 모두 스크린샷으로 확인했다(`results/macos-idle.png`, `results/windows-idle.png`). 줄 번호, 토큰 색, 현재 줄 테두리, 선택, 커서가 보인다. 다만 글꼴 패밀리가 렌더러에 전달되지 않아 고정폭이 아닌 디자인 시스템 글꼴로 그려지고, 그래서 커서와 선택 위치가 글자와 어긋난다.

## 무엇을 만들었나

- `src/lib.rs`: Monaco DOM. `.monaco-editor > .overflow-guard`, `.margin > .margin-view-overlays`(줄 번호, 현재 줄), `.monaco-scrollable-element > .lines-content > .view-overlays`(현재 줄, 선택) `+ .view-lines > .view-line > span > span.mtkN` `+ .cursors-layer > .cursor`, 세로 스크롤바, 커서 위치의 숨은 `textarea.inputarea`. 보이는 줄과 위아래 5줄만 DOM에 있고, 각 줄은 `top: index * 19px`로 절대 배치된다. 스크롤은 `.lines-content`의 `top`(Monaco 방식) 또는 `transform: translateY`(비교용)로 옮긴다. 좌표는 Monaco의 `bigNumbersDelta`처럼 100,000px 단위 기준점에 상대적으로 쓴다. 토크나이저는 줄 주석, 문자열, 숫자, 키워드, 타입, 호출만 가르는 단순한 것이다.
- `monaco/monaco-subset.css`: monaco-editor 0.52.2(`min/vs/editor/editor.main.css`, vscode 커밋 404545bded1df6ffa41ea0af4e8ddb219018c6c1)에서 필요한 규칙만 원문 그대로 vendoring했다. MIT 고지는 파일 머리와 `monaco/LICENSE.txt`에 있다. Monaco가 스크립트로 쓰는 것(테마 색 변수, `mtk*` 색, 현재 줄 테두리, 각 뷰 파트의 크기와 위치)은 `src/style.css`에 같은 값으로 적었다(VS Code Dark+, MIT).
- 파일: 생성된 100,000줄(Rust 모양 13줄 블록 반복)과 실제 Rust 파일인 stylo 0.8.0의 `stylist.rs`(4,465줄).
- `src/bin/headless.rs`: 렌더러 없이 `HtmlRuntime`이 한 프레임에 하는 단계(Dioxus render, layout, plan, bridge apply)를 공개 API로 하나씩 돌리고 잰다. batch의 레코드 수와 바이트도 센다.
- `src/bin/window.rs`: 실제 창. `HtmlRuntime`을 감싼 `Probe`가 Host 대신 호출을 받아 매 프레임 시간을 재고, 프레임 사이에 시나리오(유휴, 페이지 스크롤, 휠 스크롤, 타이핑)를 핸들러와 같은 방식으로 editor 신호에 쓴다. 시나리오가 도는 동안은 프레임이 끝나자마자 다음 프레임을 요청하므로 렌더러 속도로 돈다.
- `scripts/run-window.sh`, `.github/workflows/test-editor-spike.yml`("Editor spike"): macOS와 Windows 러너에서 빌드, headless 측정, 시나리오마다 프로세스를 따로 띄운 창 측정, 스크린샷, job summary와 artifact. 숫자로는 실패하지 않는다.

로컬에서는 아무것도 빌드하지 않았다. 숫자는 모두 CI 실행 40286286928(커밋 a35acfbc)의 것이고 원본은 `results/`에 있다.

## 지표 정의

- **host render**: Host가 페이지의 `render`에 쓴 시간. Dioxus, 스타일, 레이아웃, 디스플레이 리스트, plan, bridge, 그리고 처음 보는 텍스트에 대한 렌더러 measure 호출까지 포함한다.
- **frame interval**: 이 프레임 시작에서 다음 프레임 시작까지. UI 스레드에서 프레임이 하나씩 처리되므로 Host 몫, 렌더러 몫(batch 적용, 재구성, 그리기, present), 디스플레이 대기를 모두 포함한다. 스크롤 프레임 시간의 기준이다.
- **interval minus host**: 위 둘의 차이. 렌더러 몫과 디스플레이 대기다.
- **key to next frame**: 키를 프레임 시작에 페이지로 넘기고(이벤트가 dispatch되면 Host가 곧바로 render하는 것과 같다), 그 키를 보여 준 프레임이 그려진 뒤 다음 프레임이 시작될 때까지. 키 입력에서 프레임까지의 기준이다.
- **renderer frame avg / worst**: 렌더러의 `DXC_REPORT_LATENCY` 요약. Kotlin/Native는 프레임별 값이 아니라 평균과 최악만 출력한다.
- **real text field path**: 렌더러의 `DXC_SYNTH=type`이 첫 텍스트 필드(숨은 textarea)를 누르고 700 ms마다 글자 하나씩 5개를 commit한다. 실제 필드와 `input` 핸들러를 거친다. 렌더러가 재는 "입력을 scene에 넘긴 순간부터 그 변화가 그려진 프레임까지"다.

## 숫자: 창 (macOS, CI, 1024x768 화면, 1.0x)

ms, p50 / p95 / max. 100k는 생성 파일, stylist.rs는 실제 파일.

| 시나리오 | host render | frame interval | interval minus host | key: host | key to next frame | 렌더러 frame avg / worst |
|---|---|---|---|---|---|---|
| 유휴 100k | 4.50 / 8.00 / 10.51 | 16.38 / 19.82 / 20.61 | 11.89 / 15.65 / 16.37 | - | - | 5.74 / 50.92 |
| 페이지 스크롤 100k | 34.04 / 67.03 / 78.00 | 58.36 / 92.70 / 106.73 | 24.27 / 28.09 / 32.97 | - | - | 38.91 / 119.03 |
| 휠 스크롤 100k, top | 12.08 / 19.55 / 28.67 | 39.42 / 57.66 / 93.14 | 27.67 / 41.36 / 67.61 | - | - | 28.27 / 97.45 |
| 휠 스크롤 100k, transform | 9.92 / 17.10 / 31.95 | 34.00 / 52.40 / 78.59 | 24.20 / 37.50 / 61.07 | - | - | 24.98 / 88.91 |
| 휠 스크롤 stylist.rs | 8.02 / 14.80 / 35.14 | 31.29 / 49.82 / 84.27 | 22.77 / 36.31 / 55.50 | - | - | 22.02 / 81.19 |
| 타이핑 100k, top | 8.00 / 14.40 / 34.92 | 16.71 / 21.54 / 50.73 | 7.24 / 15.02 / 38.91 | 10.18 / 16.26 / 34.92 | 16.14 / 25.47 / 50.73 | 10.09 / 46.77 |
| 타이핑 100k, transform | 9.10 / 15.38 / 30.86 | 16.63 / 23.62 / 78.30 | 6.65 / 14.78 / 61.34 | 10.83 / 17.66 / 30.86 | 16.29 / 26.65 / 78.30 | 11.02 / 75.61 |
| 타이핑 stylist.rs | 7.43 / 11.84 / 32.85 | 16.65 / 20.18 / 47.94 | 9.04 / 15.25 / 36.19 | 9.23 / 13.19 / 32.85 | 15.66 / 20.47 / 47.94 | 9.15 / 49.07 |

실제 텍스트 필드 경로(100k): 렌더러 input latency 평균 23.05 ms, 최악 32.38 ms(5회). 그 키마다 Host의 `handle_event`는 0.03 ~ 0.09 ms, 이어지는 render는 6.6 ~ 21.3 ms였다.

Windows 러너(GraalVM 렌더러, 1028x780)도 모양이 같다. 휠 100k top: host 12.59 / 19.11, interval 36.61 / 51.83. 타이핑 100k top: host 10.62 / 17.31, key to next frame 17.70 / 28.17. Windows 렌더러는 `DXC_SYNTH=exit`로 끝나지 않아 렌더러 요약이 없다. 전체는 `results/window-windows.md`.

## 숫자: Host 단계, 렌더러 없음 (macOS)

ms, p50 / p95. 레코드는 p50 / p95 / max.

| 시나리오 (100k, top, Parley) | 합계 | render | layout | plan | apply | 레코드 | 바이트 p50 |
|---|---|---|---|---|---|---|---|
| 첫 프레임 | 25.35 | 0.00 | 24.53 | 0.26 | 0.56 | 2117 | 52,521 |
| 변화 없음 | 3.57 / 4.69 | 0.00 | 2.92 / 3.98 | 0.16 / 0.23 | 0.43 / 0.55 | 0 | 12 |
| 한 페이지 스크롤 | 18.41 / 38.74 | 1.06 / 1.52 | 15.47 / 32.73 | 0.18 / 0.23 | 1.71 / 3.65 | 1531 / 1548 / 1548 | 36,106 |
| 휠 12px | 44.78 / 57.26 | 0.43 / 0.64 | 39.98 / 51.66 | 0.20 / 0.37 | 3.91 / 5.32 | 52 / 54 / 77 | 1,150 |
| 키 입력 | 43.84 / 52.88 | 0.37 / 0.55 | 39.29 / 47.65 | 0.19 / 0.24 | 3.91 / 4.84 | 6 / 6 / 252 | 423 |

- 이벤트 경로(wheel 포인터 이벤트, textarea `input`) 자체는 0.02 ~ 0.04 ms이다. 같은 변화를 신호에 바로 써도 숫자가 같다.
- 텍스트 측정을 공짜로 만든 고정폭 측정기에서도 휠 프레임 layout이 38.57 / 47.88 ms로 같다. 시간은 텍스트 shaping이 아니라 스타일, 박스 트리, 레이아웃, 디스플레이 리스트 쪽에 있다.
- 같은 변화가 창 안 Host render에서는 8 ~ 12 ms인데 headless layout에서는 약 40 ms다. 차이의 원인은 이 스파이크에서 밝히지 못했다(측정기 차이는 위에서 배제). 판정은 창 숫자로 한다.
- `transform` 방식은 Host에서 레코드 수가 `top`과 같고, 렌더러 몫이 조금(p50 기준 약 3 ms) 줄어든다.

## 시간이 어디로 가나

1. **스크롤에서는 렌더러가 가장 크다.** 휠 프레임마다 Host가 보내는 것은 52 레코드, 약 1.2 KB(`.lines-content` 위치, 들어오고 나가는 줄)뿐인데 렌더러 몫이 p50 28 ms, 렌더러 자체 평균 28 ms다. 부모 하나의 위치가 바뀌면 그 아래 약 50개의 텍스트 노드(줄마다 span 10개 안팎)가 다시 측정되거나 다시 그려지는 것으로 보인다. 같은 장면에서 타이핑(한 줄의 텍스트와 커서가 바뀜)은 렌더러 몫이 p50 7 ms다.
2. **Host는 매 프레임 문서 전체를 다시 배치한다.** 변화가 없어도 3 ~ 6 ms, 변화가 있으면 창에서 8 ~ 20 ms다. 스크롤처럼 컨테이너 하나의 위치만 바뀌어도 스타일과 레이아웃과 디스플레이 리스트가 문서 전체에 대해 다시 돈다.
3. **페이지 단위 이동은 줄을 통째로 갈아 끼운다.** 1,531 레코드, 36 KB. Host layout p95 33 ms, interval p95 93 ms.
4. **유휴 프레임도 렌더러가 약 10 ms를 쓴다**(CI VM 기준). 변화가 없어도 장면 전체를 다시 그린다.

CI 러너는 가상 GPU를 쓰는 VM이므로 실제 Apple silicon 기기에서는 절대값이 작아질 수 있다. 그러나 스크롤 프레임에서 렌더러가 Host의 2 ~ 3배를 쓰는 비율과, 스크롤 p95가 예산의 3배를 넘는 차이는 기기 차이로 메워질 크기가 아니다.

## 통과하려면 바뀌어야 하는 것

dioxus-compose:

1. **증분 레이아웃.** 더러워진 서브트리만 다시 스타일링하고 배치하고, 디스플레이 리스트와 plan도 바뀐 노드만 만든다. 아무것도 더럽지 않으면 레이아웃을 건너뛴다. 컨테이너 하나의 `top`이나 `transform` 변경은 O(1)이어야 한다.
2. **글꼴 패밀리 전달.** 지금은 렌더러가 디자인 시스템 글꼴로 그려 `font-family: Menlo, monospace`가 무시된다. 코드 편집기는 고정폭이 전제다(#144와 관련).
3. **HTML 필드의 포커스 API**(`element.focus()` 상당). 숨은 textarea에 포커스를 줄 방법이 없어서 이 스파이크는 textarea를 커서 위(z-index 10)에 두고 사용자가 그것을 눌러야 입력된다.
4. **키 이벤트.** HTML 페이지에 오는 키는 Enter뿐이다. 화살표, Home/End, PageUp/PageDown, 빈 필드에서의 Backspace, Tab, 단축키, IME 조합 상태가 와야 편집기가 된다.
5. **Compose 제약 범위 밖 크기.** 100,000줄을 그대로 높이로 쓰면(1,900,000px) 렌더러가 `Can't represent a width of 64 and height of 1900000 in Constraints`로 macOS에서는 프로세스가 abort된다. bridge가 크기를 제한하거나 렌더러가 오류를 격리해야 한다. 스파이크는 Monaco처럼 기준점 상대 좌표로 피했다.

compose-rust:

6. **위치 변경은 배치만.** AbsoluteBox 자식의 offset 변경이 재측정 없이 배치 단계(`Modifier.offset { }` 람다 형태나 graphicsLayer translation)에서 끝나야 한다.
7. **텍스트 레이아웃과 그리기 캐시.** 바뀌지 않은 줄은 Paragraph를 다시 만들지 않고, 줄마다 레이어로 녹화해 위치만 옮긴다.
8. **유휴 프레임과 damage.** 바뀐 것이 없으면 다시 그리지 않거나 바뀐 영역만 그린다.
9. **계측.** Kotlin/Native 렌더러의 프레임별 시간 출력, `DXC_SYNTH` 타이핑 글자 수 설정(지금 5글자 고정), Windows 렌더러의 `DXC_SYNTH=exit`와 요약. 지금은 렌더러 몫을 frame interval에서 Host를 빼서 추정한다.

1과 6, 7이 스크롤 기준을, 1과 7이 키 입력 기준을 좌우한다. 2 ~ 5는 숫자와 상관없이 편집기가 쓸 만하려면 필요하다.

## 판정에 따른 제안

게이트 조건대로 편집 영역은 대안 경로로 간다. Ember 창의 나머지(활동 바, 사이드바, 탭, 상태 바, 패널)는 HTML/CSS로 두고, 편집 영역만 compose-rust CodeEditor 위젯을 widgets-in-HTML로 끼운다. 위 1, 6, 7이 들어오면 같은 워크플로를 다시 돌려 재판정할 수 있다.

## 다시 돌리기

PR에서 `experiments/editor-spike/**`나 워크플로 파일을 바꾸거나, Actions에서 "Editor spike"를 직접 실행한다. 결과는 job summary와 `editor-spike-<os>` artifact(로그, 스크린샷, headless 표, 해석된 `Cargo.lock`)에 남는다.
