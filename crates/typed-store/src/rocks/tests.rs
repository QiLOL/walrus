// Copyright (c) Walrus Foundation
// SPDX-License-Identifier: Apache-2.0

use rstest::rstest;

use super::*;
use crate::{
    rocks::safe_iter::{SafeIter, SafeRevIter},
    traits::SeekableIterator,
};

fn temp_dir() -> std::path::PathBuf {
    tempfile::tempdir()
        .expect("Failed to open temporary directory")
        .keep()
}

enum TestIteratorWrapper<'a, K, V> {
    SafeIter(SafeIter<'a, K, V>),
}

// Implement Iterator for TestIteratorWrapper that returns the same type
// result for different types of Iterator.
// For non-safe Iterator, it returns the key value pair. For SafeIterator,
// it consumes the result (assuming no error),
// and return they key value pairs.
impl<K: DeserializeOwned, V: DeserializeOwned> Iterator for TestIteratorWrapper<'_, K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            TestIteratorWrapper::SafeIter(iter) => iter.next().map(|result| result.unwrap()),
        }
    }
}

impl<K: DeserializeOwned + Serialize, V: DeserializeOwned> SeekableIterator<K>
    for TestIteratorWrapper<'_, K, V>
{
    fn seek_to_first(&mut self) {
        match self {
            TestIteratorWrapper::SafeIter(iter) => iter.seek_to_first(),
        }
    }

    fn seek_to_last(&mut self) {
        match self {
            TestIteratorWrapper::SafeIter(iter) => iter.seek_to_last(),
        }
    }

    fn seek(&mut self, key: &K) -> Result<(), TypedStoreError> {
        match self {
            TestIteratorWrapper::SafeIter(iter) => iter.seek(key),
        }
    }

    fn seek_to_prev(&mut self, key: &K) -> Result<(), TypedStoreError> {
        match self {
            TestIteratorWrapper::SafeIter(iter) => iter.seek_to_prev(key),
        }
    }

    fn key(&self) -> Result<Option<K>, TypedStoreError> {
        match self {
            TestIteratorWrapper::SafeIter(iter) => iter.key(),
        }
    }
}

fn get_iter<K, V>(db: &DBMap<K, V>) -> TestIteratorWrapper<'_, K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    TestIteratorWrapper::SafeIter(db.safe_iter().expect("failed to get iterator"))
}

fn get_reverse_iter<K, V>(
    db: &DBMap<K, V>,
    lower_bound: Option<K>,
    upper_bound: Option<K>,
) -> SafeRevIter<'_, K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    db.reversed_safe_iter_with_bounds(lower_bound, upper_bound)
        .unwrap()
}

fn get_iter_with_bounds<K, V>(
    db: &DBMap<K, V>,
    lower_bound: Option<K>,
    upper_bound: Option<K>,
) -> TestIteratorWrapper<'_, K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    TestIteratorWrapper::SafeIter(
        db.safe_iter_with_bounds(lower_bound, upper_bound)
            .expect("failed to get iterator"),
    )
}

fn get_range_iter<K, V>(
    db: &DBMap<K, V>,
    range: impl RangeBounds<K>,
) -> TestIteratorWrapper<'_, K, V>
where
    K: Serialize + DeserializeOwned,
    V: Serialize + DeserializeOwned,
{
    TestIteratorWrapper::SafeIter(db.safe_range_iter(range).expect("failed to get iterator"))
}

#[tokio::test]
async fn test_open() {
    let _db = open_map::<_, u32, String>(temp_dir(), None);
}

#[tokio::test]
async fn test_reopen() {
    let arc = {
        let db = open_map::<_, u32, String>(temp_dir(), None);
        db.insert(&123456789, &"123456789".to_string())
            .expect("Failed to insert");
        db
    };
    let db = DBMap::<u32, String>::reopen(&arc.rocksdb, None, &ReadWriteOptions::default(), false)
        .expect("Failed to re-open storage");
    assert!(
        db.contains_key(&123456789)
            .expect("Failed to retrieve item in storage")
    );
}

#[tokio::test]
async fn test_wrong_reopen() {
    let rocks = open_rocksdb(temp_dir(), &["foo", "bar", "baz"]);
    let db = DBMap::<u8, u8>::reopen(&rocks, Some("quux"), &ReadWriteOptions::default(), false);
    assert!(db.is_err());
}

