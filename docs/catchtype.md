# CatchType와 함께 사용하기

HanEngCursor는 CatchType 트레이 아이콘의 접근성 이름을 읽어 커서와 자체 트레이의 한영 표시를 갱신합니다. 키보드 훅을 추가하거나 입력 내용을 읽지 않습니다.

CatchType은 키를 직접 처리하고 Microsoft IME는 영문 상태로 유지하므로, Windows IME만 조회하면 한글 입력 중에도 `a`로 표시될 수 있습니다.

## 적용

1. 기존 실행 중인 프로그램을 트레이 메뉴에서 종료합니다.
2. 새로 빌드한 `target\release\HanEngCursor.exe`를 실행합니다.
3. CatchType을 켜고 입력창에서 한영 키를 눌러 커서 옆 표시를 확인합니다.

CatchType 아이콘을 찾지 못하면 작업 표시줄에 항상 보이도록 꺼내 놓고 다시 확인합니다. 숨겨진 아이콘 목록이 닫혀 있으면 Windows 버전에 따라 접근성 요소가 제공되지 않을 수 있습니다.

## 상태 선택

| CatchType 접근성 이름의 마지막 상태 | 표시 기준 |
| --- | --- |
| `Catchtype: 한글` | 한글 |
| `Catchtype: 영어` | 영문 (Caps Lock에 따라 a/A) |
| `Catchtype: 쉬는 중` | 기존 Windows IME 감지 |
| `Catchtype: 비밀번호 입력 중` | 기존 Windows IME 감지 |
| 아이콘 없음, 알 수 없는 상태, 조회 실패 | 기존 Windows IME 감지 |

Windows 11에서는 이름이 `Catchtype: 영어 Catchtype: 한글`처럼 최초 설명과 현재 설명을 합친 형태로 제공될 수 있습니다. 이 경우 마지막 `Catchtype:` 설명을 사용합니다. 여러 아이콘의 상태가 서로 다르면 IME 감지로 돌아갑니다.

접근성 조회는 별도 COM MTA 스레드에서 100ms 간격으로 실행하고, 아이콘 재검색은 보통 1초 간격으로 합니다. 커서 이동은 기존 50ms 타이머를 사용합니다. 조회 시작 후 750ms 이상 지났거나 전경 창이 달라진 결과는 사용하지 않습니다. 느린 접근성 조회를 기다리느라 커서 표시나 종료가 멈추지 않습니다.

## 진단

```powershell
cargo run --example catchtype_probe --target-dir target
cargo run --example catchtype_probe --target-dir target -- --watch
```

`--watch`는 60초 동안 상태가 달라질 때만 출력합니다. 일반 사용자 데스크톱에서 실행해야 하며, 격리된 실행 환경에서는 작업 표시줄을 찾지 못할 수 있습니다.

예상 출력:

```text
status=Some(Korean) labels=["Catchtype: 영어 Catchtype: 한글"]
```

CatchType 0.3.0-beta.6과 현재 PC에서 위 접근성 이름의 실제 조회를 확인했습니다. 전용 한영 키를 통한 양방향 전환, 자동 언어 전환, 비활성 상태 전환의 전체 실사용 검증은 아직 완료되지 않았습니다. CatchType이나 Windows 업데이트로 접근성 이름이 바뀌면 추가 대응이 필요할 수 있습니다.
