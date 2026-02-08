#[cfg(test)]
mod cmpf_tests {
    use crate::compare::ExitStatus;
    use crate::models::{HashAlgo, OutputFormat, SymlinkMode};
    use crate::snapshot::{SnapshotConfig, VerifyConfig, create_snapshot, verify_snapshot};
    use crate::sync::{SyncConfig, run_sync};
    use crate::utils::{collect_files, compute_hashes};
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_compute_hashes_empty_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("empty.txt");
        File::create(&file_path).unwrap();

        let res = compute_hashes(&file_path, HashAlgo::Blake3).unwrap();
        assert!(res.blake3.is_some());
        assert!(res.sha256.is_none());
    }

    #[test]
    fn test_compute_hashes_content() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("hello.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "hello world").unwrap();

        let res_b3 = compute_hashes(&file_path, HashAlgo::Blake3).unwrap();
        let res_sha = compute_hashes(&file_path, HashAlgo::Sha256).unwrap();

        assert!(res_b3.blake3.is_some());
        assert!(res_sha.sha256.is_some());
    }

    #[test]
    fn test_collect_files_basic() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();

        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(sub.join("file2.txt")).unwrap();

        let (files, errors) = collect_files(
            dir.path(),
            None,
            false,
            false,
            &None,
            &None,
            SymlinkMode::Ignore,
        )
        .unwrap();

        assert_eq!(files.len(), 2);
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_collect_files_recursive_off() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();

        File::create(dir.path().join("file1.txt")).unwrap();
        File::create(sub.join("file2.txt")).unwrap();

        let (files, _) = collect_files(
            dir.path(),
            None,
            true, // no_recursive
            false,
            &None,
            &None,
            SymlinkMode::Ignore,
        )
        .unwrap();

        assert_eq!(files.len(), 1);
    }

    #[test]
    fn test_snapshot_lifecycle() {
        let dir = tempdir().unwrap();
        let folder = dir.path().join("data");
        fs::create_dir(&folder).unwrap();
        let file_path = folder.join("file.txt");
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "content").unwrap();

        let snapshot_path = dir.path().join("snap.json");

        // Create snapshot
        create_snapshot(SnapshotConfig {
            folder: folder.clone(),
            output: Some(snapshot_path.clone()),
            algo: HashAlgo::Blake3,
            depth: None,
            no_recursive: false,
            hidden: false,
            types: None,
            ignore: None,
            symlinks: SymlinkMode::Ignore,
            threads: None,
        })
        .unwrap();

        assert!(snapshot_path.exists());

        // Verify snapshot (MATCH)
        let status = verify_snapshot(VerifyConfig {
            folder: folder.clone(),
            snapshot_path: snapshot_path.clone(),
            threads: None,
            output_format: OutputFormat::Txt,
            verbose: false,
        })
        .unwrap();
        assert_eq!(status, ExitStatus::Success);

        // Modify file and verify (DIFF)
        let mut file = File::create(&file_path).unwrap();
        writeln!(file, "changed content").unwrap();
        let status = verify_snapshot(VerifyConfig {
            folder: folder.clone(),
            snapshot_path: snapshot_path.clone(),
            threads: None,
            output_format: OutputFormat::Txt,
            verbose: false,
        })
        .unwrap();
        assert_eq!(status, ExitStatus::Diff);
    }

    #[test]
    fn test_sync_basic() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        fs::create_dir(&src).unwrap();
        fs::create_dir(&dst).unwrap();

        let file1 = src.join("file1.txt");
        let mut f = File::create(&file1).unwrap();
        writeln!(f, "content 1").unwrap();

        // Run sync
        run_sync(SyncConfig {
            source: src.clone(),
            destination: dst.clone(),
            dry_run: false,
            delete_extraneous: true,
            no_delete: false,
            algo: HashAlgo::Blake3,
            depth: None,
            no_recursive: false,
            symlinks: SymlinkMode::Ignore,
            hidden: false,
            types: None,
            ignore: None,
            threads: None,
        })
        .unwrap();

        assert!(dst.join("file1.txt").exists());

        let content = fs::read_to_string(dst.join("file1.txt")).unwrap();
        assert_eq!(content, "content 1\n");
    }

    #[test]
    fn test_collect_files_hidden() {
        let dir = tempdir().unwrap();
        File::create(dir.path().join("normal.txt")).unwrap();
        File::create(dir.path().join(".hidden")).unwrap();

        // Hidden off
        let (files, _) = collect_files(
            dir.path(),
            None,
            false,
            false,
            &None,
            &None,
            SymlinkMode::Ignore,
        )
        .unwrap();
        assert_eq!(files.len(), 1);

        // Hidden on
        let (files, _) = collect_files(
            dir.path(),
            None,
            false,
            true,
            &None,
            &None,
            SymlinkMode::Ignore,
        )
        .unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_collect_files_types() {
        let dir = tempdir().unwrap();
        File::create(dir.path().join("test.txt")).unwrap();
        File::create(dir.path().join("test.rs")).unwrap();

        let types = Some(vec!["txt".to_string()]);
        let (files, _) = collect_files(
            dir.path(),
            None,
            false,
            false,
            &types,
            &None,
            SymlinkMode::Ignore,
        )
        .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.extension().unwrap(), "txt");
    }

    #[test]
    fn test_collect_files_ignore() {
        let dir = tempdir().unwrap();
        File::create(dir.path().join("keep.txt")).unwrap();
        File::create(dir.path().join("skip.tmp")).unwrap();

        let ignore = Some(vec!["*.tmp".to_string()]);
        let (files, _) = collect_files(
            dir.path(),
            None,
            false,
            false,
            &None,
            &ignore,
            SymlinkMode::Ignore,
        )
        .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path.file_name().unwrap(), "keep.txt");
    }

    #[test]
    fn test_sync_dry_run() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("src");
        let dst = dir.path().join("dst");
        fs::create_dir(&src).unwrap();
        fs::create_dir(&dst).unwrap();

        File::create(src.join("file.txt")).unwrap();

        run_sync(SyncConfig {
            source: src,
            destination: dst.clone(),
            dry_run: true, // DRY RUN
            delete_extraneous: false,
            no_delete: false,
            algo: HashAlgo::Blake3,
            depth: None,
            no_recursive: false,
            symlinks: SymlinkMode::Ignore,
            hidden: false,
            types: None,
            ignore: None,
            threads: None,
        })
        .unwrap();

        assert!(!dst.join("file.txt").exists());
    }
}
