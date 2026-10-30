#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::{Path, PathBuf};
use std::{env, fs, io::Write as _, sync::Mutex};

#[cfg(any(target_os = "macos", target_os = "windows"))]
use crate::diagnostic::code::CF_ARTIFACT_CASE_INSENSITIVE_COLLISION;
use crate::{
    ConfigForgeError, Result,
    diagnostic::code::{
        CF_ARTIFACT_PATH_COLLISION, CF_ARTIFACT_PATH_ESCAPE, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION,
        CF_ARTIFACT_WRITE_STATE,
    },
};

use super::{path::temp_path_for, writer::ArtifactWriteTarget, *};

#[cfg(unix)]
use crate::diagnostic::code::CF_ARTIFACT_HARDLINK_ALIAS_COLLISION;

static CURRENT_DIR_LOCK: Mutex<()> = Mutex::new(());

#[cfg(target_os = "windows")]
use crate::path_util::lexical_normalize_path;

#[test]
fn reserved_paths_are_checked_before_any_direct_write() {
    let temp = tempfile::tempdir().expect("temp dir");
    let existing = temp.path().join("existing.txt");
    fs::write(&existing, "original").expect("existing artifact");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    let error = sink
        .reserve_paths(vec![existing.clone(), temp.path().join("./existing.txt")])
        .expect_err("duplicate reservation");
    assert!(error.to_string().contains("multiple artifacts"));
    assert!(sink.write_text(&existing, "replacement").is_err());
    assert_eq!(fs::read_to_string(existing).unwrap(), "original");
}

#[test]
fn reserved_sink_rejects_unplanned_and_unfinished_artifacts() {
    for mode in [
        ArtifactWriteMode::Direct,
        ArtifactWriteMode::TempCommit,
        ArtifactWriteMode::DryRun,
    ] {
        let temp = tempfile::tempdir().expect("temp dir");
        let planned = temp.path().join("out/planned.txt");
        let sink = ArtifactWriter::new(mode).open_sink();
        sink.reserve_paths(vec![planned.clone()]).expect("reserve");
        assert!(!planned.parent().unwrap().exists());
        let error = sink
            .create(temp.path().join("out/unplanned.txt"))
            .expect_err("unplanned path");
        assert_artifact_state_diagnostic(error, "unplanned.txt");
        assert!(!planned.parent().unwrap().exists());
        assert!(sink.finish().is_err());

        let sink = ArtifactWriter::new(mode).open_sink();
        sink.reserve_paths(vec![planned]).expect("reserve");
        assert_artifact_state_diagnostic(
            sink.finish().expect_err("unused reservation"),
            "planned.txt",
        );
    }
}

#[test]
fn reserved_paths_can_be_written_once_in_all_modes() {
    for mode in [
        ArtifactWriteMode::Direct,
        ArtifactWriteMode::TempCommit,
        ArtifactWriteMode::DryRun,
    ] {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("out/file.txt");
        let sink = ArtifactWriter::new(mode).open_sink();
        sink.reserve_paths(vec![path.clone()]).expect("reserve");
        sink.write_text(&path, "content").expect("planned write");
        assert_eq!(sink.finish().expect("finish").entries.len(), 1);
        assert_eq!(path.exists(), mode != ArtifactWriteMode::DryRun);

        let sink = ArtifactWriter::new(mode).open_sink();
        sink.reserve_paths(vec![path.clone()]).expect("reserve");
        sink.write_text(&path, "content")
            .expect("planned overwrite");
        assert!(sink.create(&path).is_err());
        assert!(sink.finish().is_err());
    }
}

#[test]
fn failed_path_reservation_cannot_be_ignored() {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("project");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct)
        .with_output_root(&root)
        .open_sink();
    assert!(
        sink.reserve_paths(vec!["valid.txt".into(), "../escape.txt".into()])
            .is_err()
    );
    assert!(sink.write_text("valid.txt", "content").is_err());
    assert!(sink.finish().is_err());
    assert!(!root.exists());
}

