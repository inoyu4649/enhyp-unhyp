//! 경로 규칙. `FORMAT.md`의 "경로 규칙"이 정본이다.
//!
//! 이 검사는 패킹할 때와 해제할 때 **양쪽에서** 돈다. 패킹 쪽은 실수를 빨리
//! 잡기 위한 것이고, 해제 쪽이 실제 방어선이다 — 아카이브는 우리가 만들지
//! 않았을 수도 있다.

use crate::error::{Error, Result};

/// Windows 예약 장치명. 확장자가 붙어도 여전히 장치다 (`NUL.txt` → NUL).
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn bad(path: &str, reason: &'static str) -> Error {
    Error::UnsafePath { path: path.to_string(), reason }
}

/// 아카이브 안의 경로 하나를 검사한다.
///
/// 이 함수를 통과한 경로는 대상 디렉터리 **안쪽**으로만 풀린다.
pub fn check(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(bad(path, "빈 경로"));
    }
    if path.len() > 1024 {
        return Err(bad(path, "경로가 너무 깁니다"));
    }
    if path.starts_with('/') {
        return Err(bad(path, "절대 경로"));
    }
    if path.contains('\\') {
        // 구분자는 `/` 하나뿐이다. `\`를 허용하면 Windows에서 디렉터리
        // 구분자가 되어 검사를 우회한다.
        return Err(bad(path, "역슬래시는 쓸 수 없습니다"));
    }
    // `C:` 같은 드라이브 접두사.
    let b = path.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        return Err(bad(path, "드라이브 접두사"));
    }

    for c in path.chars() {
        if (c as u32) < 0x20 {
            return Err(bad(path, "제어 문자"));
        }
        if matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') {
            return Err(bad(path, "Windows에서 쓸 수 없는 문자"));
        }
    }

    for part in path.split('/') {
        if part.is_empty() {
            // 선행 `/`는 위에서 걸렀으니 여기 걸리는 건 `a//b`나 후행 `/`다.
            return Err(bad(path, "빈 경로 성분"));
        }
        if part == "." || part == ".." {
            return Err(bad(path, "상대 경로 성분"));
        }
        if part.ends_with('.') || part.ends_with(' ') {
            // Windows가 조용히 잘라내서 다른 파일이 된다.
            return Err(bad(path, "성분이 점이나 공백으로 끝납니다"));
        }
        // 확장자를 떼고 장치명인지 본다.
        let stem = part.split('.').next().unwrap_or(part);
        if RESERVED.iter().any(|r| stem.eq_ignore_ascii_case(r)) {
            return Err(bad(path, "Windows 예약 장치명"));
        }
    }

    Ok(())
}

/// 색인 전체를 검사한다. 중복 경로도 여기서 잡는다.
pub fn check_all(entries: &[crate::format::Entry]) -> Result<()> {
    let mut seen = std::collections::HashSet::with_capacity(entries.len());
    for e in entries {
        check(&e.path)?;
        // Windows는 대소문자를 구분하지 않으므로 소문자로 접어서 본다.
        // `Res/a.png`와 `res/a.png`가 같은 파일이 되어 뒤엣것이 앞엣것을
        // 덮어쓰는 걸 막는다.
        if !seen.insert(e.path.to_lowercase()) {
            return Err(bad(&e.path, "중복 경로"));
        }
        if e.sha256.len() != 64 || !e.sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(bad(&e.path, "sha256 형식이 잘못되었습니다"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_normal_paths() {
        for p in ["MOTP.exe", "res/ui/logo.png", "docs/license/oss/a.txt", "한글 파일.json"] {
            assert!(check(p).is_ok(), "{p} 는 통과해야 합니다");
        }
    }

    #[test]
    fn rejects_traversal_and_absolute() {
        for p in ["../evil", "a/../../evil", "/etc/passwd", "C:/Windows/System32/x.dll",
                  r"a\b", "..", "."] {
            assert!(check(p).is_err(), "{p} 는 거부해야 합니다");
        }
    }

    #[test]
    fn rejects_windows_hazards() {
        for p in ["NUL", "nul.txt", "a/CON/b", "com1.log", "bad:name", "q?.png",
                  "trailing.", "trailing ", "ctrl\u{1}char"] {
            assert!(check(p).is_err(), "{p} 는 거부해야 합니다");
        }
    }

    #[test]
    fn rejects_duplicates_case_insensitively() {
        let mk = |p: &str| crate::format::Entry {
            path: p.into(), size: 0, exec: false, sha256: "0".repeat(64),
        };
        assert!(check_all(&[mk("res/a.png"), mk("RES/A.PNG")]).is_err());
        assert!(check_all(&[mk("res/a.png"), mk("res/b.png")]).is_ok());
    }
}
