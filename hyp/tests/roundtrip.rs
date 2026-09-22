//! 왕복 테스트. 이 크레이트의 계약(FORMAT.md "해제 계약")을 지키는지 본다.

use std::fs;
use std::io::Write;
use std::path::Path;

fn fixture(root: &Path) {
    fs::create_dir_all(root.join("res/ui")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("MOTP.exe"), b"MZ\x00\x00fake executable").unwrap();
    fs::write(root.join("res/ui/logo.png"), vec![0xABu8; 300_000]).unwrap();
    fs::write(root.join("res/empty.dat"), b"").unwrap();
    fs::write(root.join("docs/README.txt"), "한글 내용도 담긴다\n".repeat(50)).unwrap();
    fs::write(root.join("한글 파일.json"), r#"{"ok":true}"#).unwrap();
}

#[test]
fn roundtrip_preserves_content() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let out = tmp.path().join("out");
    let hyp = tmp.path().join("a.hyp");
    fixture(&src);

    let mut seen_total = 0u64;
    let summary = hyp::pack(&src, &hyp, &hyp::PackOptions::default(), |_d, t| seen_total = t).unwrap();
    assert_eq!(summary.entry_count, 5);
    assert_eq!(seen_total, summary.total_uncompressed);

    // 매직 넘버가 맨 앞에 있어야 한다 — 일반 압축 프로그램이 못 알아보는 근거.
    let head = fs::read(&hyp).unwrap();
    assert_eq!(&head[..5], b"HYPV1");
    assert!(hyp::looks_like_hyp(&head));

    let ex = hyp::extract(&hyp, &out, |_, _| {}).unwrap();
    assert_eq!(ex.entry_count, summary.entry_count);

    for rel in ["MOTP.exe", "res/ui/logo.png", "res/empty.dat", "docs/README.txt", "한글 파일.json"] {
        let a = fs::read(src.join(rel)).unwrap();
        let b = fs::read(out.join(rel)).unwrap();
        assert_eq!(a, b, "{rel} 내용이 다릅니다");
    }
}

#[test]
fn read_info_does_not_extract() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let hyp = tmp.path().join("a.hyp");
    fixture(&src);
    hyp::pack(&src, &hyp, &hyp::PackOptions::default(), |_, _| {}).unwrap();

    let info = hyp::read_info(&hyp).unwrap();
    assert_eq!(info.format_version, 1);
    assert_eq!(info.entries.len(), 5);
    // 색인은 경로 순으로 정렬되어 있다 — 릴리스 재현성의 근거.
    let paths: Vec<_> = info.entries.iter().map(|e| e.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
}

#[test]
fn rejects_non_hyp_file() {
    let tmp = tempfile::tempdir().unwrap();
    let fake = tmp.path().join("fake.hyp");
    // 진짜 XZ 파일이지만 HYPV1 헤더가 없다.
    fs::write(&fake, b"\xFD7zXZ\x00 not ours").unwrap();
    let e = hyp::read_info(&fake).unwrap_err();
    assert!(matches!(e, hyp::Error::NotHyp), "got {e:?}");
}

#[test]
fn rejects_corrupted_payload() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let hyp_path = tmp.path().join("a.hyp");
    fixture(&src);
    hyp::pack(&src, &hyp_path, &hyp::PackOptions::default(), |_, _| {}).unwrap();

    // 압축 본문 한가운데를 뒤집는다. XZ 자체의 CRC나 우리 sha256 둘 중
    // 하나에는 반드시 걸려야 한다 — 조용히 통과하면 안 된다.
    let mut bytes = fs::read(&hyp_path).unwrap();
    let mid = bytes.len() / 2;
    bytes[mid] ^= 0xFF;
    let mut f = fs::File::create(&hyp_path).unwrap();
    f.write_all(&bytes).unwrap();
    drop(f);

    let out = tmp.path().join("out");
    assert!(hyp::extract(&hyp_path, &out, |_, _| {}).is_err(), "손상된 아카이브가 통과했습니다");
}

#[cfg(unix)]
#[test]
fn preserves_exec_bit() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let out = tmp.path().join("out");
    let hyp_path = tmp.path().join("a.hyp");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("MOTP"), b"#!/bin/sh\n").unwrap();
    fs::write(src.join("data.json"), b"{}").unwrap();
    fs::set_permissions(src.join("MOTP"), fs::Permissions::from_mode(0o755)).unwrap();

    hyp::pack(&src, &hyp_path, &hyp::PackOptions::default(), |_, _| {}).unwrap();
    hyp::extract(&hyp_path, &out, |_, _| {}).unwrap();

    // 이게 깨지면 Linux에서 게임이 조용히 실행되지 않는다.
    let m = fs::metadata(out.join("MOTP")).unwrap().permissions().mode();
    assert_eq!(m & 0o111, 0o111, "실행 비트가 사라졌습니다: {m:o}");
    let d = fs::metadata(out.join("data.json")).unwrap().permissions().mode();
    assert_eq!(d & 0o111, 0, "데이터 파일에 실행 비트가 붙었습니다: {d:o}");
}