#[cfg(unix)]
#[test]
fn reservations_reject_filesystem_aliases_without_overwriting() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().expect("temp dir");
    let real = temp.path().join("real");
    fs::create_dir(&real).unwrap();
    let alias = temp.path().join("alias");
    symlink(&real, &alias).unwrap();
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    let error = sink
        .reserve_paths(vec![real.join("new.txt"), alias.join("new.txt")])
        .expect_err("symlink alias");
    assert_artifact_diagnostic(error, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION, "new.txt");
    assert!(!real.join("new.txt").exists());

    let first = real.join("first.txt");
    let second = real.join("second.txt");
    fs::write(&first, "original").unwrap();
    fs::hard_link(&first, &second).unwrap();
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    let error = sink
        .reserve_paths(vec![first.clone(), second])
        .expect_err("hardlink alias");
    assert_artifact_diagnostic(error, CF_ARTIFACT_HARDLINK_ALIAS_COLLISION, "second.txt");
    assert_eq!(fs::read_to_string(first).unwrap(), "original");
}

fn assert_artifact_diagnostic(error: ConfigForgeError, code: &str, current_contains: &str) {
    let ConfigForgeError::ValidationReport(report) = error else {
        panic!("expected structured artifact diagnostic");
    };
    let diagnostic = &report.diagnostics[0];
    assert_eq!(diagnostic.code, code);
    assert_eq!(diagnostic.source.as_deref(), Some("artifact.writer"));
    assert!(
        diagnostic
            .current_value
            .as_deref()
            .is_some_and(|current| current.replace('\\', "/").contains(current_contains)),
        "expected current_value to contain `{current_contains}`, got {:?}",
        diagnostic.current_value
    );
    assert!(diagnostic.expected.is_some());
}

fn assert_artifact_state_diagnostic(error: ConfigForgeError, current_contains: &str) {
    assert_artifact_diagnostic(error, CF_ARTIFACT_WRITE_STATE, current_contains);
}

#[test]
fn dry_run_returns_sorted_entries_without_writing() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let nested = temp_dir.path().join("out/./nested/../b.txt");
    let first = temp_dir.path().join("out/a.txt");

    let result = ArtifactWriter::new(ArtifactWriteMode::DryRun)
        .write(vec![
            Artifact::text(&nested, "b"),
            Artifact::text(&first, "a"),
        ])
        .expect("dry-run should succeed");

    assert_eq!(result.mode, ArtifactWriteMode::DryRun);
    assert_eq!(result.entries.len(), 2);
    assert_eq!(result.entries[0].path, first);
    assert_eq!(result.entries[0].bytes, 1);
    assert!(!result.entries[0].written);
    assert!(!temp_dir.path().join("out").exists());
}

#[test]
fn direct_writes_text_and_binary_content() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let text_path = temp_dir.path().join("text/generated.rs");
    let binary_path = temp_dir.path().join("data/items.msgpack");

    let result = ArtifactWriter::new(ArtifactWriteMode::Direct)
        .write(vec![
            Artifact::text(&text_path, "pub struct Item;"),
            Artifact::binary(&binary_path, vec![1, 2, 3]),
        ])
        .expect("direct write should succeed");

    assert_eq!(result.mode, ArtifactWriteMode::Direct);
    assert!(result.entries.iter().all(|entry| entry.written));
    assert_eq!(
        fs::read_to_string(text_path).expect("text artifact"),
        "pub struct Item;"
    );
    assert_eq!(
        fs::read(binary_path).expect("binary artifact"),
        vec![1, 2, 3]
    );
}

