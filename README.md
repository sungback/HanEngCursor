# 한/영 입력 표시기 — Walkthrough

## What Was Built

마우스 커서 옆에 현재 한/영 입력 상태를 실시간으로 표시하는 Windows 11 시스템 유틸리티.

| 항목 | 내용 |
|------|------|
| 언어 | Rust 1.93.1 |
| 핵심 라이브러리 | `windows-rs` 0.62 (Microsoft 공식) |
| 바이너리 크기 | **120KB** (release, stripped + LTO) |
| 폴링 간격 | 50ms |

## Project Structure

```
Win11Mouse/
├── Cargo.toml          # 의존성 정의
└── src/
    ├── main.rs         # 메시지 루프 + 타이머
    ├── ime.rs          # ImmGetConversionStatus로 한/영 감지
    ├── overlay.rs      # 투명 오버레이 윈도우 (GDI 렌더링)
    └── tray.rs         # 시스템 트레이 아이콘 + 종료 메뉴
```

## Key Implementation Details

### IME 상태 감지 (`ime.rs`)
- `GetGUIThreadInfo` → 현재 포커스된 창 핸들
- `ImmGetContext` + `ImmGetConversionStatus` → `IME_CMODE_NATIVE` 플래그로 한글/영문 판별

### 오버레이 윈도우 (`overlay.rs`)
- `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`
- 마젠타 색상 키 → 투명 처리 / 클릭 통과
- 한글 모드: 🔵 파란 배경 + "한" / 영문 모드: 🟢 초록 배경 + "A"

### 시스템 트레이 (`tray.rs`)
- 트레이 아이콘 + 우클릭 메뉴 → "종료" 기능

## Build Verification

```
✅ cargo build          → 0 errors, 0 warnings
✅ cargo build --release → 0 errors, 0 warnings (120KB binary)
```

## How To Run

```powershell
cd c:\Users\sam\Dropbox\Win11Mouse
cargo run --release
```

또는 빌드 후 직접 실행:

```powershell
.\target\release\win11mouse.exe
```

종료: 시스템 트레이 아이콘 **우클릭** → **종료**

## 📖 작동 원리 (Educational Overview)

이 프로그램은 자동차가 엔진, 핸들, 바퀴로 나뉘듯 4개의 핵심 부품(Rust 파일)으로 이루어져 있습니다. 마우스 커서 옆에 한/영 상태가 따라다니게 하기 위한 필수적인 기능만을 가장 가볍고 빠르게 수행하도록 설계되었습니다.

1. **`main.rs` (지휘관)**
   - 프로그램의 시작과 끝을 담당하며, 1초에 20번(50ms)씩 타이머를 발생시켜 끊임없이 시스템의 상태를 확인하고 다른 부품들에게 업데이트를 지시합니다.
2. **`ime.rs` (정보 수집가)**
   - 현재 사용자가 타이핑하고 있는 창을 알아낸 뒤, 그 창의 키보드 입력기(IME) 상태를 확인하여 지금이 "한글(Korean)" 모드인지 "영문(English)" 모드인지 판별해 냅니다.
3. **`overlay.rs` (시각 효과)**
   - 윈도우의 투명 창 기능(`WS_EX_LAYERED`)을 이용해 마우스 커서 우측 하단에 찰싹 달라붙는 마술 같은 유령 창을 생성합니다. 클릭을 무시하므로 작업에 방해를 주지 않으면서 한/영 마크(한, A)를 그려냅니다.
4. **`tray.rs` (이모지 공장)**
   - 외부 이미지 파일 없이 프로그램이 스스로 투명한 메모리 도화지를 꺼내 윈도우 기본 폰트로 🇺🇸, 🇰🇷 이모지를 그리고, 이를 시스템 트레이 아이콘으로 변환해 작업표시줄 우측 하단에 띄워줍니다.

**왜 Rust 언어를 사용했나요?**
이런 시스템 도구는 백그라운드에서 실행되며 사용자의 컴퓨터 자원을 낭비해서는 안 됩니다. Python이나 JavaScript(Electron)로 만들었다면 구동기가 필요해 수십 MB의 메모리를 차지했겠지만, Rust 컴파일러의 극한 최적화를 거친 이 프로그램은 고작 **120KB**의 초소형 크기이며 속도 지연(Lag) 없이 윈도우(Win32 API)와 직접 통신합니다.
