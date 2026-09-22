# enhyp / unhyp

HY Project 게임 배포 형식 `.hyp`(HYPV1)를 만들고 푸는 Rust 도구.

| 크레이트 | 역할 |
|---|---|
| `hyp` | 라이브러리. **HY Launcher가 이걸 링크한다** |
| `enhyp` | 디렉터리 → `.hyp`. MOTP 빌드 스크립트가 호출한다 |
| `unhyp` | `.hyp` → 디렉터리. 사람이 확인·디버깅할 때 |

형식 명세와 **해제 계약**은 [`FORMAT.md`](FORMAT.md)가 정본이다. 런처의
`install/extract.rs`가 그 계약 위에 지어져 있으므로, 계약을 바꾸려면 런처도
같이 봐야 한다.

## 쓰기

```sh
cargo build --release

# 압축 (기본 preset 9 = 최대 압축)
./target/release/enhyp dist/staging/windows MOTP_0.7.0_windows_x64.hyp

# 내용만 보기 (풀지 않는다)
./target/release/unhyp --list MOTP_0.7.0_windows_x64.hyp

# 해제
./target/release/unhyp MOTP_0.7.0_windows_x64.hyp ~/HYLauncher/motp
```

## 라이브러리로 쓰기

비공개 저장소이므로 git 의존성에 인증이 필요하다. `.cargo/config.toml`에:

```toml
[net]
git-fetch-with-cli = true
```

`Cargo.toml`에서는 **브랜치가 아니라 tag나 rev로 고정한다.** 브랜치를 가리키면
런처를 다시 빌드할 때마다 아카이브 형식이 소리 없이 바뀔 수 있다.

```toml
[dependencies]
hyp = { git = "ssh://git@github.com/inoyu4649/enhyp-unhyp.git", tag = "v0.1.0" }
```

```rust
hyp::extract(&archive, &dest, |done, total| {
    // done/total은 압축 해제 후 바이트다
})?;
```

## 이것은 암호화가 아니다

`HYPV1` 매직 넘버의 목적은 **탐색기에서 더블클릭했을 때 7-Zip이 열어서
내용물이 드러나는 상황을 막는 것**뿐이다. 형식을 아는 사람은 누구나 풀 수
있고, 그래도 된다. 이 저장소가 비공개인 것도 방어책이 아니라 그냥 정리다.

## 테스트

```sh
cargo test --workspace
```

`tests/hostile.rs`가 중요하다 — `..`, 절대 경로, 역슬래시, Windows 예약
장치명, 위조된 체크섬이 든 아카이브를 **손으로 조립해서** 거부되는지, 그리고
대상 디렉터리 바깥에 아무것도 생기지 않는지 본다. `pack()`은 같은 검사를
통과하지 못해 이런 파일을 만들어 주지 않으므로, 손으로 만드는 수밖에 없다.