#[test]
fn pure_file_name_path_does_not_try_to_create_empty_parent_dir() {
    let _guard = CURRENT_DIR_LOCK.lock().expect("current dir lock");
    let original_dir = env::current_dir().expect("current dir");
    let temp_dir = tempfile::tempdir().expect("temp dir");
    env::set_current_dir(temp_dir.path()).expect("set current dir");

    let result = ArtifactWriter::new(ArtifactWriteMode::Direct).write(vec![Artifact::text(
        "generated.rs",
        "pub struct Generated;",
    )]);

    env::set_current_dir(original_dir).expect("restore current dir");
    result.expect("pure file name artifact should write");
    assert_eq!(
        fs::read_to_string(temp_dir.path().join("generated.rs")).expect("written artifact"),
        "pub struct Generated;"
    );
}

#[test]
fn temp_commit_writes_artifacts() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/generated.rs");

    ArtifactWriter::new(ArtifactWriteMode::TempCommit)
        .write(vec![Artifact::text(&path, "pub struct Generated;")])
        .expect("temp commit should succeed");

    assert_eq!(
        fs::read_to_string(path).expect("written artifact"),
        "pub struct Generated;"
    );
}

#[test]
fn temp_commit_replaces_existing_final_file() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/generated.rs");
    fs::create_dir_all(path.parent().expect("out dir")).expect("create out dir");
    fs::write(&path, "old content").expect("seed existing artifact");

    let result = ArtifactWriter::new(ArtifactWriteMode::TempCommit)
        .write(vec![Artifact::text(&path, "new content")])
        .expect("temp commit should replace existing final file");

    assert_eq!(result.mode, ArtifactWriteMode::TempCommit);
    assert_eq!(result.entries[0].path, path);
    assert_eq!(result.entries[0].bytes, "new content".len());
    assert_eq!(
        fs::read_to_string(&result.entries[0].path).expect("written artifact"),
        "new content"
    );
}

#[test]
fn sink_direct_stream_writes_content_and_entries() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let first = temp_dir.path().join("out/b.txt");
    let second = temp_dir.path().join("out/a.txt");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();

    let mut stream = sink.create(&first).expect("create first stream");
    stream.write_all(b"b").expect("write first");
    let first_entry = stream.finish().expect("finish first");
    assert_eq!(first_entry.bytes, 1);
    sink.write_text(&second, "aa").expect("write second");

    let result = sink.finish().expect("finish sink");

    assert_eq!(result.mode, ArtifactWriteMode::Direct);
    assert_eq!(result.entries[0].path, second);
    assert_eq!(result.entries[0].bytes, 2);
    assert_eq!(result.entries[1].path, first);
    assert_eq!(fs::read_to_string(first).expect("first artifact"), "b");
    assert_eq!(fs::read_to_string(second).expect("second artifact"), "aa");
}

#[test]
fn sink_dry_run_counts_stream_bytes_without_writing() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/generated.rs");
    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

    let mut stream = sink.create(&path).expect("create dry-run stream");
    stream.write_all(b"abc").expect("write dry-run stream");
    stream.write_all(b"de").expect("write dry-run stream");
    stream.finish().expect("finish dry-run stream");
    let result = sink.finish().expect("finish sink");

    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].path, path);
    assert_eq!(result.entries[0].bytes, 5);
    assert!(!result.entries[0].written);
    assert!(!temp_dir.path().join("out").exists());
}

struct TestProducer {
    plan: ArtifactPlan,
    content: &'static [u8],
}

impl ArtifactProducer for TestProducer {
    fn plan(&self) -> &ArtifactPlan {
        &self.plan
    }

    fn produce(self, stream: &mut ArtifactWriteStream) -> Result<()> {
        stream.write_all_artifact(self.content)
    }
}

#[test]
fn sink_writes_artifact_producer() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/producer.bin");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    let producer = TestProducer {
        plan: ArtifactPlan::new(&path),
        content: &[1, 2, 3, 4],
    };

    let entry = sink.write_producer(producer).expect("producer write");
    let result = sink.finish().expect("finish sink");

    assert_eq!(entry.bytes, 4);
    assert_eq!(result.entries[0].path, path);
    assert_eq!(fs::read(path).expect("producer content"), vec![1, 2, 3, 4]);
}

