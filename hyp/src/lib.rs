//! `.hyp` (HYPV1) 아카이브.
//!
//! HY Project 게임의 배포 단위다. LZMA(XZ) preset 9으로 압축하고 `HYPV1`
//! 매직 넘버를 달아 일반 압축 프로그램이 인식하지 못하게 한다.
//!
//! **이것은 암호화가 아니다.** 형식을 아는 사람은 누구나 풀 수 있다. 목적은
//! 탐색기에서 더블클릭했을 때 7-Zip이 열어 내용물이 드러나는 상황을 막는 것뿐이다.
//!
//! 형식 명세는 저장소 루트의 `FORMAT.md`가 정본이다.
//!
//! ```no_run
//! # fn main() -> Result<(), hyp::Error> {
//! use std::path::Path;
//!
//! hyp::pack(Path::new("staging"), Path::new("MOTP_0.7.0_windows_x64.hyp"),
//!           &hyp::PackOptions::default(), |done, total| {
//!     eprintln!("{done}/{total}");
//! })?;
//!
//! hyp::extract(Path::new("MOTP_0.7.0_windows_x64.hyp"), Path::new("out"), |_, _| {})?;
//! # Ok(()) }
//! ```

mod error;
mod format;
mod pack;
mod path_guard;
mod unpack;

pub use error::{Error, Result};
pub use format::{Entry, Header, Index, Info, FORMAT_VERSION, MAGIC};
pub use pack::{pack, PackOptions, PackSummary};
pub use unpack::{extract, extract_from, read_info, ExtractSummary};

/// 이 바이트열로 시작하지 않으면 `.hyp`가 아니다.
///
/// 런처가 내려받은 파일을 **한 바이트도 풀기 전에** 거르는 데 쓴다.
pub fn looks_like_hyp(head: &[u8]) -> bool {
    head.len() >= MAGIC.len() && &head[..MAGIC.len()] == MAGIC
}
