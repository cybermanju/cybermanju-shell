// CyberManju OS — Search (crates/tests/src/search.rs).
// Role: workspace integration tests.
// Exports: module root (see items below).
// Integrates: cybermanju_search.
// Contracts: none special (pure logic; errors bubble to caller).
use cybermanju_search::{SearchIndex, SearchRequest};

#[test]
fn test_search_index_creation() {
    let dir = tempfile::tempdir().unwrap();
    let idx = SearchIndex::new(dir.path().to_str().unwrap()).unwrap();
    assert_eq!(idx.doc_count().unwrap(), 0);
}

#[test]
fn test_search_index_add_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let idx = SearchIndex::new(dir.path().to_str().unwrap()).unwrap();

    idx.add_document(
        "f1",
        "test.rs",
        "fn main() { println!(\"hello\"); }",
        &["rust".into(), "test".into()],
        "file",
        false,
        false,
        "2026-01-01T00:00:00Z",
        None,
    )
    .unwrap();
    idx.commit().unwrap();

    assert_eq!(idx.doc_count().unwrap(), 1);

    let req = SearchRequest {
        query: "main".into(),
        limit: Some(10),
        offset: None,
    };
    let results = idx.search(&req).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].file_id, "f1");
}

#[test]
fn test_search_index_remove_document() {
    let dir = tempfile::tempdir().unwrap();
    let idx = SearchIndex::new(dir.path().to_str().unwrap()).unwrap();

    idx.add_document(
        "f1",
        "deleteme.txt",
        "this will be deleted",
        &[],
        "file",
        false,
        false,
        "2026-01-01T00:00:00Z",
        None,
    )
    .unwrap();
    idx.commit().unwrap();

    assert_eq!(idx.doc_count().unwrap(), 1);

    idx.remove_document("f1").unwrap();
    idx.commit().unwrap();

    assert_eq!(idx.doc_count().unwrap(), 0);
}

#[test]
fn test_search_empty_query() {
    let dir = tempfile::tempdir().unwrap();
    let idx = SearchIndex::new(dir.path().to_str().unwrap()).unwrap();
    let req = SearchRequest {
        query: "".into(),
        limit: Some(10),
        offset: None,
    };
    let results = idx.search(&req).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_search_no_results() {
    let dir = tempfile::tempdir().unwrap();
    let idx = SearchIndex::new(dir.path().to_str().unwrap()).unwrap();

    idx.add_document(
        "f1",
        "readme.md",
        "This is a readme file",
        &["docs".into()],
        "file",
        false,
        false,
        "2026-01-01T00:00:00Z",
        None,
    )
    .unwrap();
    idx.commit().unwrap();

    let req = SearchRequest {
        query: "xyznonexistent".into(),
        limit: Some(10),
        offset: None,
    };
    let results = idx.search(&req).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_suggest() {
    let dir = tempfile::tempdir().unwrap();
    let idx = SearchIndex::new(dir.path().to_str().unwrap()).unwrap();

    idx.add_document(
        "f1",
        "photo.jpg",
        "",
        &["photo".into()],
        "file",
        false,
        false,
        "2026-01-01T00:00:00Z",
        None,
    )
    .unwrap();
    idx.commit().unwrap();

    let suggestions = idx.suggest("pho", 10).unwrap();
    assert!(!suggestions.is_empty());
}

#[test]
fn test_multiple_documents() {
    let dir = tempfile::tempdir().unwrap();
    let idx = SearchIndex::new(dir.path().to_str().unwrap()).unwrap();

    for i in 0..5 {
        idx.add_document(
            &format!("f{}", i),
            &format!("file_{}.txt", i),
            "content with unique term alphaomega",
            &[],
            "file",
            false,
            false,
            "2026-01-01T00:00:00Z",
            None,
        )
        .unwrap();
    }
    idx.commit().unwrap();

    assert_eq!(idx.doc_count().unwrap(), 5);

    let req = SearchRequest {
        query: "alphaomega".into(),
        limit: Some(10),
        offset: None,
    };
    let results = idx.search(&req).unwrap();
    assert_eq!(results.len(), 5);
}
