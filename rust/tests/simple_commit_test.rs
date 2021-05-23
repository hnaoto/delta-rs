extern crate chrono;
extern crate deltalake;
extern crate utime;

#[cfg(feature = "s3")]
#[allow(dead_code)]
mod s3_common;

#[allow(dead_code)]
mod fs_common;

use std::collections::HashMap;

use deltalake::{action, DeltaTransactionError};

#[cfg(feature = "s3")]
mod simple_commit_s3 {
    use super::*;

    #[cfg(feature = "dynamodb")]
    #[tokio::test]
    async fn test_two_commits_s3() {
        let path = "s3://deltars/simple_commit_rw1";
        s3_common::setup_dynamodb("concurrent_writes");
        prepare_s3(path).await;

        test_two_commits(path).await.unwrap();
    }

    #[cfg(not(feature = "dynamodb"))]
    #[tokio::test]
    async fn test_two_commits_s3_fails_with_no_lock() {
        use deltalake::{StorageError, TransactionCommitAttemptError};

        let path = "s3://deltars/simple_commit_rw2";
        prepare_s3(path).await;

        let result = test_two_commits(path).await;
        if let Err(DeltaTransactionError::TransactionCommitAttempt { ref inner }) = result {
            if let TransactionCommitAttemptError::Storage { source } = inner {
                if let StorageError::S3Generic(err) = source {
                    assert_eq!(err, "dynamodb locking is not enabled");
                    return;
                }
            }
        }

        result.unwrap();

        panic!("S3 commit without dynamodb locking is expected to fail")
    }

    async fn prepare_s3(path: &str) {
        let delta_log = format!("{}/_delta_log", path);
        s3_common::cleanup_dir_except(&delta_log, vec!["00000000000000000000.json".to_string()])
            .await;
    }
}

mod simple_commit_fs {
    // Tests are run serially to allow usage of the same local fs directory.
    use serial_test::serial;

    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_two_commits_fs() {
        prepare_fs();
        test_two_commits("./tests/data/simple_commit")
            .await
            .unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn test_commit_version_succeeds_if_version_does_not_exist() {
        prepare_fs();

        let table_path = "./tests/data/simple_commit";
        let mut table = deltalake::open_table(table_path).await.unwrap();

        assert_eq!(0, table.version);
        assert_eq!(0, table.get_files().len());

        let mut tx1 = table.create_transaction(None);
        tx1.add_actions(tx1_actions());
        let result = tx1.commit_version(1, None).await.unwrap();

        assert_eq!(1, result);
        assert_eq!(1, table.version);
        assert_eq!(2, table.get_files().len());
    }

    #[tokio::test]
    #[serial]
    async fn test_commit_version_fails_if_version_exists() {
        prepare_fs();

        let table_path = "./tests/data/simple_commit";
        let mut table = deltalake::open_table(table_path).await.unwrap();

        assert_eq!(0, table.version);
        assert_eq!(0, table.get_files().len());

        let mut tx1 = table.create_transaction(None);
        tx1.add_actions(tx1_actions());
        let _ = tx1.commit_version(1, None).await.unwrap();

        let mut tx2 = table.create_transaction(None);
        tx2.add_actions(tx2_actions());
        // we already committed version 1 - this should fail and return error for caller to handle.
        let result = tx2.commit_version(1, None).await;

        match result {
            Err(deltalake::DeltaTransactionError::VersionAlreadyExists { .. }) => {
                assert!(true, "Delta version already exists.");
            }
            _ => {
                assert!(false, "Delta version should already exist.");
            }
        }

        assert!(result.is_err());
        assert_eq!(1, table.version);
        assert_eq!(2, table.get_files().len());
    }
}

async fn test_two_commits(table_path: &str) -> Result<(), DeltaTransactionError> {
    let mut table = deltalake::open_table(table_path).await?;

    assert_eq!(0, table.version);
    assert_eq!(0, table.get_files().len());

    let mut tx1 = table.create_transaction(None);
    tx1.add_actions(tx1_actions());
    let version = tx1.commit(None).await?;

    assert_eq!(1, version);
    assert_eq!(version, table.version);
    assert_eq!(2, table.get_files().len());

    let mut tx2 = table.create_transaction(None);
    tx2.add_actions(tx2_actions());
    let version = tx2.commit(None).await.unwrap();

    assert_eq!(2, version);
    assert_eq!(version, table.version);
    assert_eq!(4, table.get_files().len());
    Ok(())
}

fn tx1_actions() -> Vec<action::Action> {
    vec![
        action::Action::add(action::Add {
            path: String::from(
                "part-00000-b44fcdb0-8b06-4f3a-8606-f8311a96f6dc-c000.snappy.parquet",
            ),
            size: 396,
            partitionValues: HashMap::new(),
            partitionValues_parsed: None,
            modificationTime: 1564524294000,
            dataChange: true,
            stats: None,
            stats_parsed: None,
            tags: None,
        }),
        action::Action::add(action::Add {
            path: String::from(
                "part-00001-185eca06-e017-4dea-ae49-fc48b973e37e-c000.snappy.parquet",
            ),
            size: 400,
            partitionValues: HashMap::new(),
            partitionValues_parsed: None,
            modificationTime: 1564524294000,
            dataChange: true,
            stats: None,
            stats_parsed: None,
            tags: None,
        }),
    ]
}

fn tx2_actions() -> Vec<action::Action> {
    vec![
        action::Action::add(action::Add {
            path: String::from(
                "part-00000-512e1537-8aaa-4193-b8b4-bef3de0de409-c000.snappy.parquet",
            ),
            size: 396,
            partitionValues: HashMap::new(),
            partitionValues_parsed: None,
            modificationTime: 1564524296000,
            dataChange: true,
            stats: None,
            stats_parsed: None,
            tags: None,
        }),
        action::Action::add(action::Add {
            path: String::from(
                "part-00001-4327c977-2734-4477-9507-7ccf67924649-c000.snappy.parquet",
            ),
            size: 400,
            partitionValues: HashMap::new(),
            partitionValues_parsed: None,
            modificationTime: 1564524296000,
            dataChange: true,
            stats: None,
            stats_parsed: None,
            tags: None,
        }),
    ]
}

fn prepare_fs() {
    fs_common::cleanup_dir_except(
        "./tests/data/simple_commit/_delta_log",
        vec!["00000000000000000000.json".to_string()],
    );
}