#[test]
fn sink_rejects_duplicate_path_when_stream_is_open() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let first = temp_dir.path().join("out/data/items.json");
    let duplicate = temp_dir.path().join("out/data/./nested/../items.json");
    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

    let stream = sink.create(&first).expect("create first stream");
    let error = sink.create(&duplicate).unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_COLLISION, "items.json");
    stream.finish().expect("finish first stream");
    let error = sink.finish().unwrap_err();
    assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_COLLISION, "items.json");
}

#[test]
fn sink_finish_fails_after_ignored_duplicate_create_error() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/data/items.json");
    let duplicate = temp_dir.path().join("out/data/./items.json");
    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

    let stream = sink.create(&path).expect("create first stream");
    let _ignored = sink.create(&duplicate).unwrap_err();
    stream.finish().expect("finish first stream");

    let error = sink.finish().unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_COLLISION, "items.json");
}

#[test]
fn sink_finish_fails_after_ignored_open_file_error() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let blocked_parent = temp_dir.path().join("out-as-file");
    fs::write(&blocked_parent, "not a directory").expect("blocked parent file");
    let path = blocked_parent.join("generated.rs");
    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();

    let _ignored = sink.create(&path).unwrap_err();

    let error = sink.finish().unwrap_err();

    assert_artifact_state_diagnostic(error, "generated.rs");
}

#[test]
fn sink_finish_preserves_flush_error_after_ignored_stream_finish_error() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/generated.rs");
    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun).open_sink();

    let mut stream = sink.create(&path).expect("create stream");
    stream.target = ArtifactWriteTarget::FailFlush;
    stream.write_all(b"content").expect("write content");

    let immediate_error = stream.finish().unwrap_err();
    assert!(matches!(
        immediate_error,
        ConfigForgeError::WriteFile { path: error_path, source }
            if error_path == path && source.to_string().contains("injected flush failure")
    ));

    let finish_error = sink.finish().unwrap_err();

    assert_artifact_state_diagnostic(finish_error, "generated.rs");
}

#[test]
fn sink_temp_commit_defers_visibility_until_finish() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/generated.rs");
    let sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();

    let mut stream = sink.create(&path).expect("create temp stream");
    stream
        .write_all(b"pub struct Generated;")
        .expect("write temp");
    stream.finish().expect("finish temp stream");
    assert!(!path.exists());

    let result = sink.finish().expect("finish temp sink");

    assert_eq!(result.entries[0].path, path);
    assert_eq!(
        fs::read_to_string(path).expect("written artifact"),
        "pub struct Generated;"
    );
}

#[test]
fn sink_temp_commit_drop_cleans_finished_temp_files_without_finish() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/generated.rs");

    {
        let sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();
        sink.write_text(&path, "pub struct Generated;")
            .expect("temp artifact should write");
        assert!(!path.exists());

        let temp_files = fs::read_dir(path.parent().expect("out dir"))
            .expect("out dir should exist")
            .collect::<std::result::Result<Vec<_>, _>>()
            .expect("read temp dir");
        assert_eq!(temp_files.len(), 1);
    }

    assert!(!path.exists());
    let temp_files = fs::read_dir(path.parent().expect("out dir"))
        .expect("out dir should exist")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("read temp dir");
    assert!(temp_files.is_empty());
}

#[test]
fn sink_temp_commit_finish_clears_temp_registry_before_late_clone_drop() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/generated.rs");
    let temp_path = temp_path_for(&path, 0);

    let retained_sink_clone = {
        let sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();
        let retained_sink_clone = sink.clone();
        sink.write_text(&path, "first")
            .expect("first temp artifact should write");
        sink.finish().expect("first sink should commit");
        retained_sink_clone
    };
    assert_eq!(fs::read_to_string(&path).expect("first artifact"), "first");

    let second_sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();
    let mut second_stream = second_sink.create(&path).expect("create second stream");
    second_stream.write_all(b"second").expect("write second");
    second_stream.finish().expect("finish second stream");
    assert!(temp_path.exists());

    drop(retained_sink_clone);

    assert!(
        temp_path.exists(),
        "late drop of a previously finished sink clone must not remove a new sink temp file"
    );
    second_sink.finish().expect("second sink should commit");
    assert_eq!(
        fs::read_to_string(&path).expect("second artifact"),
        "second"
    );
}

