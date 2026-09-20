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

## 라이선스

Apache License 2.0. [LICENSE](LICENSE)를 참고하세요.