#[tokio::test]
async fn test_contains_key() {
    let db = open_map(temp_dir(), None);

    db.insert(&123456789, &"123456789".to_string())
        .expect("Failed to insert");
    assert!(
        db.contains_key(&123456789)
            .expect("Failed to call contains key")
    );
    assert!(
        !db.contains_key(&000000000)
            .expect("Failed to call contains key")
    );
}

#[tokio::test]
async fn test_multi_contain() {
    let db = open_map(temp_dir(), None);

    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");
    db.insert(&789, &"789".to_string())
        .expect("Failed to insert");

    let result = db
        .multi_contains_keys([123, 456])
        .expect("Failed to check multi keys existence");

    assert_eq!(result.len(), 2);
    assert!(result[0]);
    assert!(result[1]);

    let result = db
        .multi_contains_keys([123, 987, 789])
        .expect("Failed to check multi keys existence");

    assert_eq!(result.len(), 3);
    assert!(result[0]);
    assert!(!result[1]);
    assert!(result[2]);
}

#[tokio::test]
async fn test_get() {
    let db = open_map(temp_dir(), None);

    db.insert(&123456789, &"123456789".to_string())
        .expect("Failed to insert");
    assert_eq!(
        Some("123456789".to_string()),
        db.get(&123456789).expect("Failed to get")
    );
    assert_eq!(None, db.get(&000000000).expect("Failed to get"));
}

#[tokio::test]
async fn test_multi_get() {
    let db = open_map(temp_dir(), None);

    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");

    let result = db.multi_get([123, 456, 789]).expect("Failed to multi get");

    assert_eq!(result.len(), 3);
    assert_eq!(result[0], Some("123".to_string()));
    assert_eq!(result[1], Some("456".to_string()));
    assert_eq!(result[2], None);
}

#[tokio::test]
async fn test_skip() {
    let db = open_map(temp_dir(), None);

    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");
    db.insert(&789, &"789".to_string())
        .expect("Failed to insert");

    // Skip all smaller
    let key_vals: Vec<_> = get_iter_with_bounds(&db, Some(456), None).collect();
    assert_eq!(key_vals.len(), 2);
    assert_eq!(key_vals[0], (456, "456".to_string()));
    assert_eq!(key_vals[1], (789, "789".to_string()));

    // Skip to the end
    assert_eq!(get_iter_with_bounds(&db, Some(999), None).count(), 0);

    // Skip to last
    assert_eq!(
        get_reverse_iter(&db, None, None).next(),
        Some(Ok((789, "789".to_string()))),
    );

    // Skip to successor of first value
    assert_eq!(get_iter_with_bounds(&db, Some(000), None).count(), 3);
    assert_eq!(get_iter_with_bounds(&db, Some(000), None).count(), 3);
}

#[tokio::test]
async fn test_reverse_iter_with_bounds() {
    let db = open_map(temp_dir(), None);
    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");
    db.insert(&789, &"789".to_string())
        .expect("Failed to insert");

    let mut iter = get_reverse_iter(&db, None, Some(999));
    assert_eq!(iter.next().unwrap(), Ok((789, "789".to_string())));

    db.insert(&999, &"999".to_string())
        .expect("Failed to insert");
    let mut iter = get_reverse_iter(&db, None, Some(999));
    assert_eq!(iter.next().unwrap(), Ok((999, "999".to_string())));

    let mut iter = get_reverse_iter(&db, None, None);
    assert_eq!(iter.next().unwrap(), Ok((999, "999".to_string())));
}

#[tokio::test]
async fn test_remove() {
    let db = open_map(temp_dir(), None);

    db.insert(&123456789, &"123456789".to_string())
        .expect("Failed to insert");
    assert!(db.get(&123456789).expect("Failed to get").is_some());

    db.remove(&123456789).expect("Failed to remove");
    assert!(db.get(&123456789).expect("Failed to get").is_none());
}