#[test]
fn sink_temp_commit_fails_when_stream_is_dropped_without_finish() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/generated.rs");
    let sink = ArtifactWriter::new(ArtifactWriteMode::TempCommit).open_sink();

    {
        let mut stream = sink.create(&path).expect("create temp stream");
        stream.write_all(b"partial").expect("write temp stream");
    }

    let error = sink.finish().unwrap_err();

    assert_artifact_state_diagnostic(error, "generated.rs");
    assert!(!path.exists());
    let out_dir = temp_dir.path().join("out");
    let temp_files = fs::read_dir(out_dir)
        .expect("out dir")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("read temp dir");
    assert!(temp_files.is_empty());
}

#[test]
fn writer_vec_api_uses_sink_and_matches_manual_sink_result() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let writer_path = temp_dir.path().join("writer/out.txt");
    let sink_path = temp_dir.path().join("sink/out.txt");

    let writer_result = ArtifactWriter::new(ArtifactWriteMode::Direct)
        .write(vec![Artifact::text(&writer_path, "same")])
        .expect("writer api");

    let sink = ArtifactWriter::new(ArtifactWriteMode::Direct).open_sink();
    sink.write_text(&sink_path, "same").expect("sink write");
    let sink_result = sink.finish().expect("sink finish");

    assert_eq!(writer_result.entries[0].bytes, sink_result.entries[0].bytes);
    assert_eq!(
        writer_result.entries[0].written,
        sink_result.entries[0].written
    );
    assert_eq!(
        fs::read_to_string(writer_path).expect("writer file"),
        "same"
    );
    assert_eq!(fs::read_to_string(sink_path).expect("sink file"), "same");
}

#[test]
fn direct_stream_writes_large_artifact_through_standard_writer() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("out/large.bin");

    let result = ArtifactWriter::new(ArtifactWriteMode::Direct)
        .write(vec![Artifact::binary(&path, vec![1, 2, 3, 4])])
        .expect("large artifact should write");

    assert_eq!(result.entries[0].bytes, 4);
    assert_eq!(fs::read(path).expect("written artifact"), vec![1, 2, 3, 4]);
}

#[test]
fn temp_commit_writes_multiple_artifacts_through_standard_writer() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let first = temp_dir.path().join("out/a.txt");
    let second = temp_dir.path().join("out/b.txt");

    let result = ArtifactWriter::new(ArtifactWriteMode::TempCommit)
        .write(vec![
            Artifact::text(&second, "bb"),
            Artifact::text(&first, "aa"),
        ])
        .expect("bounded temp commit should succeed");

    assert_eq!(result.entries[0].path, first);
    assert_eq!(result.entries[1].path, second);
    assert_eq!(fs::read_to_string(first).expect("first artifact"), "aa");
    assert_eq!(fs::read_to_string(second).expect("second artifact"), "bb");
}

#[test]
fn duplicate_normalized_paths_are_rejected_before_writing() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let first = temp_dir.path().join("out/data/items.json");
    let second = temp_dir.path().join("out/data/./nested/../items.json");

    let error = ArtifactWriter::new(ArtifactWriteMode::Direct)
        .write(vec![
            Artifact::binary(first, Vec::new()),
            Artifact::text(second, "conflict"),
        ])
        .unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_COLLISION, "items.json");
    assert!(!temp_dir.path().join("out").exists());
}

#[test]
fn output_root_rejects_absolute_path_escape() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let root = temp_dir.path().join("project");
    let outside = temp_dir.path().join("outside/generated.rs");

    let error = ArtifactWriter::new(ArtifactWriteMode::Direct)
        .with_output_root(&root)
        .write(vec![Artifact::text(&outside, "escape")])
        .unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_ESCAPE, "outside/generated.rs");
    assert!(!outside.exists());
}

