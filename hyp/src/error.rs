//! 오류 타입.
//!
//! `thiserror`를 쓰지 않는다. 런처가 이 크레이트를 의존성으로 끌어 쓰므로
//! 의존성 트리를 좁게 유지한다(MOTP의 "좁게 시작한다"와 같은 이유).

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum Error {
    /// 매직 넘버가 `HYPV1`이 아니다. 십중팔구 .hyp 파일이 아니다.
    NotHyp,
    /// 형식 버전을 모른다. 더 새 런처가 필요하다.
    UnsupportedVersion(u8),
    /// 헤더가 잘렸거나 값이 말이 안 된다.
    BadHeader(String),
    /// 색인 JSON을 못 읽었다.
    BadIndex(String),
    /// 경로 규칙 위반. FORMAT.md의 "경로 규칙" 참조.
    ///
    /// 엔트리 하나만 건너뛰지 않고 아카이브 전체를 거부한다 — 절반만 설치된
    /// 게임을 남기지 않기 위해서다.
    UnsafePath { path: String, reason: &'static str },
    /// 색인의 sha256과 실제 내용이 다르다.
    ChecksumMismatch { path: String, expected: String, actual: String },
    /// 스트림이 색인이 말한 것보다 일찍 끝났다.
    Truncated { path: String, expected: u64, got: u64 },
    /// 심볼릭 링크는 담을 수 없다.
    Symlink(PathBuf),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotHyp => write!(f, ".hyp 파일이 아닙니다 (매직 넘버 HYPV1 불일치)"),
            Error::UnsupportedVersion(v) => {
                write!(f, "지원하지 않는 .hyp 형식 버전입니다: {v} (이 빌드는 1까지 읽습니다)")
            }
            Error::BadHeader(m) => write!(f, "헤더가 손상되었습니다: {m}"),
            Error::BadIndex(m) => write!(f, "색인을 읽을 수 없습니다: {m}"),
            Error::UnsafePath { path, reason } => {
                write!(f, "안전하지 않은 경로입니다: {path} ({reason})")
            }
            Error::ChecksumMismatch { path, expected, actual } => write!(
                f,
                "내용이 손상되었습니다: {path}\n  기대: {expected}\n  실제: {actual}"
            ),
            Error::Truncated { path, expected, got } => {
                write!(f, "파일이 잘렸습니다: {path} ({got}/{expected} 바이트)")
            }
            Error::Symlink(p) => write!(f, "심볼릭 링크는 담을 수 없습니다: {}", p.display()),
            Error::Io(e) => write!(f, "입출력 오류: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
