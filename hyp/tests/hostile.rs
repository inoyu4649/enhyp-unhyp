//! 악의적으로 만들어진 아카이브를 거부하는지 본다.
//!
//! 아카이브를 손으로 조립한다 — `pack()`은 같은 검사를 통과하지 못해서
//! 애초에 이런 파일을 만들어 주지 않기 때문이다. 방어선은 해제 쪽이고,
//! 이 테스트가 그 방어선을 직접 겨눈다.

use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

/// 경로 하나짜리 `.hyp`를 조립한다. 내용은 `body`.
fn craft(dest: &Path, path_in_archive: &str, body: &[u8]) {
    let sha = hex::encode(Sha256::digest(body));
    let index = serde_json::json!({
        "entries": [
            { "path": path_in_archive, "size": body.len(), "exec": false, "sha256": sha }
        ]
    });
    let index = serde_json::to_vec(&index).unwrap();

    let mut header = [0u8; 64];
    header[0..5].copy_from_slice(b"HYPV1");
    header[5] = 1;
    header[8..16].copy_from_slice(&(body.len() as u64).to_le_bytes());
    header[16..24].copy_from_slice(&1u64.to_le_bytes());
    header[24..32].copy_from_slice(&(index.len() as u64).to_le_bytes());

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    let mut enc = liblzma::write::XzEncoder::new(Vec::new(), 9);
    enc.write_all(&index).unwrap();
    enc.write_all(body).unwrap();
    out.extend_from_slice(&enc.finish().unwrap());
    std::fs::write(dest, out).unwrap();
}

/// `dest_dir` 바깥에 무엇이든 생겼으면 실패다.
fn assert_nothing_escaped(sandbox: &Path, dest_dir: &Path) {
    for e in std::fs::read_dir(sandbox).unwrap() {
        let p = e.unwrap().path();
        if p == dest_dir || p.extension().map(|x| x == "hyp").unwrap_or(false) {
            continue;
        }
        panic!("대상 디렉터리 바깥에 파일이 생겼습니다: {}", p.display());
    }
}

fn reject_case(path_in_archive: &str) {
    let tmp = tempfile::tempdir().unwrap();
    let sandbox = tmp.path();
    let archive = sandbox.join("evil.hyp");
    let dest = sandbox.join("out");
    craft(&archive, path_in_archive, b"pwned");

    let err = hyp::extract(&archive, &dest, |_, _| {})
        .expect_err(&format!("{path_in_archive} 를 통과시켰습니다"));
    assert!(
        matches!(err, hyp::Error::UnsafePath { .. }),
        "{path_in_archive} → 기대한 UnsafePath 가 아니라 {err:?}"
    );
    assert_nothing_escaped(sandbox, &dest);
}

#[test]
fn rejects_parent_traversal() {
    reject_case("../escaped.txt");
    reject_case("res/../../escaped.txt");
    reject_case("..");
}

#[test]
fn rejects_absolute_paths() {
    reject_case("/etc/passwd");
    reject_case("C:/Windows/System32/evil.dll");
}

#[test]
fn rejects_backslash_separator() {
    // `\`를 허용하면 Windows에서 디렉터리 구분자가 되어 `/` 기준 검사를 우회한다.
    reject_case(concat!("..", "\\", "escaped.txt"));
    reject_case(concat!("res", "\\", "..", "\\", "..", "\\", "escaped.txt"));
}

#[test]
fn rejects_windows_device_names() {
    reject_case("NUL");
    reject_case("res/CON.png");
}

#[test]
fn rejects_index_disagreeing_with_header() {
    // 색인의 크기 합과 헤더의 total_uncompressed가 어긋나면 진행률이 거짓이
    // 되고, 더 중요하게는 조립된 아카이브라는 신호다.
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("bad.hyp");
    let dest = tmp.path().join("out");

    let body = b"hello";
    let index = serde_json::to_vec(&serde_json::json!({
        "entries": [{ "path": "a.txt", "size": 5, "exec": false,
                      "sha256": hex::encode(Sha256::digest(body)) }]
    })).unwrap();

    let mut header = [0u8; 64];
    header[0..5].copy_from_slice(b"HYPV1");
    header[5] = 1;
    header[8..16].copy_from_slice(&999u64.to_le_bytes()); // ← 거짓말
    header[16..24].copy_from_slice(&1u64.to_le_bytes());
    header[24..32].copy_from_slice(&(index.len() as u64).to_le_bytes());

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    let mut enc = liblzma::write::XzEncoder::new(Vec::new(), 9);
    enc.write_all(&index).unwrap();
    enc.write_all(body).unwrap();
    out.extend_from_slice(&enc.finish().unwrap());
    std::fs::write(&archive, out).unwrap();

    let err = hyp::extract(&archive, &dest, |_, _| {}).expect_err("불일치를 통과시켰습니다");
    assert!(matches!(err, hyp::Error::BadIndex(_)), "got {err:?}");
}

#[test]
fn rejects_tampered_checksum() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("bad.hyp");
    let dest = tmp.path().join("out");

    let index = serde_json::to_vec(&serde_json::json!({
        "entries": [{ "path": "a.txt", "size": 5, "exec": false, "sha256": "0".repeat(64) }]
    })).unwrap();

    let mut header = [0u8; 64];
    header[0..5].copy_from_slice(b"HYPV1");
    header[5] = 1;
    header[8..16].copy_from_slice(&5u64.to_le_bytes());
    header[16..24].copy_from_slice(&1u64.to_le_bytes());
    header[24..32].copy_from_slice(&(index.len() as u64).to_le_bytes());

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    let mut enc = liblzma::write::XzEncoder::new(Vec::new(), 9);
    enc.write_all(&index).unwrap();
    enc.write_all(b"hello").unwrap();
    out.extend_from_slice(&enc.finish().unwrap());
    std::fs::write(&archive, out).unwrap();

    let err = hyp::extract(&archive, &dest, |_, _| {}).expect_err("체크섬 불일치를 통과시켰습니다");
    assert!(matches!(err, hyp::Error::ChecksumMismatch { .. }), "got {err:?}");
}