#[test]
fn output_root_rejects_parent_escape() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let root = temp_dir.path().join("project");

    let error = ArtifactWriter::new(ArtifactWriteMode::DryRun)
        .with_output_root(&root)
        .write(vec![Artifact::text("../outside/generated.rs", "escape")])
        .unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_ESCAPE, "../outside/generated.rs");
}

#[cfg(unix)]
#[test]
fn output_root_detects_symlink_alias_conflict() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let root = temp_dir.path().join("project");
    let out = root.join("out");
    let alias = root.join("out_alias");
    fs::create_dir_all(&out).expect("create out dir");
    std::os::unix::fs::symlink(&out, &alias).expect("create symlink alias");

    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
        .with_output_root(&root)
        .open_sink();
    sink.write_text(out.join("items.json"), "first")
        .expect("first path should plan");
    let error = sink
        .write_text(alias.join("items.json"), "second")
        .unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION, "items.json");
}

#[cfg(unix)]
#[test]
fn output_root_rejects_existing_symlink_target_escape() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let root = temp_dir.path().join("project");
    let outside = temp_dir.path().join("outside.txt");
    let symlink_path = root.join("out/generated.rs");
    fs::create_dir_all(symlink_path.parent().expect("out dir")).expect("create out dir");
    fs::write(&outside, "outside").expect("write outside file");
    std::os::unix::fs::symlink(&outside, &symlink_path).expect("create symlink file");

    let error = ArtifactWriter::new(ArtifactWriteMode::Direct)
        .with_output_root(&root)
        .write(vec![Artifact::text(&symlink_path, "new")])
        .unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_PATH_ESCAPE, "generated.rs");
    assert_eq!(
        fs::read_to_string(outside).expect("outside file"),
        "outside"
    );
}

#[cfg(unix)]
#[test]
fn output_root_detects_existing_hardlink_alias_conflict() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let root = temp_dir.path().join("project");
    let first = root.join("out/a.txt");
    let second = root.join("out/b.txt");
    fs::create_dir_all(first.parent().expect("out dir")).expect("create out dir");
    fs::write(&first, "same inode").expect("write first hardlink target");
    fs::hard_link(&first, &second).expect("create hardlink alias");

    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
        .with_output_root(&root)
        .open_sink();
    sink.write_text(&first, "first")
        .expect("first path should plan");
    let error = sink.write_text(&second, "second").unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_HARDLINK_ALIAS_COLLISION, "b.txt");
}

#[cfg(target_os = "macos")]
#[test]
fn output_root_detects_case_insensitive_collision_on_default_macos_volume() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    if !directory_is_case_insensitive(temp_dir.path()) {
        eprintln!("skipping macOS case-insensitive collision test: temp volume is case-sensitive");
        return;
    }
    assert_case_alias_conflict(temp_dir.path().join("project"));
}

#[cfg(windows)]
#[test]
fn output_root_detects_case_insensitive_collision_on_windows() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    assert_case_alias_conflict(temp_dir.path().join("project"));
}

#[cfg(any(target_os = "macos", windows))]
fn assert_case_alias_conflict(root: PathBuf) {
    let reserved_sink = ArtifactWriter::new(ArtifactWriteMode::Direct)
        .with_output_root(&root)
        .open_sink();
    let error = reserved_sink
        .reserve_paths(vec!["Out/items.json".into(), "out/items.json".into()])
        .expect_err("case alias reservation");
    assert_artifact_diagnostic(error, CF_ARTIFACT_CASE_INSENSITIVE_COLLISION, "items.json");
    assert!(!root.exists());
    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
        .with_output_root(&root)
        .open_sink();
    sink.write_text("Out/items.json", "first")
        .expect("first path should plan");
    let error = sink.write_text("out/items.json", "second").unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_CASE_INSENSITIVE_COLLISION, "items.json");
}