#[tokio::test]
async fn test_iter() {
    let db = open_map(temp_dir(), None);
    db.insert(&123456789, &"123456789".to_string())
        .expect("Failed to insert");
    db.insert(&987654321, &"987654321".to_string())
        .expect("Failed to insert");

    let mut iter = get_iter(&db);

    assert_eq!(Some((123456789, "123456789".to_string())), iter.next());
    assert_eq!(Some((987654321, "987654321".to_string())), iter.next());
    assert_eq!(None, iter.next());
}

#[tokio::test]
async fn test_iter_reverse() {
    let db = open_map(temp_dir(), None);

    db.insert(&1, &"1".to_string()).expect("Failed to insert");
    db.insert(&2, &"2".to_string()).expect("Failed to insert");
    db.insert(&3, &"3".to_string()).expect("Failed to insert");

    let mut iter = get_reverse_iter(&db, None, None);
    assert_eq!(Some(Ok((3, "3".to_string()))), iter.next());
    assert_eq!(Some(Ok((2, "2".to_string()))), iter.next());
    assert_eq!(Some(Ok((1, "1".to_string()))), iter.next());
    assert_eq!(None, iter.next());

    let mut iter = get_iter_with_bounds(&db, Some(1), None);
    assert_eq!(Some((1, "1".to_string())), iter.next());
    assert_eq!(Some((2, "2".to_string())), iter.next());
}

