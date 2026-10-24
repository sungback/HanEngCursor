# HanEngCursor — 한/영 입력 표시기

마우스 커서 옆과 시스템 트레이에 현재 한/영 입력 상태를 표시하는 Windows용 Rust 유틸리티입니다.

- 한글: 초록색 원과 `한`
- 영문: 빨간색 원과 `a`, Caps Lock이 켜져 있으면 `A`
- 클릭과 포커스를 방해하지 않는 투명 오버레이
- 모니터별 DPI에 맞춘 크기·글꼴·간격 조정
- 모니터 작업 영역 가장자리에서 위치 보정
- Explorer 재시작 시 트레이 재등록과 일시적 Shell 오류 재시도
- CatchType 트레이 접근성 이름을 이용한 한/영 상태 보완

## 요구 환경

- Windows 10 1607 이상 또는 Windows 11
- 빌드: Rust MSVC 툴체인과 Visual Studio Build Tools의 C++ 도구 및 Windows SDK
- 현재 검증한 툴체인: Rust 1.98.1, windows-rs 0.62
- 기본 배포 대상: x86_64-pc-windows-msvc

## 빌드와 실행

```powershell
cargo build --release --locked --target-dir target
.\target\release\HanEngCursor.exe
```

기존 실행 인스턴스가 있으면 트레이 메뉴에서 종료한 뒤 새 실행 파일을 실행하세요. 종료는 트레이 아이콘 **우클릭 → 종료**입니다. 자동 시작 등록과 설치 프로그램은 제공하지 않습니다.

## 동작 방식

50ms 타이머가 입력 상태와 커서 위치를 갱신합니다. 먼저 CatchType의 유효한 상태를 사용하고, 없으면 포커스된 창의 Windows IME 상태를 조회합니다.

Windows IME는 대상 스레드의 키보드 배열과 `ImmGetDefaultIMEWnd` / `WM_IME_CONTROL`로 판별합니다. IME 열림 상태를 먼저 확인해 이전 한글 변환 플래그가 남은 경우를 처리하며, 메시지 조회마다 20ms 타임아웃을 적용합니다.

CatchType 접근성 조회는 별도 COM MTA 스레드에서 수행합니다. 조회 결과는 전경 창이 같고 750ms 이내일 때만 사용합니다. 입력 내용이나 키보드 훅은 수집하지 않습니다. 설정과 제한은 [CatchType 안내](docs/catchtype.md)를 참조하세요.

오버레이는 Per-Monitor V2 DPI 인식을 선언하며 이전 환경에서는 Per-Monitor로 동작합니다. 100% 배율에서 24px인 표시와 15px 글꼴이 모니터 배율에 맞게 조정됩니다. 오른쪽 공간이 부족하면 커서 왼쪽으로 옮기고, 작업 표시줄을 제외한 모니터 작업 영역 안에 표시합니다.

트레이는 `TaskbarCreated` 메시지를 받으면 재등록합니다. 등록·갱신에 실패하면 1초 간격으로 재시도하며 오버레이는 계속 동작합니다. 메시지를 놓친 경우에도 5초 주기의 등록 상태 점검으로 복구를 시도합니다.

## 구조

| 파일 | 역할 |
| --- | --- |
| `src/main.rs` | 앱 상태 소유, 메시지 루프, 타이머, 오류 안내 |
| `src/ime.rs` | Windows IME 판별 |
| `src/catchtype.rs` | CatchType 접근성 상태 조회와 캐시 |
| `src/overlay.rs` | 투명 창과 GDI 렌더링, DPI에 따른 글꼴 관리 |
| `src/layout.rs` | 배율 계산과 모니터 경계 위치 보정 |
| `src/tray.rs` | 트레이 아이콘, 복구·재시도, 종료 메뉴 |
| `src/window.rs` | 창 핸들과 상태 연결의 수명 관리 |
| `app.rc`, `app.manifest` | 내장 아이콘과 DPI 매니페스트 |

## 검증

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --all-targets --locked

# 사용자 데스크톱과 Explorer가 있는 환경에서 추가 검증
cargo test --bin HanEngCursor --locked -- --ignored --test-threads=1 --nocapture

# CatchType / IME / Caps Lock 상태 진단
cargo run --example catchtype_probe --locked -- --monitor --seconds 60
```

검증 범위와 아직 확인하지 못한 실사용 시나리오는 [검증 기록](docs/validation.md)에 구분해 두었습니다. 오류 정책은 [상태 소유권과 오류 처리](docs/error-handling.md)를 참조하세요.

## 배포 패키지 생성

`Cargo.lock`을 저장소에 포함해 의존성 버전을 고정합니다. 다음 스크립트가 포맷·정적 검사·테스트·release 빌드를 순서대로 실행하고 ZIP과 실행 파일의 SHA-256을 생성합니다.

```powershell
.\scripts\build-release.ps1

# 현재 데스크톱에서 Shell 복구와 네이티브 DPI 검사도 실행
.\scripts\build-release.ps1 -Interactive
```

결과는 `dist/HanEngCursor-v0.2.2-x86_64-pc-windows-msvc.zip`입니다. ZIP에는 실행 파일, README, [변경 내역](CHANGELOG.md), 사용·검증 문서, `SHA256SUMS.txt`, 빌드 툴체인과 실제 실행 파일 크기를 기록한 `BUILDINFO.txt`가 들어갑니다. 실행 파일 크기는 빌드마다 달라질 수 있으므로 고정된 크기를 보장하지 않습니다.

생성한 ZIP은 로컬 산출물이며 GitHub Release 업로드는 별도로 수행합니다.
