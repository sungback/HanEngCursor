# CatchType와 함께 사용하기

HanEngCursor는 CatchType 트레이 아이콘의 접근성 이름을 읽어 커서와 자체 트레이의 한영 표시를 갱신합니다. 키보드 훅을 추가하거나 입력 내용을 읽지 않습니다.

CatchType은 키를 직접 처리하고 Microsoft IME는 영문 상태로 유지하므로, Windows IME만 조회하면 한글 입력 중에도 `a`로 표시될 수 있습니다.

## 적용

1. 기존 실행 중인 프로그램을 트레이 메뉴에서 종료합니다.
2. 새로 빌드한 `target\release\HanEngCursor.exe`를 실행합니다.
3. CatchType을 켜고 입력창에서 한영 키를 눌러 커서 옆 표시를 확인합니다.

CatchType 아이콘을 찾지 못하면 작업 표시줄에 항상 보이도록 꺼내 놓고 다시 확인합니다. 숨겨진 아이콘 목록이 닫혀 있으면 Windows 버전에 따라 접근성 요소가 제공되지 않을 수 있습니다.

## 상태 선택

툴팁 문구가 판마다 조금씩 바뀌므로 정확한 문장 대신 마지막 `Catchtype` 뒤의 낱말로 판단합니다.

| 마지막 `Catchtype` 뒤에 있는 말 (예) | 표시 기준 |
| --- | --- |
| `꺼`, `쉬`, `비밀번호`, `보안 입력`, `권한`, `off`, `paused` (예: `Catchtype 꺼 둠 · 메뉴나 Ctrl+Alt+W로 켜요`, `…글 칸이 아닌 곳(그림판·게임 등)이라 쉬고 있어요`) | 기존 Windows IME 감지 |
| `(가)`, `한글`, `Korean` (예: `Catchtype 켜짐 · 한글 (가)`) | 한글 |
| `(A)`, `영어`, `English` (예: `Catchtype 켜짐 · 영어 (A)`) | 영문 (Caps Lock에 따라 a/A) |
| 그 밖의 말 (예: 로고 아이콘의 `Catchtype 켜짐`) | 기존 Windows IME 감지 |
| 아이콘 없음, 조회 실패 | 기존 Windows IME 감지 |

쉬는 상태를 나타내는 말을 먼저 봅니다. 예전 형식(`Catchtype: 한글`)도 그대로 인식합니다.

Catchtype 아이콘 모양을 **로고**로 두면 툴팁에 켜짐/꺼짐만 나오고 한/영이 없어 커서 표시가 맞지 않습니다. 메뉴에서 **가/A** 아이콘을 사용하세요.

Windows 11에서는 이름이 `Catchtype 켜짐 · 영어 (A) Catchtype 켜짐 · 한글 (가)`처럼 최초 설명과 현재 설명을 합친 형태로 제공될 수 있습니다. 이 경우 마지막 `Catchtype` 설명을 사용합니다. 여러 아이콘의 상태가 서로 다르면 IME 감지로 돌아갑니다.

접근성 조회는 별도 COM MTA 스레드에서 100ms 간격으로 실행하고, 아이콘 재검색은 보통 1초 간격으로 합니다. 커서 이동은 기존 50ms 타이머를 사용합니다. 조회 시작 후 750ms 이상 지났거나 전경 창이 달라진 결과는 사용하지 않습니다. 느린 접근성 조회를 기다리느라 커서 표시나 종료가 멈추지 않습니다.

## 진단

```powershell
cargo run --example catchtype_probe --target-dir target
cargo run --example catchtype_probe --target-dir target -- --watch
```

`--watch`는 60초 동안 상태가 달라질 때만 출력합니다. 일반 사용자 데스크톱에서 실행해야 하며, 격리된 실행 환경에서는 작업 표시줄을 찾지 못할 수 있습니다.

예상 출력:

```text
status=Some(Korean) labels=["Catchtype 켜짐 · 영어 (A) Catchtype 켜짐 · 한글 (가)"]
```

CatchType 0.3.0-beta.9.1과 현재 PC에서 위 접근성 이름의 실제 조회를 확인했습니다. 아이콘은 문구와 관계없이 `Catchtype`으로 시작하는 이름으로 찾으므로, 문구가 또 바뀌면 진단 출력의 `labels`에 새 문구가 그대로 보입니다. 전용 한영 키를 통한 양방향 전환, 자동 언어 전환, 비활성 상태 전환의 전체 실사용 검증은 아직 완료되지 않았습니다. CatchType이나 Windows 업데이트로 접근성 이름이 바뀌면 추가 대응이 필요할 수 있습니다.
