//! `.hyp` 만들기.
//!
//! 파일을 **두 번 읽는다**: 한 번은 sha256과 크기를 재려고, 한 번은 압축하려고.
//! 색인이 압축 스트림 안에 있어서 나중에 덧칠할 수 없기 때문이다. 대신 압축
//! 사전이 갈라지지 않아 압축률을 얻는다. 300 MB짜리 게임에서도 두 번째 읽기는
//! 대부분 페이지 캐시에서 온다.

use crate::error::{Error, Result};
use crate::format::*;
use crate::path_guard;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct PackOptions {
    /// 이 이름을 가진 디렉터리·파일은 담지 않는다. 경로 성분 단위로 비교한다.
    pub exclude: Vec<String>,
    /// XZ preset. 기본 9 (요구사항 1E-1).
    pub preset: u32,
}

impl Default for PackOptions {
    fn default() -> Self {
        // 게임 설치본에 섞여 들어가면 곤란한 것들. 빌드 스테이징을 그대로
        // 패킹하는 실수를 자주 하므로 기본값으로 막아 둔다.
        Self {
            exclude: [".git", "node_modules", ".DS_Store", "Thumbs.db", ".hylauncher"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            preset: PRESET,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackSummary {
    pub entry_count: u64,
    pub total_uncompressed: u64,
    pub compressed_size: u64,
}

struct Scanned {
    entry: Entry,
    abs: PathBuf,
}

/// 디렉터리 하나를 `.hyp` 하나로 만든다.
///
/// `progress(done, total)`는 **압축 해제 후 바이트** 기준으로 불린다.
pub fn pack<P: FnMut(u64, u64)>(
    src_dir: &Path,
    dest: &Path,
    opts: &PackOptions,
    mut progress: P,
) -> Result<PackSummary> {
    let src_dir = src_dir.canonicalize()?;
    let mut scanned = Vec::new();
    scan(&src_dir, &src_dir, opts, &mut scanned)?;

    // 결정적 순서. 같은 입력이면 같은 바이트가 나와야 릴리스를 재현할 수 있다.
    scanned.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));

    let entries: Vec<Entry> = scanned.iter().map(|s| s.entry.clone()).collect();
    path_guard::check_all(&entries)?;

    let total_uncompressed: u64 = entries.iter().map(|e| e.size).sum();
    let index_json = serde_json::to_vec(&Index { entries })
        .map_err(|e| Error::BadIndex(e.to_string()))?;

    let header = Header {
        format_version: FORMAT_VERSION,
        flags: 0,
        total_uncompressed,
        entry_count: scanned.len() as u64,
        index_len: index_json.len() as u64,
    };

    if let Some(parent) = dest.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let out = File::create(dest)?;
    let mut out = BufWriter::new(out);
    header.write_to(&mut out)?;

    let mut enc = liblzma::write::XzEncoder::new(out, opts.preset);
    enc.write_all(&index_json)?;

    let mut done = 0u64;
    let mut buf = vec![0u8; CHUNK];
    progress(0, total_uncompressed);

    for s in &scanned {
        let mut f = BufReader::new(File::open(&s.abs)?);
        let mut written = 0u64;
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            enc.write_all(&buf[..n])?;
            written += n as u64;
            done += n as u64;
            progress(done, total_uncompressed);
        }
        // 스캔과 압축 사이에 파일이 바뀌면 색인과 내용이 어긋난다. 조용히
        // 넘어가면 해제할 때 엉뚱한 파일에서 체크섬이 틀린다.
        if written != s.entry.size {
            return Err(Error::Truncated {
                path: s.entry.path.clone(),
                expected: s.entry.size,
                got: written,
            });
        }
    }

    let out = enc.finish()?;
    let out = out.into_inner().map_err(|e| Error::Io(e.into_error()))?;
    out.sync_all()?;
    let compressed_size = out.metadata()?.len();

    Ok(PackSummary {
        entry_count: scanned.len() as u64,
        total_uncompressed,
        compressed_size,
    })
}

fn scan(root: &Path, dir: &Path, opts: &PackOptions, out: &mut Vec<Scanned>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if opts.exclude.iter().any(|x| x == &name) {
            continue;
        }
        let abs = entry.path();

        // symlink_metadata: 링크를 따라가지 않는다. 따라가면 링크가 가리키는
        // 바깥 파일이 아카이브에 빨려 들어온다.
        let meta = std::fs::symlink_metadata(&abs)?;
        if meta.file_type().is_symlink() {
            return Err(Error::Symlink(abs));
        }

        if meta.is_dir() {
            scan(root, &abs, opts, out)?;
            continue;
        }
        if !meta.is_file() {
            continue; // 소켓, FIFO 등. 게임 설치본에 있을 리 없다.
        }

        let rel = abs.strip_prefix(root).unwrap_or(&abs);
        let path: String = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/");
        path_guard::check(&path)?;

        let (size, sha256) = hash_file(&abs)?;
        out.push(Scanned {
            entry: Entry { path, size, exec: is_exec(&meta), sha256 },
            abs,
        });
    }
    Ok(())
}

fn hash_file(p: &Path) -> Result<(u64, String)> {
    let mut f = BufReader::new(File::open(p)?);
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((size, hex::encode(hasher.finalize())))
}

#[cfg(unix)]
fn is_exec(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

/// Windows에는 실행 비트가 없다. 확장자로 정한다 — Linux용 `.hyp`는 WSL이나
/// Linux에서 패킹되므로 위의 unix 갈래를 타고, 이쪽은 Windows용 `.hyp`를 만들
/// 때만 쓰인다. 거기서 exec 비트는 어차피 해제할 때 무시된다.
#[cfg(not(unix))]
fn is_exec(_meta: &std::fs::Metadata) -> bool {
    false
}