#[tokio::test]
async fn test_insert_batch() {
    let db = open_map(temp_dir(), None);
    let keys_vals = (1..100).map(|i| (i, i.to_string()));
    let mut insert_batch = db.batch();
    insert_batch
        .insert_batch(&db, keys_vals.clone())
        .expect("Failed to batch insert");
    insert_batch.write().expect("Failed to execute batch");
    for (k, v) in keys_vals {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
}

#[tokio::test]
async fn test_insert_batch_across_cf() {
    let rocks = open_rocksdb(temp_dir(), &["First_CF", "Second_CF"]);

    let db_cf_1 = DBMap::reopen(
        &rocks,
        Some("First_CF"),
        &ReadWriteOptions::default(),
        false,
    )
    .expect("Failed to open storage");
    let keys_vals_1 = (1..100).map(|i| (i, i.to_string()));

    let db_cf_2 = DBMap::reopen(
        &rocks,
        Some("Second_CF"),
        &ReadWriteOptions::default(),
        false,
    )
    .expect("Failed to open storage");
    let keys_vals_2 = (1000..1100).map(|i| (i, i.to_string()));

    let mut batch = db_cf_1.batch();
    batch
        .insert_batch(&db_cf_1, keys_vals_1.clone())
        .expect("Failed to batch insert")
        .insert_batch(&db_cf_2, keys_vals_2.clone())
        .expect("Failed to batch insert");

    batch.write().expect("Failed to execute batch");
    for (k, v) in keys_vals_1 {
        let val = db_cf_1.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }

    for (k, v) in keys_vals_2 {
        let val = db_cf_2.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
}

#[tokio::test]
async fn test_insert_batch_across_different_db() {
    let rocks = open_rocksdb(temp_dir(), &["First_CF", "Second_CF"]);
    let rocks2 = open_rocksdb(temp_dir(), &["First_CF", "Second_CF"]);

    let db_cf_1: DBMap<i32, String> = DBMap::reopen(
        &rocks,
        Some("First_CF"),
        &ReadWriteOptions::default(),
        false,
    )
    .expect("Failed to open storage");
    let keys_vals_1 = (1..100).map(|i| (i, i.to_string()));

    let db_cf_2: DBMap<i32, String> = DBMap::reopen(
        &rocks2,
        Some("Second_CF"),
        &ReadWriteOptions::default(),
        false,
    )
    .expect("Failed to open storage");
    let keys_vals_2 = (1000..1100).map(|i| (i, i.to_string()));

    assert!(
        db_cf_1
            .batch()
            .insert_batch(&db_cf_1, keys_vals_1)
            .expect("Failed to batch insert")
            .insert_batch(&db_cf_2, keys_vals_2)
            .is_err()
    );
}

#[tokio::test]
async fn test_delete_batch() {
    let db = DBMap::<i32, String>::open(
        temp_dir(),
        MetricConf::default(),
        None,
        None,
        &ReadWriteOptions::default(),
    )
    .expect("Failed to open storage");

    let keys_vals = (1..100).map(|i| (i, i.to_string()));
    let mut batch = db.batch();
    batch
        .insert_batch(&db, keys_vals)
        .expect("Failed to batch insert");

    // delete the odd-index keys
    let deletion_keys = (1..100).step_by(2);
    batch
        .delete_batch(&db, deletion_keys)
        .expect("Failed to batch delete");

    batch.write().expect("Failed to execute batch");

    for (k, _) in get_iter(&db) {
        assert_eq!(k % 2, 0);
    }
}

#[tokio::test]
async fn test_delete_range() {
    let db: DBMap<i32, String> = DBMap::open(
        temp_dir(),
        MetricConf::default(),
        None,
        None,
        &ReadWriteOptions::default().set_ignore_range_deletions(false),
    )
    .expect("Failed to open storage");

    // Note that the last element is (100, "100".to_owned()) here
    let keys_vals = (0..101).map(|i| (i, i.to_string()));
    let mut batch = db.batch();
    batch
        .insert_batch(&db, keys_vals)
        .expect("Failed to batch insert");

    batch
        .schedule_delete_range(&db, &50, &100)
        .expect("Failed to delete range");

    batch.write().expect("Failed to execute batch");

    for k in 0..50 {
        assert!(db.contains_key(&k).expect("Failed to query legal key"),);
    }
    for k in 50..100 {
        assert!(!db.contains_key(&k).expect("Failed to query legal key"));
    }

    // range operator is not inclusive of to
    assert!(db.contains_key(&100).expect("Failed to query legal key"));
}

#[tokio::test]
async fn test_clear() {
    let db = DBMap::<i32, String>::open(
        temp_dir(),
        MetricConf::default(),
        None,
        Some("table"),
        &ReadWriteOptions::default(),
    )
    .expect("Failed to open storage");
    // Test clear of empty map
    let _ = db.unsafe_clear();

    let keys_vals = (0..101).map(|i| (i, i.to_string()));
    let mut insert_batch = db.batch();
    insert_batch
        .insert_batch(&db, keys_vals)
        .expect("Failed to batch insert");

    insert_batch.write().expect("Failed to execute batch");

    // Check we have multiple entries
    assert!(db.safe_iter().expect("failed to get iterator").count() > 1);
    let _ = db.unsafe_clear();
    assert_eq!(db.safe_iter().expect("failed to get iterator").count(), 0);
    // Clear again to ensure safety when clearing empty map
    let _ = db.unsafe_clear();
    assert_eq!(db.safe_iter().expect("failed to get iterator").count(), 0);
    // Clear with one item
    let _ = db.insert(&1, &"e".to_string());
    assert_eq!(db.safe_iter().expect("failed to get iterator").count(), 1);
    let _ = db.unsafe_clear();
    assert_eq!(db.safe_iter().expect("failed to get iterator").count(), 0);
}

#[tokio::test]
async fn test_iter_with_bounds() {
    let db = open_map(temp_dir(), None);

    // Add [1, 50) and (50, 100) in the db
    for i in 1..100 {
        if i != 50 {
            db.insert(&i, &i.to_string()).unwrap();
        }
    }

    // Tests basic bounded scan.
    let db_iter = get_iter_with_bounds(&db, Some(20), Some(90));
    assert_eq!(
        (20..50)
            .chain(51..90)
            .map(|i| (i, i.to_string()))
            .collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Don't specify upper bound.
    let db_iter = get_iter_with_bounds(&db, Some(20), None);
    assert_eq!(
        (20..50)
            .chain(51..100)
            .map(|i| (i, i.to_string()))
            .collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Don't specify lower bound.
    let db_iter = get_iter_with_bounds(&db, None, Some(90));
    assert_eq!(
        (1..50)
            .chain(51..90)
            .map(|i| (i, i.to_string()))
            .collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Don't specify any bounds.
    let db_iter = get_iter_with_bounds(&db, None, None);
    assert_eq!(
        (1..50)
            .chain(51..100)
            .map(|i| (i, i.to_string()))
            .collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Specify a bound outside of dataset.
    let db_iter = db
        .safe_iter_with_bounds(Some(200), Some(300))
        .expect("failed to get iterator");
    assert!(db_iter.collect::<Vec<_>>().is_empty());

    // Skip to first key in the bound (bound is [1, 50))
    let db_iter = get_iter_with_bounds(&db, Some(1), Some(50));
    assert_eq!(
        (1..50).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );
}

#[rstest]
#[tokio::test]
async fn test_range_iter() {
    let db = open_map(temp_dir(), None);

    // Add [1, 50) and (50, 100) in the db
    for i in 1..100 {
        if i != 50 {
            db.insert(&i, &i.to_string()).unwrap();
        }
    }

    // Tests basic range iterating with inclusive end.
    let db_iter = get_range_iter(&db, 10..=20);
    assert_eq!(
        (10..21).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Tests range with min start and exclusive end.
    let db_iter = get_range_iter(&db, ..20);
    assert_eq!(
        (1..20).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Tests range with max end.
    let db_iter = get_range_iter(&db, 60..);
    assert_eq!(
        (60..100).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );

    // Skip to first key in the bound (bound is [1, 49))
    let db_iter = get_range_iter(&db, 1..49);
    assert_eq!(
        (1..49).map(|i| (i, i.to_string())).collect::<Vec<_>>(),
        db_iter.collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_is_empty() {
    let db = DBMap::<i32, String>::open(
        temp_dir(),
        MetricConf::default(),
        None,
        Some("table"),
        &ReadWriteOptions::default(),
    )
    .expect("Failed to open storage");

    // Test empty map is truly empty
    assert!(db.is_empty());
    let _ = db.unsafe_clear();
    assert!(db.is_empty());

    let keys_vals = (0..101).map(|i| (i, i.to_string()));
    let mut insert_batch = db.batch();
    insert_batch
        .insert_batch(&db, keys_vals)
        .expect("Failed to batch insert");

    insert_batch.write().expect("Failed to execute batch");

    // Check we have multiple entries and not empty
    assert!(db.safe_iter().expect("failed to get iterator").count() > 1);
    assert!(!db.is_empty());

    // Clear again to ensure empty works after clearing
    let _ = db.unsafe_clear();
    assert_eq!(db.safe_iter().expect("failed to get iterator").count(), 0);
    assert!(db.is_empty());
}

#[tokio::test]
async fn test_multi_insert() {
    // Init a DB
    let db: DBMap<i32, String> = open_map(temp_dir(), Some("table"));
    // Create kv pairs
    let keys_vals = (0..101).map(|i| (i, i.to_string()));

    db.multi_insert(keys_vals.clone())
        .expect("Failed to multi-insert");

    for (k, v) in keys_vals {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
}

#[tokio::test]
async fn test_checkpoint() {
    let path_prefix = temp_dir();
    let db_path = path_prefix.join("db");
    let db: DBMap<i32, String> = open_map(db_path, Some("table"));
    // Create kv pairs
    let keys_vals = (0..101).map(|i| (i, i.to_string()));

    db.multi_insert(keys_vals.clone())
        .expect("Failed to multi-insert");
    let checkpointed_path = path_prefix.join("checkpointed_db");
    db.rocksdb
        .checkpoint(&checkpointed_path)
        .expect("Failed to create db checkpoint");
    // Create more kv pairs
    let new_keys_vals = (101..201).map(|i| (i, i.to_string()));
    db.multi_insert(new_keys_vals.clone())
        .expect("Failed to multi-insert");
    // Verify checkpoint
    let checkpointed_db: DBMap<i32, String> = open_map(checkpointed_path, Some("table"));
    // Ensure keys inserted before checkpoint are present in original and checkpointed db
    for (k, v) in keys_vals {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v.clone()), val);
        let val = checkpointed_db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
    // Ensure keys inserted after checkpoint are only present in original db but not
    // in checkpointed db
    for (k, v) in new_keys_vals {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v.clone()), val);
        let val = checkpointed_db.get(&k).expect("Failed to get inserted key");
        assert_eq!(None, val);
    }
}

#[tokio::test]
async fn test_multi_remove() {
    // Init a DB
    let db: DBMap<i32, String> = open_map(temp_dir(), Some("table"));

    // Create kv pairs
    let keys_vals = (0..101).map(|i| (i, i.to_string()));

    db.multi_insert(keys_vals.clone())
        .expect("Failed to multi-insert");

    // Check insertion
    for (k, v) in keys_vals.clone() {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }

    // Remove 50 items
    db.multi_remove(keys_vals.clone().map(|kv| kv.0).take(50))
        .expect("Failed to multi-remove");
    assert_eq!(
        db.safe_iter().expect("failed to get iterator").count(),
        101 - 50
    );

    // Check that the remaining are present
    for (k, v) in keys_vals.skip(50) {
        let val = db.get(&k).expect("Failed to get inserted key");
        assert_eq!(Some(v), val);
    }
}

fn open_map<P: AsRef<Path>, K, V>(path: P, opt_cf: Option<&str>) -> DBMap<K, V> {
    DBMap::<K, V>::open(
        path,
        MetricConf::default(),
        None,
        opt_cf,
        &ReadWriteOptions::default(),
    )
    .expect("failed to open rocksdb")
}

fn open_rocksdb<P: AsRef<Path>>(path: P, opt_cfs: &[&str]) -> Arc<RocksDB> {
    open_cf(path, None, MetricConf::default(), opt_cfs).expect("failed to open rocksdb")
}

#[tokio::test]
async fn test_sampling() {
    let sampling_interval = SamplingInterval::new(Duration::ZERO, 10);
    for _i in 0..10 {
        assert!(!sampling_interval.sample());
    }
    assert!(sampling_interval.sample());
    for _i in 0..10 {
        assert!(!sampling_interval.sample());
    }
    assert!(sampling_interval.sample());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_sampling_time() {
    let sampling_interval = SamplingInterval::new(Duration::from_secs(1), 10);
    for _i in 0..10 {
        assert!(!sampling_interval.sample());
    }
    assert!(!sampling_interval.sample());
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(sampling_interval.sample());
    for _i in 0..10 {
        assert!(!sampling_interval.sample());
    }
    assert!(!sampling_interval.sample());
    tokio::time::advance(Duration::from_secs(1)).await;
    tokio::task::yield_now().await;
    assert!(sampling_interval.sample());
}

#[tokio::test]
async fn test_iterator_seek() {
    let db: DBMap<u32, String> = open_map(temp_dir(), None);

    db.insert(&123, &"123".to_string())
        .expect("Failed to insert");
    db.insert(&456, &"456".to_string())
        .expect("Failed to insert");
    db.insert(&789, &"789".to_string())
        .expect("Failed to insert");

    let mut iter = get_iter(&db);

    assert!(matches!(
        iter.key().unwrap_err(),
        TypedStoreError::IteratorNotInitialized
    ));

    iter.seek(&0).unwrap();
    assert_eq!(iter.key().unwrap(), Some(123));
    assert_eq!(
        iter.by_ref().collect::<Vec<_>>(),
        vec![
            (123, "123".to_string()),
            (456, "456".to_string()),
            (789, "789".to_string())
        ]
    );

    iter.seek(&123).unwrap();
    assert_eq!(iter.key().unwrap(), Some(123));

    iter.seek(&234).unwrap();
    assert_eq!(iter.key().unwrap(), Some(456));
    assert_eq!(
        iter.by_ref().collect::<Vec<_>>(),
        vec![(456, "456".to_string()), (789, "789".to_string())]
    );
    assert_eq!(iter.key().unwrap(), None);

    iter.seek(&567).unwrap();
    assert_eq!(iter.key().unwrap(), Some(789));

    iter.seek_to_prev(&234).unwrap();
    assert_eq!(iter.key().unwrap(), Some(123));

    iter.seek_to_prev(&123).unwrap();
    assert_eq!(iter.key().unwrap(), Some(123));

    iter.seek_to_prev(&122).unwrap();
    assert_eq!(iter.key().unwrap(), None);

    iter.seek(&789).unwrap();
    assert_eq!(iter.key().unwrap(), Some(789));

    iter.seek(&890).unwrap();
    assert_eq!(iter.key().unwrap(), None);

    iter.seek_to_last();
    assert_eq!(iter.key().unwrap(), Some(789));

    iter.seek_to_first();
    assert_eq!(iter.key().unwrap(), Some(123));

    iter.seek_to_last();
    assert_eq!(iter.key().unwrap(), Some(789));
}