#[cfg(target_os = "macos")]
fn directory_is_case_insensitive(directory: &Path) -> bool {
    let probe = directory.join("configforge_case_probe");
    let alias = directory.join("CONFIGFORGE_CASE_PROBE");
    fs::write(&probe, "probe").expect("write case probe");
    alias.exists()
}

#[cfg(windows)]
#[test]
fn output_root_detects_windows_symlink_alias_conflict() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let root = temp_dir.path().join("project");
    let out = root.join("out");
    let alias = root.join("out_alias");
    fs::create_dir_all(&out).expect("create out dir");
    if !try_create_windows_symlink_dir(&out, &alias) {
        return;
    }

    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
        .with_output_root(&root)
        .open_sink();
    sink.write_text(out.join("items.json"), "first")
        .expect("first path should plan");
    let error = sink
        .write_text(alias.join("items.json"), "second")
        .unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION, "items.json");
}

#[cfg(windows)]
#[test]
fn output_root_detects_windows_junction_alias_conflict() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let root = temp_dir.path().join("project");
    let out = root.join("out");
    let alias = root.join("out_junction");
    fs::create_dir_all(&out).expect("create out dir");
    if !try_create_windows_junction(&out, &alias) {
        return;
    }

    let sink = ArtifactWriter::new(ArtifactWriteMode::DryRun)
        .with_output_root(&root)
        .open_sink();
    sink.write_text(out.join("items.json"), "first")
        .expect("first path should plan");
    let error = sink
        .write_text(alias.join("items.json"), "second")
        .unwrap_err();

    assert_artifact_diagnostic(error, CF_ARTIFACT_SYMLINK_ALIAS_COLLISION, "items.json");
}

#[cfg(windows)]
#[test]
fn windows_lexical_normalization_preserves_drive_verbatim_and_unc_prefixes() {
    let cases = [
        (
            r"C:\Project\out\..\Data\items.json",
            r"C:\Project\Data\items.json",
        ),
        (
            r"\\?\C:\Project\out\..\Data\items.json",
            r"\\?\C:\Project\Data\items.json",
        ),
        (
            r"\\server\share\Project\out\..\Data\items.json",
            r"\\server\share\Project\Data\items.json",
        ),
    ];

    for (input, expected) in cases {
        assert_eq!(
            lexical_normalize_path(Path::new(input)),
            PathBuf::from(expected)
        );
    }
}

#[cfg(windows)]
fn try_create_windows_symlink_dir(target: &Path, link: &Path) -> bool {
    match std::os::windows::fs::symlink_dir(target, link) {
        Ok(()) => true,
        Err(error) => {
            eprintln!(
                "skipping Windows symlink alias test: could not create directory symlink `{}` -> `{}`: {error}",
                link.display(),
                target.display()
            );
            false
        }
    }
}

#[cfg(windows)]
fn try_create_windows_junction(target: &Path, link: &Path) -> bool {
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output();
    match output {
        Ok(output) if output.status.success() => true,
        Ok(output) => {
            eprintln!(
                "skipping Windows junction alias test: mklink /J failed with status {}: {}{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            false
        }
        Err(error) => {
            eprintln!("skipping Windows junction alias test: could not run mklink /J: {error}");
            false
        }
    }
}

#[test]
fn temp_commit_reports_rename_failure_and_cleans_temp_file() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let root = temp_dir.path().join("project");
    let path = root.join("out/generated.rs");
    fs::create_dir_all(&path).expect("create final path as directory");

    let error = ArtifactWriter::new(ArtifactWriteMode::TempCommit)
        .with_output_root(&root)
        .write(vec![Artifact::text(&path, "content")])
        .unwrap_err();

    assert!(
        matches!(error, ConfigForgeError::WriteFile { path: error_path, .. } if error_path == path)
    );
    let temp_files = fs::read_dir(path.parent().expect("out dir"))
        .expect("read out dir")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect out dir");
    assert!(temp_files.iter().all(|entry| entry.path() == path));
}
