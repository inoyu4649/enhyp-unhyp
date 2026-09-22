//! 바깥 헤더와 색인 자료형. 정본 명세는 저장소 루트의 `FORMAT.md`다.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

pub const MAGIC: &[u8; 5] = b"HYPV1";
pub const FORMAT_VERSION: u8 = 1;
pub const HEADER_LEN: usize = 64;

/// 스트리밍 버퍼. 아카이브도 파일도 통째로 메모리에 올리지 않는다.
pub const CHUNK: usize = 64 * 1024;

/// XZ preset. 요구사항 1E-1의 "LZMA -9 (최대 압축)".
pub const PRESET: u32 = 9;

#[derive(Debug, Clone, Copy)]
pub struct Header {
    pub format_version: u8,
    pub flags: u16,
    /// 모든 파일 내용의 바이트 합. **진행률 총량**이다.
    pub total_uncompressed: u64,
    pub entry_count: u64,
    /// 색인 JSON의 압축 해제 후 길이.
    pub index_len: u64,
}

impl Header {
    pub fn write_to<W: Write>(&self, w: &mut W) -> Result<()> {
        let mut buf = [0u8; HEADER_LEN];
        buf[0..5].copy_from_slice(MAGIC);
        buf[5] = self.format_version;
        buf[6..8].copy_from_slice(&self.flags.to_le_bytes());
        buf[8..16].copy_from_slice(&self.total_uncompressed.to_le_bytes());
        buf[16..24].copy_from_slice(&self.entry_count.to_le_bytes());
        buf[24..32].copy_from_slice(&self.index_len.to_le_bytes());
        // 32..64 는 예약. 0으로 남긴다.
        w.write_all(&buf)?;
        Ok(())
    }

    /// 매직과 버전을 **한 바이트도 풀기 전에** 검사한다.
    pub fn read_from<R: Read>(r: &mut R) -> Result<Self> {
        let mut buf = [0u8; HEADER_LEN];

        // 매직을 먼저, 따로 읽는다. 헤더 전체를 read_exact 하면 64바이트보다
        // 짧은 파일이 전부 "헤더 손상"으로 보고되는데, 그런 파일은 십중팔구
        // 그냥 .hyp가 아니다 — 사용자에게는 그렇게 말해야 한다.
        let n = read_up_to(r, &mut buf[..MAGIC.len()])?;
        if n < MAGIC.len() || &buf[..MAGIC.len()] != MAGIC {
            return Err(Error::NotHyp);
        }
        r.read_exact(&mut buf[MAGIC.len()..])
            .map_err(|_| Error::BadHeader("헤더 64바이트를 읽지 못했습니다".into()))?;

        let format_version = buf[5];
        if format_version != FORMAT_VERSION {
            return Err(Error::UnsupportedVersion(format_version));
        }

        let flags = u16::from_le_bytes(buf[6..8].try_into().unwrap());
        let total_uncompressed = u64::from_le_bytes(buf[8..16].try_into().unwrap());
        let entry_count = u64::from_le_bytes(buf[16..24].try_into().unwrap());
        let index_len = u64::from_le_bytes(buf[24..32].try_into().unwrap());

        // 색인이 비정상적으로 크면 파싱을 시도하기 전에 잘라낸다. 게임 하나에
        // 엔트리 수십만 개가 들어갈 일은 없고, 이 값은 그대로 할당 크기가 된다.
        if index_len > 64 * 1024 * 1024 {
            return Err(Error::BadHeader(format!("색인 길이가 비정상입니다: {index_len}")));
        }
        if entry_count > 1_000_000 {
            return Err(Error::BadHeader(format!("엔트리 개수가 비정상입니다: {entry_count}")));
        }

        Ok(Header { format_version, flags, total_uncompressed, entry_count, index_len })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// `/` 구분자, 항상 상대 경로.
    pub path: String,
    pub size: u64,
    /// Unix 실행 비트. Windows에서 해제할 때는 무시된다.
    #[serde(default)]
    pub exec: bool,
    /// 원본 내용의 SHA-256 소문자 hex.
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub entries: Vec<Entry>,
}

/// 아카이브 요약. `read_info()`가 돌려준다.
#[derive(Debug, Clone)]
pub struct Info {
    pub format_version: u8,
    pub total_uncompressed: u64,
    pub entry_count: u64,
    pub entries: Vec<Entry>,
}

/// 짧은 읽기를 견디며 최대 `buf.len()` 바이트를 채운다.
///
/// `Read`는 요청한 만큼 안 줘도 되는 계약이다. 파이프나 네트워크 스트림에서
/// 첫 read가 5바이트를 다 주지 않는다고 ".hyp가 아니다"라고 판정하면 안 된다.
fn read_up_to<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        match r.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Error::Io(e)),
        }
    }
    Ok(filled)
}
