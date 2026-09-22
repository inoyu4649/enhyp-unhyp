//! `.hyp` 풀기.
//!
//! 스트리밍이다. 아카이브도, 그 안의 파일 하나도 통째로 메모리에 올리지
//! 않는다 — 버퍼는 항상 `CHUNK` 크기다. 런처가 2 GB짜리 게임을 풀 때도
//! 메모리 사용량이 변하지 않아야 한다.
//!
//! **원자성은 호출자 책임이다.** 이 모듈은 대상 디렉터리에 그냥 쓴다. 설치본
//! 교체의 원자성은 런처가 staging + rename으로 만든다.

use crate::error::{Error, Result};
use crate::format::*;
use crate::path_guard;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExtractSummary {
    pub entry_count: u64,
    pub total_uncompressed: u64,
}

/// 헤더와 색인만 읽는다. 파일 내용은 풀지 않는다.
pub fn read_info(archive: &Path) -> Result<Info> {
    let mut f = BufReader::new(File::open(archive)?);
    let header = Header::read_from(&mut f)?;
    let mut dec = liblzma::read::XzDecoder::new(f);
    let index = read_index(&mut dec, &header)?;
    Ok(Info {
        format_version: header.format_version,
        total_uncompressed: header.total_uncompressed,
        entry_count: header.entry_count,
        entries: index.entries,
    })
}

/// 아카이브를 `dest_dir` 밑으로 푼다.
///
/// `progress(done, total)`는 압축 해제 후 바이트 기준이다.
pub fn extract<P: FnMut(u64, u64)>(
    archive: &Path,
    dest_dir: &Path,
    progress: P,
) -> Result<ExtractSummary> {
    let f = BufReader::new(File::open(archive)?);
    extract_from(f, dest_dir, progress)
}

/// 임의의 `Read`에서 푼다.
pub fn extract_from<R: Read, P: FnMut(u64, u64)>(
    mut reader: R,
    dest_dir: &Path,
    mut progress: P,
) -> Result<ExtractSummary> {
    let header = Header::read_from(&mut reader)?;
    let mut dec = liblzma::read::XzDecoder::new(reader);
    let index = read_index(&mut dec, &header)?;

    // 경로 검사는 **파일을 하나도 만들기 전에** 전부 끝낸다. 중간에 걸러서
    // 멈추면 절반만 풀린 디렉터리가 남는다.
    path_guard::check_all(&index.entries)?;

    let declared: u64 = index.entries.iter().map(|e| e.size).sum();
    if declared != header.total_uncompressed {
        return Err(Error::BadIndex(format!(
            "색인의 크기 합({declared})이 헤더({})와 다릅니다",
            header.total_uncompressed
        )));
    }
    if index.entries.len() as u64 != header.entry_count {
        return Err(Error::BadIndex(format!(
            "색인의 엔트리 수({})가 헤더({})와 다릅니다",
            index.entries.len(),
            header.entry_count
        )));
    }

    std::fs::create_dir_all(dest_dir)?;
    let total = header.total_uncompressed;
    let mut done = 0u64;
    let mut buf = vec![0u8; CHUNK];
    progress(0, total);

    for e in &index.entries {
        // check_all을 통과했으므로 이 join은 dest_dir 안쪽에 머문다.
        let rel = e.path.replace('/', std::path::MAIN_SEPARATOR_STR);
        let out_path = dest_dir.join(rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut out = BufWriter::new(File::create(&out_path)?);
        let mut hasher = Sha256::new();
        let mut left = e.size;

        while left > 0 {
            let want = std::cmp::min(left as usize, buf.len());
            let n = dec.read(&mut buf[..want])?;
            if n == 0 {
                return Err(Error::Truncated {
                    path: e.path.clone(),
                    expected: e.size,
                    got: e.size - left,
                });
            }
            out.write_all(&buf[..n])?;
            hasher.update(&buf[..n]);
            left -= n as u64;
            done += n as u64;
            progress(done, total);
        }

        out.flush()?;
        drop(out);

        let actual = hex::encode(hasher.finalize());
        if actual != e.sha256.to_lowercase() {
            return Err(Error::ChecksumMismatch {
                path: e.path.clone(),
                expected: e.sha256.clone(),
                actual,
            });
        }

        set_exec(&out_path, e.exec)?;
    }

    Ok(ExtractSummary { entry_count: header.entry_count, total_uncompressed: total })
}

fn read_index<R: Read>(dec: &mut R, header: &Header) -> Result<Index> {
    let mut raw = vec![0u8; header.index_len as usize];
    dec.read_exact(&mut raw).map_err(|e| {
        Error::BadIndex(format!("색인 {}바이트를 읽지 못했습니다: {e}", header.index_len))
    })?;
    serde_json::from_slice(&raw).map_err(|e| Error::BadIndex(e.to_string()))
}

/// 실행 비트를 복원한다.
///
/// 이게 깨지면 Linux에서 게임이 **조용히** 실행되지 않는다 — 런처는 spawn
/// 실패만 보고 이유를 모른다. FORMAT.md의 해제 계약에 박아 둔 항목이다.
#[cfg(unix)]
fn set_exec(path: &Path, exec: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if exec { 0o755 } else { 0o644 };
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_exec(_path: &Path, _exec: bool) -> Result<()> {
    Ok(()) // Windows에는 실행 비트가 없다.
}
